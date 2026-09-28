//! ex-308: stored against measured text widths across the whole ex-003
//! corpus, the 232 catalogue libraries of `fixtures/libraries` and the
//! scene-bearing files of upstream's test fixtures (`fixtures/upstream`),
//! both as `fixtures/manifest.json` lists them.
//!
//! Upstream stores each text element's `width` and `height` as the writing
//! browser measured them: `measureText(text, getFontString(element),
//! lineHeight)` (`packages/element/src/textMeasurements.ts:12-27`), called
//! through `refreshTextDimensions` and `getAdjustedDimensions`
//! (`packages/element/src/newElement.ts:533-580`). The port measures every
//! text of a vendored family the same way with [`FontStore`] and compares.
//!
//! Every text of a vendored family falls in one class, decided without
//! looking at its width:
//!
//! - `measured`: has `lineHeight` and `autoResize` is not false, and its
//!   stored `height` is what `measureText` gives, `fontSize * lineHeight *
//!   lines` (`getTextHeight`, `textMeasurements.ts:170-177`), within the
//!   tolerance. Its `width` is a browser measurement of the same text.
//! - `height_mismatch`: has `lineHeight` and `autoResize`, but a `height`
//!   `measureText` cannot give, so the element was not written by
//!   upstream's measurement (a generator wrote it); its width is reported,
//!   not gated.
//! - `legacy`: no `lineHeight`, written before upstream measured on a
//!   canvas with a unitless line height; restore detects one from the
//!   height (`packages/excalidraw/data/restore.ts:549-560`). Reported, not
//!   gated.
//! - `fixed_width`: `autoResize: false`, whose `width` is the width the
//!   user wrapped to, not a measurement. Counted.
//!
//! The report, `tests/fixtures/text-width-corpus-report.json`, gives the
//! count, the number within 0.5 px, and the max and mean |measured -
//! stored| per family and class, and this test rebuilds and compares it
//! (`EXCALI_BLESS=1` rewrites it after a deliberate change; the diff is the
//! review). For Excalifont, Nunito and Comic Shanns (ADR-007's three
//! families) every `measured` text must be within 0.5 px, and so must the
//! family's mean; the only exceptions are the texts of
//! [`KNOWN_DEVIATIONS`], each pinned to its stored and measured width.

mod common;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use excali_core::element::FontFamily;
use excali_core::png::decode_png_metadata;
use excali_core::svg_payload::decode_svg_base64_payload;
use excali_text::font_metadata::get_font_string;
use excali_text::text_measurements::measure_text;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const TOLERANCE: f64 = 0.5;
const REPORT: &str = "tests/fixtures/text-width-corpus-report.json";

/// Excalifont, Nunito, Comic Shanns: the families whose `measured` texts
/// are held to [`TOLERANCE`] (ADR-007).
const GATED_FAMILIES: [u32; 3] = [5, 6, 8];

/// The texts of the gated families that are `measured` by class and still
/// deviate by more than 0.5 px: `(file under fixtures/, element id, stored
/// width, measured width)`.
///
/// Nothing the port can reproduce gives these widths. Each was measured
/// again (2026-09-28) in every vendored family at its font size, in its
/// own family without kerning, and with the Excalifont and Comic Shanns
/// builds upstream shipped before `61623bbeba` (2024-10-20; the single-file
/// `packages/excalidraw/fonts/assets/*-Regular.woff2` at `a80cb5896a`).
/// No family and neither older build comes within 0.5 px of the texts of
/// more than one character (for the single `K` and `7`, Liberation Sans
/// happens to), and the older Excalifont measures every corpus text exactly
/// as the vendored one does. Every stored width is wider than the
/// measurement, most other texts of the same libraries match to 0.001 px
/// (19 of 27 and 16 of 31 in the two Excalifont libraries with most of
/// these), `K` is stored exactly 1.000 px wider and `A` as a whole 25 px.
/// That fits a width kept from an earlier state of the element better than
/// a measuring error. A text that starts to
/// measure within 0.5 px must be removed from the list, and a new
/// deviation fails the gate.
const KNOWN_DEVIATIONS: &[(&str, &str, f64, f64)] = &[
    // Excalifont (5)
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "tSfxrqkItq3SZLhIbkX8I",
        10.807999610900879,
        9.808,
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "Vw9cbJ_YyXodALTOaKDJl",
        167.9038791656494,
        166.96,
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "ZtJ-xO5KFQFAKmKZw52Sk",
        101.34393501281738,
        100.32,
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "3hvYeGmB_6MXefzOj3ju5",
        167.9038791656494,
        166.96,
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "g3eA4wT2HZVZ3tORzoFoa",
        130.2078845500946,
        129.52,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "b13s8Yg3H6MAv5DP57EQu",
        112.87196350097656,
        111.42,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "4hdzG83Rxx1fldGChWrTe",
        123.52796936035156,
        122.076,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "bcj9TbntkjxAbnJT_A8Kq",
        153.6599578857422,
        151.74,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "MGLV7_xBOL0gssIKcdHJa",
        153.6599578857422,
        151.74,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "dmlv3N_9yOdMLcbvn0XXr",
        153.6599578857422,
        151.74,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "iZXwggIULIN6zn5T-JunY",
        329.6038818359375,
        329.076,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "ycFqvLAQT_EpoP5IbUbMZ",
        218.53192138671875,
        216.864,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "oLC62R0OYAZYsSLIqv04a",
        74.31198120117188,
        72.108,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "nANmHqm_Fgj1adE7oRI0H",
        212.419921875,
        211.752,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "bMz65OYUyPcq4ZMyzw3nX",
        209.21194458007812,
        208.044,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "ZNbPoQfsdlJcHOS4dgTd9",
        212.30393981933594,
        210.852,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "HcRiKbkgPSdROBEureNmK",
        25.0,
        24.336,
    ),
    // Comic Shanns (8)
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "19KTHBlJtvmGlR4K0BJ3U",
        11.519999623298645,
        11.0,
    ),
];

/// The libraries with `height_mismatch` texts and how many: Nunito texts
/// of a generated library, whose heights are
/// `fontSize * 1.4` under a stored `lineHeight` of 1.25 and whose widths
/// are `0.6 * fontSize` per character.
const HEIGHT_MISMATCH: &[(&str, u32, usize)] = &[(
    "libraries/datavizfairy/dashboard-charts.excalidrawlib.gz",
    6,
    328,
)];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Measured,
    HeightMismatch,
    Legacy,
    FixedWidth,
}

impl Class {
    fn key(self) -> &'static str {
        match self {
            Class::Measured => "measured",
            Class::HeightMismatch => "height_mismatch",
            Class::Legacy => "legacy",
            Class::FixedWidth => "fixed_width",
        }
    }
}

/// Which class a text of a vendored family is in, from everything but its
/// width. `measured_height` is `measureText`'s height for the text.
fn classify(element: &Value, measured_height: f64) -> Class {
    let line_height = element
        .get("lineHeight")
        .and_then(Value::as_f64)
        .filter(|lh| *lh > 0.0);
    if element.get("autoResize") == Some(&Value::Bool(false)) {
        return Class::FixedWidth;
    }
    if line_height.is_none() {
        return Class::Legacy;
    }
    let stored_height = element.get("height").and_then(Value::as_f64);
    match stored_height {
        Some(h) if (h - measured_height).abs() <= TOLERANCE => Class::Measured,
        _ => Class::HeightMismatch,
    }
}

/// One text element of the corpus in a vendored family.
#[derive(Debug)]
struct Text {
    /// The file under `fixtures/`.
    file: String,
    /// `upstream` or `libraries`.
    source: &'static str,
    item: usize,
    id: String,
    family: u32,
    font_size: f64,
    text: String,
    class: Class,
    stored: f64,
    measured: f64,
}

impl Text {
    fn deviation(&self) -> f64 {
        (self.measured - self.stored).abs()
    }
}

/// Corpus-wide counts of text elements the port does not measure.
#[derive(Debug, Default)]
struct Skipped {
    /// fontFamily -> texts, for families with no vendored faces.
    not_vendored: BTreeMap<String, usize>,
    /// Texts without a numeric `fontFamily`, `fontSize`, `width` or `text`.
    malformed: usize,
}

struct Corpus {
    texts: Vec<Text>,
    skipped: Skipped,
    library_files: usize,
    upstream_files: Vec<String>,
}

/// The elements of a library (`libraryItems` of v2 or `library` of v1) or
/// scene, with the index of the item they belong to (0 for a scene).
fn elements(doc: &Value) -> Vec<(usize, &Value)> {
    let mut out = Vec::new();
    if let Some(els) = doc.get("elements").and_then(Value::as_array) {
        out.extend(els.iter().map(|e| (0, e)));
    }
    let items = doc
        .get("libraryItems")
        .or_else(|| doc.get("library"))
        .and_then(Value::as_array);
    for (index, item) in items.into_iter().flatten().enumerate() {
        let els = match item {
            Value::Array(els) => Some(els),
            Value::Object(o) => o.get("elements").and_then(Value::as_array),
            _ => None,
        };
        out.extend(els.into_iter().flatten().map(|e| (index, e)));
    }
    out
}

/// The scene text an upstream fixture carries, `None` for files that carry
/// none: a library as is, the payload embedded in a PNG or SVG export.
fn upstream_scene(name: &str, bytes: &[u8]) -> Option<String> {
    if name.ends_with(".excalidrawlib") {
        Some(String::from_utf8(bytes.to_vec()).expect("library is UTF-8"))
    } else if name.ends_with(".png") {
        decode_png_metadata(bytes).ok().flatten()
    } else if name.ends_with(".svg") {
        decode_svg_base64_payload(std::str::from_utf8(bytes).ok()?).ok()
    } else {
        None
    }
}

fn corpus() -> &'static Corpus {
    static CORPUS: OnceLock<Corpus> = OnceLock::new();
    CORPUS.get_or_init(build_corpus)
}

fn build_corpus() -> Corpus {
    let store = common::store();
    let vendored: Vec<String> = store.families().map(str::to_owned).collect();
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("fixtures/manifest.json")).expect("manifest"),
    )
    .expect("manifest parses");
    let mut corpus = Corpus {
        texts: Vec::new(),
        skipped: Skipped::default(),
        library_files: 0,
        upstream_files: Vec::new(),
    };
    for entry in manifest["files"].as_array().expect("files") {
        let path = entry["path"].as_str().expect("path");
        let bytes = std::fs::read(repo().join("fixtures").join(path)).expect(path);
        assert_eq!(sha256(&bytes), entry["sha256"], "{path}");
        let (source, scene) = match entry["source"].as_str() {
            Some("libraries") if path.ends_with(".excalidrawlib.gz") => {
                let mut text = Vec::new();
                flate2::read::GzDecoder::new(&bytes[..])
                    .read_to_end(&mut text)
                    .expect("gzip");
                assert_eq!(sha256(&text), entry["content_sha256"], "{path}");
                corpus.library_files += 1;
                ("libraries", String::from_utf8(text).expect("UTF-8"))
            }
            Some("upstream") => match upstream_scene(path, &bytes) {
                Some(scene) => {
                    corpus.upstream_files.push(path.to_owned());
                    ("upstream", scene)
                }
                None => continue,
            },
            _ => continue,
        };
        let doc: Value = serde_json::from_str(&scene).expect(path);
        for (item, element) in elements(&doc) {
            if element.get("type").and_then(Value::as_str) != Some("text") {
                continue;
            }
            let fields = (
                element.get("fontFamily").and_then(Value::as_u64),
                element.get("fontSize").and_then(Value::as_f64),
                element.get("width").and_then(Value::as_f64),
                element.get("text").and_then(Value::as_str),
            );
            let (Some(family), Some(font_size), Some(stored), Some(text)) = fields else {
                corpus.skipped.malformed += 1;
                continue;
            };
            let family = u32::try_from(family).expect("fontFamily fits u32");
            let name = FontFamily(family).name();
            if !name.is_some_and(|n| vendored.iter().any(|v| v == n)) {
                *corpus
                    .skipped
                    .not_vendored
                    .entry(family.to_string())
                    .or_default() += 1;
                continue;
            }
            let font = get_font_string(font_size, FontFamily(family));
            let line_height = element
                .get("lineHeight")
                .and_then(Value::as_f64)
                .filter(|lh| *lh > 0.0)
                .unwrap_or(1.25);
            let dims = measure_text(text, &font, line_height, store);
            corpus.texts.push(Text {
                file: path.to_owned(),
                source,
                item,
                id: element["id"].as_str().unwrap_or_default().to_owned(),
                family,
                font_size,
                text: text.to_owned(),
                class: classify(element, dims.height),
                stored,
                measured: dims.width,
            });
        }
    }
    corpus
}

fn round(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

#[derive(Default)]
struct Stats {
    texts: usize,
    within: usize,
    max: f64,
    sum: f64,
}

impl Stats {
    fn add(&mut self, deviation: f64) {
        self.texts += 1;
        if deviation <= TOLERANCE {
            self.within += 1;
        }
        self.max = self.max.max(deviation);
        self.sum += deviation;
    }

    fn mean(&self) -> f64 {
        if self.texts == 0 {
            0.0
        } else {
            self.sum / self.texts as f64
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "texts": self.texts,
            "within_tolerance": self.within,
            "max_deviation_px": round(self.max),
            "mean_deviation_px": round(self.mean()),
        })
    }
}

/// `(fontFamily, class) -> stats` over `texts`.
fn stats<'a>(texts: impl Iterator<Item = &'a Text>) -> BTreeMap<(u32, Class), Stats> {
    let mut out: BTreeMap<(u32, Class), Stats> = BTreeMap::new();
    for t in texts {
        out.entry((t.family, t.class))
            .or_default()
            .add(t.deviation());
    }
    out
}

fn family_table(stats: &BTreeMap<(u32, Class), Stats>) -> Value {
    let mut families: BTreeMap<u32, Map<String, Value>> = BTreeMap::new();
    for ((family, class), s) in stats {
        let entry = families.entry(*family).or_insert_with(|| {
            let mut m = Map::new();
            m.insert("fontFamily".into(), json!(family));
            m.insert("name".into(), json!(FontFamily(*family).name()));
            m.insert("gated".into(), json!(GATED_FAMILIES.contains(family)));
            m
        });
        entry.insert(class.key().into(), s.to_json());
    }
    Value::Array(families.into_values().map(Value::Object).collect())
}

/// JSON text with every non-ASCII character escaped, so the report holds
/// no emoji variation selectors (the authorship gate rejects them).
fn ascii_json(value: &Value) -> String {
    let text = serde_json::to_string_pretty(value).expect("report") + "\n";
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii() {
            out.push(ch);
        } else {
            let mut units = [0u16; 2];
            for unit in ch.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

fn report() -> Value {
    let corpus = corpus();
    let all = stats(corpus.texts.iter());
    let upstream = stats(corpus.texts.iter().filter(|t| t.source == "upstream"));
    let upstream_texts: Vec<Value> = corpus
        .texts
        .iter()
        .filter(|t| t.source == "upstream")
        .map(|t| {
            json!({
                "file": t.file,
                "id": t.id,
                "fontFamily": t.family,
                "fontSize": t.font_size,
                "text": t.text,
                "class": t.class.key(),
                "stored_width": t.stored,
                "measured_width": round(t.measured),
            })
        })
        .collect();
    let known: Vec<Value> = KNOWN_DEVIATIONS
        .iter()
        .map(|(file, id, stored, measured)| {
            let t = find(file, id);
            json!({
                "file": file,
                "item": t.item,
                "id": id,
                "fontFamily": t.family,
                "fontSize": t.font_size,
                "text": t.text,
                "stored_width": stored,
                "measured_width": measured,
                "deviation_px": round(t.measured - t.stored),
            })
        })
        .collect();
    json!({
        "generator": "crates/excali-text/tests/text_width_corpus.rs",
        "description": "Stored against measured widths of every text element in a vendored \
    family across the ex-003 corpus (fixtures/manifest.json): the catalogue libraries and the \
    scene-bearing upstream test fixtures. deviation = |measured - stored| in px, measured with \
    excali_text's FontStore as measureText(text, getFontString(element), lineHeight). Classes \
    are decided without the width; see classes. Regenerate with EXCALI_BLESS=1 cargo test -p \
    excali-text --test text_width_corpus.",
        "tolerance_px": TOLERANCE,
        "gated_families": GATED_FAMILIES,
        "classes": {
            "measured": "lineHeight set, autoResize not false, stored height = fontSize * \
    lineHeight * lines (getTextHeight): a width measureText wrote; gated for the gated families",
            "height_mismatch": "lineHeight set and autoResize not false, but a stored height \
    measureText cannot give: not written by upstream's measurement; reported",
            "legacy": "no lineHeight: written before measureText used a unitless line \
    height; reported",
            "fixed_width": "autoResize false: width is the wrap width the user set, not a \
    measurement; counted",
        },
        "sources": {
            "library_files": corpus.library_files,
            "upstream_files": corpus.upstream_files,
        },
        "families": family_table(&all),
        "upstream_fixtures": {
            "families": family_table(&upstream),
            "texts": upstream_texts,
        },
        "skipped": {
            "not_vendored_by_fontFamily": corpus.skipped.not_vendored,
            "malformed": corpus.skipped.malformed,
        },
        "known_deviations": known,
    })
}

fn find(file: &str, id: &str) -> &'static Text {
    let matches: Vec<&Text> = corpus()
        .texts
        .iter()
        .filter(|t| t.file == file && t.id == id)
        .collect();
    assert_eq!(matches.len(), 1, "{file} {id}: expected exactly one text");
    matches[0]
}

#[test]
fn report_matches_the_committed_report() {
    let report = report();
    // The per-family table, for CI logs (`--nocapture`).
    for row in report["families"].as_array().expect("families") {
        println!("{row}");
    }
    let text = ascii_json(&report);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(REPORT);
    if std::env::var_os("EXCALI_BLESS").is_some() {
        std::fs::write(&path, &text).expect("write report");
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{REPORT}: {e} (run with EXCALI_BLESS=1 to write it)"));
    assert!(
        committed == text,
        "{REPORT} is out of date: run with EXCALI_BLESS=1 and review the diff"
    );
}

#[test]
fn corpus_is_every_library_and_every_scene_bearing_upstream_fixture() {
    let corpus = corpus();
    assert_eq!(corpus.library_files, 232);
    assert_eq!(
        corpus
            .upstream_files
            .iter()
            .map(|p| p.rsplit('/').next().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "fixture_library.excalidrawlib",
            "smiley_embedded_v2.png",
            "smiley_embedded_v2.svg",
            "test_embedded_v1.png",
            "test_embedded_v1.svg",
        ]
    );
    let mut per_class: BTreeMap<&str, usize> = BTreeMap::new();
    for t in &corpus.texts {
        *per_class.entry(t.class.key()).or_default() += 1;
    }
    // ex-302's baseline: 1866 texts with lineHeight and autoResize in the
    // vendored families, of which 328 have a height measureText cannot give.
    assert_eq!(
        per_class,
        BTreeMap::from([
            ("measured", 1538),
            ("height_mismatch", 328),
            ("legacy", 4403),
            ("fixed_width", 585),
        ])
    );
}

#[test]
fn gated_families_measure_within_half_a_pixel() {
    let corpus = corpus();
    let mut failures = Vec::new();
    for family in GATED_FAMILIES {
        let measured: Vec<&Text> = corpus
            .texts
            .iter()
            .filter(|t| t.family == family && t.class == Class::Measured)
            .collect();
        assert!(
            !measured.is_empty(),
            "fontFamily {family}: no measured texts"
        );
        let mean = measured.iter().map(|t| t.deviation()).sum::<f64>() / measured.len() as f64;
        if mean > TOLERANCE {
            failures.push(format!(
                "fontFamily {family}: mean deviation {mean} px over {} texts",
                measured.len()
            ));
        }
        for t in measured {
            let known = KNOWN_DEVIATIONS
                .iter()
                .any(|(file, id, _, _)| *file == t.file && *id == t.id);
            if t.deviation() > TOLERANCE && !known {
                failures.push(format!(
                    "{} item {} id {}: {:?} at {} px in fontFamily {family}: measured {} stored {}",
                    t.file, t.item, t.id, t.text, t.font_size, t.measured, t.stored
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} deviations beyond {TOLERANCE} px:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn known_deviations_still_deviate_as_recorded() {
    for (file, id, stored, measured) in KNOWN_DEVIATIONS {
        let t = find(file, id);
        assert!(
            GATED_FAMILIES.contains(&t.family),
            "{id}: not a gated family"
        );
        assert_eq!(t.class, Class::Measured, "{id}: not a measured text");
        assert_eq!(t.stored, *stored, "{id}: stored width");
        assert!(
            (t.measured - measured).abs() < 0.001,
            "{id}: measures {} now, recorded {measured}",
            t.measured
        );
        assert!(
            t.deviation() > TOLERANCE,
            "{id}: now within {TOLERANCE} px; remove it from KNOWN_DEVIATIONS"
        );
        // Every one is a stored width wider than the text measures.
        assert!(t.stored > t.measured, "{id}");
    }
    let mut ids: Vec<&str> = KNOWN_DEVIATIONS.iter().map(|k| k.1).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), KNOWN_DEVIATIONS.len(), "an id is listed twice");
}

#[test]
fn only_the_generated_library_has_heights_measure_text_cannot_give() {
    let mut found: BTreeMap<(&str, u32), usize> = BTreeMap::new();
    for t in corpus()
        .texts
        .iter()
        .filter(|t| t.class == Class::HeightMismatch)
    {
        *found.entry((t.file.as_str(), t.family)).or_default() += 1;
    }
    let expected: BTreeMap<(&str, u32), usize> = HEIGHT_MISMATCH
        .iter()
        .map(|(file, family, n)| ((*file, *family), *n))
        .collect();
    assert_eq!(found, expected);
}

#[test]
fn classes_follow_the_stored_fields_not_the_width() {
    let base = json!({
        "type": "text", "fontSize": 20, "lineHeight": 1.25, "autoResize": true,
        "height": 50, "width": 1000,
    });
    // Two lines at 20 px and 1.25: measureText's height is 50.
    assert_eq!(classify(&base, 50.0), Class::Measured);
    assert_eq!(classify(&base, 50.4), Class::Measured);
    assert_eq!(classify(&base, 25.0), Class::HeightMismatch);
    let mut no_resize = base.clone();
    no_resize["autoResize"] = json!(false);
    assert_eq!(classify(&no_resize, 50.0), Class::FixedWidth);
    let mut missing = base.clone();
    missing.as_object_mut().unwrap().remove("autoResize");
    assert_eq!(classify(&missing, 50.0), Class::Measured);
    let mut legacy = base.clone();
    legacy.as_object_mut().unwrap().remove("lineHeight");
    assert_eq!(classify(&legacy, 50.0), Class::Legacy);
    let mut legacy_fixed = legacy.clone();
    legacy_fixed["autoResize"] = json!(false);
    assert_eq!(classify(&legacy_fixed, 50.0), Class::FixedWidth);
    let mut no_height = base;
    no_height.as_object_mut().unwrap().remove("height");
    assert_eq!(classify(&no_height, 50.0), Class::HeightMismatch);
}
