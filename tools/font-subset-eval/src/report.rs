//! The evaluation: every candidate on every font face upstream's SVG export
//! inlines in `upstream-subsets.json`, measured against upstream's own
//! subset of the same face.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::candidates::{decode, encode, subset, Encoder, Subsetter};
use crate::fidelity::{compare, Fidelity};

/// What goes in a face's `src: url(…)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Candidate {
    /// The vendored file as it is, what `excali_svg::FontFiles` inlines
    /// today (`crates/excali-svg/src/fonts.rs`).
    Whole,
    /// The vendored file subset and encoded as WOFF2, as upstream does.
    Subset(Subsetter, Encoder),
}

impl Candidate {
    /// Whole first, then every subsetter with every encoder.
    pub fn all() -> Vec<Candidate> {
        let mut out = vec![Candidate::Whole];
        for s in Subsetter::ALL {
            for e in Encoder::ALL {
                out.push(Candidate::Subset(s, e));
            }
        }
        out
    }

    pub fn name(self) -> String {
        match self {
            Candidate::Whole => "whole".into(),
            Candidate::Subset(s, e) => format!("{}+{}", s.id(), e.id()),
        }
    }

    /// The Cargo features the candidate needs (wasm.sh builds them alone).
    pub fn features(self) -> Vec<&'static str> {
        match self {
            Candidate::Whole => vec![],
            Candidate::Subset(s, e) => vec![s.id(), e.id()],
        }
    }
}

/// One `@font-face` rule of upstream-subsets.json.
#[derive(Clone, Debug)]
pub struct Declaration {
    pub family: String,
    /// The face's file under upstream's packages/excalidraw/fonts.
    pub file: String,
    pub code_points: Vec<u32>,
    /// Upstream's subset, as inlined.
    pub woff2: Vec<u8>,
}

/// One scene of upstream-subsets.json.
#[derive(Clone, Debug)]
pub struct Scene {
    pub name: String,
    /// `browser` or `vitest` (upstream's test FontFace).
    pub font_face: String,
    /// `(fontFamily, text)` of each text element.
    pub texts: Vec<(u64, String)>,
    pub declarations: Vec<Declaration>,
}

pub const UPSTREAM_SUBSETS: &str = "tools/font-subset-eval/upstream-subsets.json";
pub const MANIFEST: &str = "crates/excali-text/assets/fonts/manifest.json";
pub const FONTS: &str = "crates/excali-text/assets/fonts";
pub const REPORT: &str = "tools/font-subset-eval/report.json";

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Standard base64, as the fixture writes it (Node's Buffer).
pub fn base64_decode(text: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .unwrap_or_else(|e| panic!("base64: {e}"))
}

/// The upstream commit and scenes of upstream-subsets.json.
pub fn load_scenes(root: &Path) -> (String, Vec<Scene>) {
    let v = read_json(&root.join(UPSTREAM_SUBSETS));
    let scenes = v["scenes"]
        .as_array()
        .expect("scenes")
        .iter()
        .map(|s| Scene {
            name: s["name"].as_str().expect("name").to_string(),
            font_face: s["fontFace"].as_str().expect("fontFace").to_string(),
            texts: s["texts"]
                .as_array()
                .expect("texts")
                .iter()
                .map(|t| {
                    (
                        t["fontFamily"].as_u64().expect("fontFamily"),
                        t["text"].as_str().expect("text").to_string(),
                    )
                })
                .collect(),
            declarations: s["declarations"]
                .as_array()
                .expect("declarations")
                .iter()
                .map(|d| Declaration {
                    family: d["family"].as_str().expect("family").to_string(),
                    file: d["file"].as_str().expect("file").to_string(),
                    code_points: d["codePoints"]
                        .as_array()
                        .expect("codePoints")
                        .iter()
                        .map(|c| c.as_u64().expect("code point") as u32)
                        .collect(),
                    woff2: base64_decode(d["woff2"].as_str().expect("woff2")),
                })
                .collect(),
        })
        .collect();
    (
        v["upstream"].as_str().expect("upstream").to_string(),
        scenes,
    )
}

/// From excali-text's font manifest: upstream file → vendored file, and
/// FontFace family name → family id.
pub struct Manifest {
    pub files: HashMap<String, String>,
    pub families: HashMap<String, u64>,
}

pub fn load_manifest(root: &Path) -> Manifest {
    let v = read_json(&root.join(MANIFEST));
    let mut files = HashMap::new();
    let mut families = HashMap::new();
    for f in v["families"].as_array().expect("families") {
        let id = f["id"].as_u64().expect("id");
        families.insert(f["family"].as_str().expect("family").to_string(), id);
        for face in f["faces"].as_array().expect("faces") {
            if let (Some(up), Some(file)) = (face["upstreamFile"].as_str(), face["file"].as_str()) {
                files.insert(up.to_string(), file.to_string());
            }
        }
    }
    Manifest { files, families }
}

/// The family ids whose text a face of `family_id` draws: its own, and
/// Excalifont's for Xiaolai, which upstream inlines with Excalifont's
/// characters (`Fonts.ts:196-205`).
pub fn text_families(family_id: u64) -> Vec<u64> {
    match family_id {
        100 => vec![100, 5],
        id => vec![id],
    }
}

/// The upstream checkout (UPSTREAM_DIR, else scripts/upstream/checkout.sh
/// --print-dir), for the one upstream file the port does not vendor
/// (Liberation Sans 1.05, ADR-004).
pub fn upstream_dir(root: &Path) -> PathBuf {
    if let Some(dir) = std::env::var_os("UPSTREAM_DIR") {
        return PathBuf::from(dir);
    }
    let out = std::process::Command::new("bash")
        .arg(root.join("scripts/upstream/checkout.sh"))
        .arg("--print-dir")
        .output()
        .expect("scripts/upstream/checkout.sh --print-dir");
    PathBuf::from(String::from_utf8(out.stdout).expect("utf8").trim())
}

/// Sums over declarations.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub declarations: usize,
    /// Subsetting or encoding failed (upstream then inlines the whole file,
    /// `subsetToBase64`, which the size counts).
    pub failed: usize,
    /// Output wuff decodes and ttf-parser parses.
    pub decoded: usize,
    pub code_points: usize,
    pub kept: usize,
    pub glyphs_equal: usize,
    pub metrics_equal: usize,
    pub runs: usize,
    pub runs_equal: usize,
    /// Declarations with everything equal.
    pub full: usize,
    /// Output byte-identical to upstream's woff2.
    pub woff2_identical: usize,
    /// Output decoded to the same sfnt bytes as upstream's woff2.
    pub sfnt_identical: usize,
    /// Font bytes inlined.
    pub bytes: usize,
    /// Characters of the `url(…)` contents: the data URL.
    pub data_url: usize,
}

impl Totals {
    fn add_fidelity(&mut self, f: &Fidelity) {
        self.decoded += 1;
        self.code_points += f.code_points;
        self.kept += f.kept;
        self.glyphs_equal += f.glyphs_equal;
        self.metrics_equal += usize::from(f.metrics_equal);
        self.runs += f.runs;
        self.runs_equal += f.runs_equal;
        self.full += usize::from(f.full());
    }

    /// Full fidelity on every declaration, and nothing failed.
    pub fn faithful(&self) -> bool {
        self.failed == 0 && self.full == self.declarations
    }

    fn to_json(&self) -> Value {
        json!({
            "declarations": self.declarations,
            "failed": self.failed,
            "decoded": self.decoded,
            "codePoints": self.code_points,
            "kept": self.kept,
            "glyphsEqual": self.glyphs_equal,
            "metricsEqual": self.metrics_equal,
            "runs": self.runs,
            "runsEqual": self.runs_equal,
            "full": self.full,
            "woff2Identical": self.woff2_identical,
            "sfntIdentical": self.sfnt_identical,
            "bytes": self.bytes,
            "dataUrl": self.data_url,
        })
    }
}

/// `data:<mime>;base64,<payload>`: its length for `bytes` bytes.
pub fn data_url_len(mime: &str, bytes: usize) -> usize {
    "data:".len() + mime.len() + ";base64,".len() + bytes.div_ceil(3) * 4
}

/// `part / whole` as a percentage with one decimal, rounded half up, from
/// integers so the report is the same on every platform.
pub fn percent(part: usize, whole: usize) -> String {
    if whole == 0 {
        return "n/a".into();
    }
    let tenths = (part as u128 * 1000 * 2 + whole as u128) / (whole as u128 * 2);
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// The whole evaluation.
pub struct Report {
    pub upstream: String,
    pub scenes: Vec<Scene>,
    pub upstream_totals: Totals,
    pub candidates: Vec<(Candidate, Totals)>,
    /// Per scene: upstream's bytes and each candidate's, in `candidates`
    /// order.
    pub scene_bytes: Vec<(usize, Vec<usize>)>,
}

struct Face {
    /// The vendored file.
    file: Vec<u8>,
    mime: &'static str,
    sfnt: Vec<u8>,
    /// Upstream's original face, file and sfnt: the vendored file's, but
    /// for Liberation Sans.
    upstream_file: Vec<u8>,
    upstream_sfnt: Vec<u8>,
}

/// The vendored face standing for upstream's `file`, and upstream's own.
fn load_face(root: &Path, manifest: &Manifest, file: &str) -> Face {
    let vendored = manifest
        .files
        .get(file)
        .unwrap_or_else(|| panic!("{file} is not in the manifest"));
    let bytes = std::fs::read(root.join(FONTS).join(vendored)).expect("vendored font");
    let sfnt = decode(&bytes).expect("vendored font decodes");
    let (upstream_file, upstream_sfnt) = if vendored == file {
        (bytes.clone(), sfnt.clone())
    } else {
        let path = upstream_dir(root)
            .join("packages/excalidraw/fonts")
            .join(file);
        let up = std::fs::read(&path).unwrap_or_else(|e| {
            panic!("{}: {e} (run scripts/upstream/checkout.sh)", path.display())
        });
        let up_sfnt = decode(&up).expect("upstream font decodes");
        (up, up_sfnt)
    };
    let mime = if bytes.starts_with(b"wOF2") {
        "font/woff2"
    } else {
        "font/ttf"
    };
    Face {
        file: bytes,
        mime,
        sfnt,
        upstream_file,
        upstream_sfnt,
    }
}

/// The texts of `scene` drawn in the face of `d`.
fn texts_for<'a>(scene: &'a Scene, manifest: &Manifest, d: &Declaration) -> Vec<&'a str> {
    let family_id = *manifest
        .families
        .get(&d.family)
        .unwrap_or_else(|| panic!("family {} is not in the manifest", d.family));
    let ids = text_families(family_id);
    scene
        .texts
        .iter()
        .filter(|(f, _)| ids.contains(f))
        .map(|(_, t)| t.as_str())
        .collect()
}

/// Runs every candidate over every declaration.
pub fn evaluate(root: &Path) -> Report {
    let (upstream, scenes) = load_scenes(root);
    let manifest = load_manifest(root);
    let mut faces: HashMap<String, Face> = HashMap::new();
    let candidates = Candidate::all();
    let mut upstream_totals = Totals::default();
    let mut totals: Vec<Totals> = vec![Totals::default(); candidates.len()];
    let mut scene_bytes = Vec::new();
    for scene in &scenes {
        let mut up_bytes = 0;
        let mut bytes = vec![0; candidates.len()];
        for d in &scene.declarations {
            let face = faces
                .entry(d.file.clone())
                .or_insert_with(|| load_face(root, &manifest, &d.file));
            let texts = texts_for(scene, &manifest, d);

            // Upstream's own subset, against upstream's original face.
            let up_sfnt = decode(&d.woff2).expect("upstream subset decodes");
            upstream_totals.declarations += 1;
            upstream_totals.bytes += d.woff2.len();
            upstream_totals.data_url += data_url_len("font/woff2", d.woff2.len());
            upstream_totals.woff2_identical += 1;
            upstream_totals.sfnt_identical += 1;
            up_bytes += d.woff2.len();
            if let Some(f) = compare(&face.upstream_sfnt, &up_sfnt, &d.code_points, &texts) {
                upstream_totals.add_fidelity(&f);
            }

            for (i, c) in candidates.iter().enumerate() {
                let t = &mut totals[i];
                t.declarations += 1;
                let (out, mime) = match *c {
                    Candidate::Whole => (Ok(face.file.clone()), face.mime),
                    Candidate::Subset(s, e) => (
                        subset(s, &face.sfnt, &d.code_points).and_then(|sub| encode(e, &sub)),
                        "font/woff2",
                    ),
                };
                let out = match out {
                    Ok(out) => out,
                    Err(_) => {
                        // subsetToBase64's fallback: the whole file.
                        t.failed += 1;
                        t.bytes += face.file.len();
                        t.data_url += data_url_len(face.mime, face.file.len());
                        bytes[i] += face.file.len();
                        continue;
                    }
                };
                t.bytes += out.len();
                t.data_url += data_url_len(mime, out.len());
                bytes[i] += out.len();
                t.woff2_identical += usize::from(out == d.woff2);
                let Ok(sfnt) = decode(&out) else { continue };
                t.sfnt_identical += usize::from(sfnt == up_sfnt);
                if let Some(f) = compare(&face.sfnt, &sfnt, &d.code_points, &texts) {
                    t.add_fidelity(&f);
                }
            }
        }
        scene_bytes.push((up_bytes, bytes));
    }
    Report {
        upstream,
        scenes,
        upstream_totals,
        candidates: candidates.into_iter().zip(totals).collect(),
        scene_bytes,
    }
}

impl Report {
    pub fn totals(&self, name: &str) -> &Totals {
        if name == "upstream" {
            return &self.upstream_totals;
        }
        &self
            .candidates
            .iter()
            .find(|(c, _)| c.name() == name)
            .unwrap_or_else(|| panic!("no candidate {name}"))
            .1
    }

    pub fn to_json(&self) -> Value {
        let up = self.upstream_totals.bytes;
        let mut candidates = Vec::new();
        for (c, t) in &self.candidates {
            let (subsetter, encoder) = match c {
                Candidate::Whole => (Value::Null, Value::Null),
                Candidate::Subset(s, e) => (json!(s.name()), json!(e.name())),
            };
            let mut o = Map::new();
            o.insert("name".into(), json!(c.name()));
            o.insert("subsetter".into(), subsetter);
            o.insert("encoder".into(), encoder);
            o.insert("percentOfUpstream".into(), json!(percent(t.bytes, up)));
            o.insert("faithful".into(), json!(t.faithful()));
            if let Value::Object(m) = t.to_json() {
                o.extend(m);
            }
            candidates.push(Value::Object(o));
        }
        let scenes: Vec<Value> = self
            .scenes
            .iter()
            .zip(&self.scene_bytes)
            .map(|(s, (up, bytes))| {
                let mut o = Map::new();
                o.insert("name".into(), json!(s.name));
                o.insert("fontFace".into(), json!(s.font_face));
                o.insert("declarations".into(), json!(s.declarations.len()));
                o.insert("upstream".into(), json!(up));
                for ((c, _), b) in self.candidates.iter().zip(bytes) {
                    o.insert(c.name(), json!(b));
                }
                Value::Object(o)
            })
            .collect();
        json!({
            "description": "ex-408: every @font-face rule of upstream's SVG export in upstream-subsets.json (upstream's own harfbuzzjs subsets, tools/goldens/font-subset.mjs) made again from the port's vendored face (crates/excali-text/assets/fonts) by each candidate: the whole file, or a subsetter (upstream's input: the rule's code points, every layout feature, HarfBuzz's defaults otherwise) with a WOFF2 encoder at brotli quality 11. bytes: font bytes inlined (a failed subset counts the whole file, subsetToBase64's fallback); dataUrl: characters of the url() contents. Fidelity against the original face with ttf-parser 0.25.1 and rustybuzz 0.20.1: codePoints the original maps, kept by the output, glyphsEqual (advance and outline), metricsEqual (declarations with unitsPerEm, hhea, OS/2 typo and win metrics and USE_TYPO_METRICS equal), runs of the scene's text in the face and runsEqual (same clusters, glyph outlines, advances and offsets when shaped), full (declarations with all equal), woff2Identical and sfntIdentical (to upstream's subset, as encoded and as wuff 0.2.9 decodes it). upstream is upstream's subset measured the same way against upstream's original face. Written by tools/font-subset-eval (cargo run --release -- --write).",
            "upstream": self.upstream,
            "scenes": self.scenes.len(),
            "upstreamSubset": self.upstream_totals.to_json(),
            "candidates": candidates,
            "perScene": scenes,
        })
    }
}

/// report.json's text for `report`.
pub fn report_text(report: &Report) -> String {
    let mut text = serde_json::to_string_pretty(&report.to_json()).expect("json");
    text.push('\n');
    text
}

/// The candidates the browser check renders: upstream's subset, and each
/// subsetter with the pure Rust encoder (the encoder does not change the
/// decoded font; `sfntIdentical` shows both encoders decode alike).
pub fn browser_candidates() -> Vec<Candidate> {
    Subsetter::ALL
        .into_iter()
        .map(|s| Candidate::Subset(s, Encoder::Ttf2woff2))
        .collect()
}

/// The input of browser/render.mjs: per browser scene and face, the
/// original face, upstream's subset and each browser candidate's output
/// (fonts stored once, by index), and the runs of the scene's text in it.
pub fn browser_cases(root: &Path) -> Value {
    use base64::Engine;
    let (_, scenes) = load_scenes(root);
    let manifest = load_manifest(root);
    let mut faces: HashMap<String, Face> = HashMap::new();
    let mut fonts: Vec<Vec<u8>> = Vec::new();
    let mut index = |bytes: Vec<u8>| -> usize {
        if let Some(i) = fonts.iter().position(|f| *f == bytes) {
            return i;
        }
        fonts.push(bytes);
        fonts.len() - 1
    };
    let mut cases = Vec::new();
    for scene in scenes.iter().filter(|s| s.font_face == "browser") {
        for d in &scene.declarations {
            let face = faces
                .entry(d.file.clone())
                .or_insert_with(|| load_face(root, &manifest, &d.file));
            let texts = texts_for(scene, &manifest, d);
            let parsed = ttf_parser::Face::parse(&face.sfnt, 0).expect("original parses");
            let runs: Vec<String> = texts
                .iter()
                .flat_map(|t| crate::fidelity::runs(&parsed, &d.code_points, t))
                .collect();
            let up_parsed =
                ttf_parser::Face::parse(&face.upstream_sfnt, 0).expect("upstream original parses");
            let up_runs: Vec<String> = texts
                .iter()
                .flat_map(|t| crate::fidelity::runs(&up_parsed, &d.code_points, t))
                .collect();
            let mut subsets = Map::new();
            subsets.insert(
                "upstream".into(),
                json!({ "original": index(face.upstream_file.clone()), "font": index(d.woff2.clone()), "runs": up_runs }),
            );
            let original = index(face.file.clone());
            for c in browser_candidates() {
                let Candidate::Subset(s, e) = c else { continue };
                let font = subset(s, &face.sfnt, &d.code_points)
                    .and_then(|sub| encode(e, &sub))
                    .map(&mut index)
                    .ok();
                subsets.insert(
                    c.name(),
                    json!({ "original": original, "font": font, "runs": runs }),
                );
            }
            cases.push(json!({
                "scene": scene.name,
                "family": d.family,
                "file": d.file,
                "subsets": subsets,
            }));
        }
    }
    let engine = base64::engine::general_purpose::STANDARD;
    json!({
        "candidates": std::iter::once("upstream".to_string())
            .chain(browser_candidates().into_iter().map(Candidate::name))
            .collect::<Vec<_>>(),
        "fonts": fonts.iter().map(|f| engine.encode(f)).collect::<Vec<_>>(),
        "cases": cases,
    })
}
