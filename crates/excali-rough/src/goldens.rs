//! The golden harness (ex-217): compares the port's drawables with the
//! goldens `tools/goldens/generate.mjs` writes from upstream's own code, and
//! prints every difference as a number with where it is.
//!
//! Compiled only with the `goldens` feature. The parity tests turn it on
//! (this crate lists itself as a dev-dependency with the feature), so
//! `cargo test --workspace` always runs them; the library proper, and its
//! wasm32 build, never see JSON.
//!
//! Formats are those of `tools/goldens/README.md` ("Format"):
//!
//! - rough cases `{ id, method, args, options, drawable }` go through
//!   [`Report::drawable`];
//! - element cases `{ id, element, renderConfig, shapes }` go through
//!   [`Report::element`], with `shapes` a list of `{ type: "rough",
//!   drawable }` and `{ type: "svgPath", d }`;
//! - a drawable is `{ shape, options, sets: [{ type, ops: [{ op, data }] }] }`.
//!
//! A difference is reported with the case id, the element id (element
//! cases), the shape, set and op index, the data index, and the expected
//! (upstream) and actual (port) values with their difference in units and
//! ulps. A failing file panics with one block per failing case:
//!
//! ```text
//! elements-rectangle.json: 1 of 54 cases differ from upstream
//!
//! case rectangle/seed7-r1, element rectangle_seed7-r1, shape 0, set 0 (path), op 1 (bcurveTo) data[4]
//!     expected 200.5
//!     actual   200
//!     diff     -0.5 (2251799813685248 ulp; tolerance exact)
//!     expected op bcurveTo [66.9, 0.6, 133.4, -0.1, 200.5, 0.7]
//!     actual op   bcurveTo [66.9, 0.6, 133.4, -0.1, 200, 0.7]
//! ```
//!
//! [`Manifest`] reads `goldens/manifest.json`, which pins every golden file
//! by sha256 and case count and names the upstream commit.

use std::collections::BTreeMap;
use std::fmt;

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::{Drawable, Op, Options};

/// How many differences of one case are printed; the rest are counted.
pub const SHOWN_PER_CASE: usize = 8;

/// How close a number must be to upstream's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tolerance {
    /// `==` on doubles (so `0 == -0`, and NaN never matches).
    Exact,
    /// Equal, or within this fraction of the larger of 1 and the expected
    /// magnitude.
    Relative(f64),
}

impl Tolerance {
    /// Whether `actual` is close enough to `expected`.
    pub fn accepts(self, expected: f64, actual: f64) -> bool {
        match self {
            Tolerance::Exact => expected == actual,
            Tolerance::Relative(t) => {
                expected == actual || (actual - expected).abs() <= t * expected.abs().max(1.0)
            }
        }
    }
}

impl fmt::Display for Tolerance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tolerance::Exact => f.write_str("exact"),
            Tolerance::Relative(t) => write!(f, "relative {t:e}"),
        }
    }
}

/// The number of representable doubles between `a` and `b` (0 for `0` and
/// `-0`).
pub fn ulps(a: f64, b: f64) -> u64 {
    fn key(x: f64) -> i128 {
        // Bits as an integer that increases with the value: negative
        // doubles count down from -0.
        let bits = x.to_bits() as i64;
        if bits < 0 {
            i128::from(i64::MIN) - i128::from(bits)
        } else {
            i128::from(bits)
        }
    }
    u64::try_from((key(a) - key(b)).unsigned_abs()).unwrap_or(u64::MAX)
}

// ---------------------------------------------------------------------------
// Drawables as the goldens record them

/// The resolved options as rough.js holds them (`ResolvedOptions` without
/// the randomizer): the defaults always, the optional keys when set.
pub fn options_json(o: &Options) -> Value {
    let mut m = Map::new();
    let mut put = |k: &str, v: Value| {
        m.insert(k.to_owned(), v);
    };
    put("maxRandomnessOffset", json!(o.max_randomness_offset));
    put("roughness", json!(o.roughness));
    put("bowing", json!(o.bowing));
    put("stroke", json!(o.stroke));
    put("strokeWidth", json!(o.stroke_width));
    put("curveTightness", json!(o.curve_tightness));
    put("curveFitting", json!(o.curve_fitting));
    put("curveStepCount", json!(o.curve_step_count));
    put("fillStyle", json!(o.fill_style));
    put("fillWeight", json!(o.fill_weight));
    put("hachureAngle", json!(o.hachure_angle));
    put("hachureGap", json!(o.hachure_gap));
    put("dashOffset", json!(o.dash_offset));
    put("dashGap", json!(o.dash_gap));
    put("zigzagOffset", json!(o.zigzag_offset));
    put("seed", json!(o.seed));
    put("disableMultiStroke", json!(o.disable_multi_stroke));
    put("disableMultiStrokeFill", json!(o.disable_multi_stroke_fill));
    put("preserveVertices", json!(o.preserve_vertices));
    put("fillShapeRoughnessGain", json!(o.fill_shape_roughness_gain));
    if let Some(v) = &o.fill {
        put("fill", json!(v));
    }
    if let Some(v) = o.simplification {
        put("simplification", json!(v));
    }
    if let Some(v) = &o.stroke_line_dash {
        put("strokeLineDash", json!(v));
    }
    if let Some(v) = o.stroke_line_dash_offset {
        put("strokeLineDashOffset", json!(v));
    }
    if let Some(v) = &o.fill_line_dash {
        put("fillLineDash", json!(v));
    }
    if let Some(v) = o.fill_line_dash_offset {
        put("fillLineDashOffset", json!(v));
    }
    if let Some(v) = o.fixed_decimal_place_digits {
        put("fixedDecimalPlaceDigits", json!(v));
    }
    Value::Object(m)
}

/// `{ op, data }`.
pub fn op_json(op: &Op) -> Value {
    json!({ "op": op.name(), "data": op.data() })
}

/// `{ shape, options, sets: [{ type, ops }] }`, the golden form of a
/// drawable.
pub fn drawable_json(d: &Drawable) -> Value {
    json!({
        "shape": d.shape.as_str(),
        "options": options_json(&d.options),
        "sets": d.sets.iter().map(|s| json!({
            "type": s.kind.as_str(),
            "ops": s.ops.iter().map(op_json).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------
// Differences

/// Where a difference is. Unset parts do not apply (a rough primitive case
/// has no element; an option difference has no op).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Location {
    /// The case `id`.
    pub case: String,
    /// The element's `id`, for element cases.
    pub element: Option<String>,
    /// Index into the case's `shapes`.
    pub shape: Option<usize>,
    /// Index into the drawable's `sets`, and the expected set `type`.
    pub set: Option<(usize, String)>,
    /// Index into the set's `ops`, and the expected `op`.
    pub op: Option<(usize, String)>,
    /// Index into the op's `data`.
    pub data: Option<usize>,
    /// What else names the value: `shape`, `options.roughness`, `type`,
    /// `d number 4`, ...
    pub path: String,
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "case {}", self.case)?;
        if let Some(e) = &self.element {
            write!(f, ", element {e}")?;
        }
        if let Some(s) = self.shape {
            write!(f, ", shape {s}")?;
        }
        if let Some((i, t)) = &self.set {
            write!(f, ", set {i} ({t})")?;
        }
        if let Some((i, o)) = &self.op {
            write!(f, ", op {i} ({o})")?;
        }
        if let Some(i) = self.data {
            write!(f, " data[{i}]")?;
        }
        if !self.path.is_empty() {
            write!(f, ", {}", self.path)?;
        }
        Ok(())
    }
}

/// What differs.
#[derive(Clone, Debug, PartialEq)]
pub enum Difference {
    /// A number outside the tolerance.
    Number {
        expected: f64,
        actual: f64,
        tolerance: Tolerance,
    },
    /// A string, boolean, key presence or type; `None` is an absent key.
    Value {
        expected: Option<Value>,
        actual: Option<Value>,
    },
    /// A list of a different length (`shapes`, `sets`, `ops`, `data`, ...).
    Count {
        what: &'static str,
        expected: usize,
        actual: usize,
    },
}

/// One difference between upstream's golden and the port.
#[derive(Clone, Debug, PartialEq)]
pub struct Mismatch {
    pub location: Location,
    pub difference: Difference,
    /// Extra lines printed under the difference (the whole op, expected
    /// and actual, for a number inside an op).
    pub context: Vec<String>,
}

fn number_text(x: f64) -> String {
    format!("{x}")
}

fn diff_text(d: f64) -> String {
    if d == 0.0 || d.abs() >= 1e-4 {
        format!("{d}")
    } else {
        format!("{d:e}")
    }
}

fn value_text(v: &Option<Value>) -> String {
    match v {
        None => "(absent)".to_owned(),
        Some(Value::Number(n)) => n.as_f64().map_or_else(|| n.to_string(), number_text),
        Some(v) => v.to_string(),
    }
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.difference {
            Difference::Number {
                expected,
                actual,
                tolerance,
            } => {
                writeln!(f, "{}", self.location)?;
                writeln!(f, "    expected {}", number_text(*expected))?;
                writeln!(f, "    actual   {}", number_text(*actual))?;
                write!(
                    f,
                    "    diff     {} ({} ulp; tolerance {tolerance})",
                    diff_text(actual - expected),
                    ulps(*expected, *actual)
                )?;
            }
            Difference::Value { expected, actual } => {
                writeln!(f, "{}", self.location)?;
                writeln!(f, "    expected {}", value_text(expected))?;
                write!(f, "    actual   {}", value_text(actual))?;
            }
            Difference::Count {
                what,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "{}\n    {what}: expected {expected}, actual {actual}",
                    self.location
                )?;
            }
        }
        for line in &self.context {
            write!(f, "\n    {line}")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Comparison

struct Walk<'a> {
    at: Location,
    tolerance: Tolerance,
    found: &'a mut Vec<Mismatch>,
}

impl Walk<'_> {
    fn push(&mut self, at: Location, difference: Difference, context: Vec<String>) {
        self.found.push(Mismatch {
            location: at,
            difference,
            context,
        });
    }

    fn with_path(&self, path: impl Into<String>) -> Location {
        let mut at = self.at.clone();
        at.path = path.into();
        at
    }

    fn count(&mut self, what: &'static str, expected: usize, actual: usize) {
        if expected != actual {
            let at = self.at.clone();
            self.push(
                at,
                Difference::Count {
                    what,
                    expected,
                    actual,
                },
                Vec::new(),
            );
        }
    }

    fn value(&mut self, path: &str, expected: Option<&Value>, actual: Option<&Value>) {
        match (expected, actual) {
            (Some(Value::Number(e)), Some(Value::Number(a))) => {
                let (e, a) = (
                    e.as_f64().unwrap_or(f64::NAN),
                    a.as_f64().unwrap_or(f64::NAN),
                );
                if !Tolerance::Exact.accepts(e, a) {
                    let at = self.with_path(path);
                    self.push(
                        at,
                        Difference::Number {
                            expected: e,
                            actual: a,
                            tolerance: Tolerance::Exact,
                        },
                        Vec::new(),
                    );
                }
            }
            (Some(Value::Array(e)), Some(Value::Array(a))) => {
                if e.len() != a.len() {
                    let at = self.with_path(path);
                    self.push(
                        at,
                        Difference::Count {
                            what: "items",
                            expected: e.len(),
                            actual: a.len(),
                        },
                        Vec::new(),
                    );
                } else {
                    for (i, (x, y)) in e.iter().zip(a).enumerate() {
                        self.value(&format!("{path}[{i}]"), Some(x), Some(y));
                    }
                }
            }
            (e, a) if e == a => {}
            (e, a) => {
                let at = self.with_path(path);
                self.push(
                    at,
                    Difference::Value {
                        expected: e.cloned(),
                        actual: a.cloned(),
                    },
                    Vec::new(),
                );
            }
        }
    }

    fn options(&mut self, expected: &Value, actual: &Value) {
        let empty = Map::new();
        let e = expected.as_object().unwrap_or(&empty);
        let a = actual.as_object().unwrap_or(&empty);
        for (k, v) in e {
            self.value(&format!("options.{k}"), Some(v), a.get(k));
        }
        for (k, v) in a {
            if !e.contains_key(k) {
                self.value(&format!("options.{k}"), None, Some(v));
            }
        }
    }

    fn op(&mut self, expected: &Value, actual: &Value) {
        let e_name = expected["op"].as_str().unwrap_or_default().to_owned();
        let a_name = actual["op"].as_str().unwrap_or_default();
        self.at.op = self.at.op.take().map(|(i, _)| (i, e_name.clone()));
        if e_name != a_name {
            self.value("", Some(&expected["op"]), Some(&actual["op"]));
            return;
        }
        let empty = Vec::new();
        let e = expected["data"].as_array().unwrap_or(&empty);
        let a = actual["data"].as_array().unwrap_or(&empty);
        self.count("data", e.len(), a.len());
        let op_line = |data: &[Value]| {
            let numbers: Vec<String> = data
                .iter()
                .map(|v| v.as_f64().map_or_else(|| v.to_string(), number_text))
                .collect();
            format!("{e_name} [{}]", numbers.join(", "))
        };
        for (i, (x, y)) in e.iter().zip(a).enumerate() {
            let (x, y) = (
                x.as_f64().unwrap_or(f64::NAN),
                y.as_f64().unwrap_or(f64::NAN),
            );
            if !self.tolerance.accepts(x, y) {
                let mut at = self.at.clone();
                at.data = Some(i);
                let context = vec![
                    format!("expected op {}", op_line(e)),
                    format!("actual op   {}", op_line(a)),
                ];
                let tolerance = self.tolerance;
                self.push(
                    at,
                    Difference::Number {
                        expected: x,
                        actual: y,
                        tolerance,
                    },
                    context,
                );
            }
        }
    }

    fn set(&mut self, expected: &Value, actual: &Value) {
        let e_type = expected["type"].as_str().unwrap_or_default().to_owned();
        self.at.set = self.at.set.take().map(|(i, _)| (i, e_type));
        self.value("type", expected.get("type"), actual.get("type"));
        let empty = Vec::new();
        let e = expected["ops"].as_array().unwrap_or(&empty);
        let a = actual["ops"].as_array().unwrap_or(&empty);
        self.count("ops", e.len(), a.len());
        for (i, (x, y)) in e.iter().zip(a).enumerate() {
            self.at.op = Some((i, String::new()));
            self.op(x, y);
        }
        self.at.op = None;
    }

    fn drawable(&mut self, expected: &Value, actual: &Value) {
        self.value("shape", expected.get("shape"), actual.get("shape"));
        self.options(&expected["options"], &actual["options"]);
        let empty = Vec::new();
        let e = expected["sets"].as_array().unwrap_or(&empty);
        let a = actual["sets"].as_array().unwrap_or(&empty);
        self.count("sets", e.len(), a.len());
        for (i, (x, y)) in e.iter().zip(a).enumerate() {
            self.at.set = Some((i, String::new()));
            self.set(x, y);
        }
        self.at.set = None;
    }

    fn svg_path(&mut self, expected: &str, actual: &str) {
        let e = svg_tokens(expected);
        let a = svg_tokens(actual);
        if e.len() != a.len() {
            let at = self.with_path("d");
            self.push(
                at,
                Difference::Count {
                    what: "d tokens",
                    expected: e.len(),
                    actual: a.len(),
                },
                Vec::new(),
            );
        }
        let mut number = 0;
        for (i, (x, y)) in e.iter().zip(&a).enumerate() {
            match (x, y) {
                (SvgToken::Number(x), SvgToken::Number(y)) => {
                    if !self.tolerance.accepts(*x, *y) {
                        let at = self.with_path(format!("d number {number}"));
                        let tolerance = self.tolerance;
                        self.push(
                            at,
                            Difference::Number {
                                expected: *x,
                                actual: *y,
                                tolerance,
                            },
                            Vec::new(),
                        );
                    }
                    number += 1;
                }
                (x, y) if x == y => {}
                (x, y) => {
                    let at = self.with_path(format!("d token {i}"));
                    self.push(
                        at,
                        Difference::Value {
                            expected: Some(x.json()),
                            actual: Some(y.json()),
                        },
                        Vec::new(),
                    );
                    // numbers after a structural difference do not line up
                    return;
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum SvgToken {
    Command(char),
    Number(f64),
}

impl SvgToken {
    fn json(&self) -> Value {
        match self {
            SvgToken::Command(c) => Value::String(c.to_string()),
            SvgToken::Number(n) => json!(n),
        }
    }
}

/// Splits SVG path data into command letters and numbers (separators are
/// whitespace and commas).
fn svg_tokens(d: &str) -> Vec<SvgToken> {
    let mut out = Vec::new();
    let b = d.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() || c == b',' {
            i += 1;
        } else if c.is_ascii_alphabetic() && c != b'e' && c != b'E' {
            out.push(SvgToken::Command(char::from(c)));
            i += 1;
        } else {
            let start = i;
            i += 1;
            while i < b.len() {
                let c = b[i];
                let exponent_sign = (c == b'-' || c == b'+') && matches!(b[i - 1], b'e' | b'E');
                if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || exponent_sign {
                    i += 1;
                } else {
                    break;
                }
            }
            let text = &d[start..i];
            match text.parse::<f64>() {
                Ok(n) => out.push(SvgToken::Number(n)),
                Err(_) => out.push(SvgToken::Command(char::from(b[start]))),
            }
        }
    }
    out
}

/// One shape the port produced for an element, in `shapes` order.
#[derive(Clone, Copy, Debug)]
pub enum ActualShape<'a> {
    /// `{ type: "rough", drawable }`.
    Rough(&'a Drawable),
    /// `{ type: "svgPath", d }` (freedraw).
    SvgPath(&'a str),
}

impl ActualShape<'_> {
    fn kind(&self) -> &'static str {
        match self {
            ActualShape::Rough(_) => "rough",
            ActualShape::SvgPath(_) => "svgPath",
        }
    }
}

fn case_location(case: &Value) -> Location {
    Location {
        case: case["id"]
            .as_str()
            .map_or_else(|| case["id"].to_string(), str::to_owned),
        element: case
            .get("element")
            .and_then(|e| e.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        ..Location::default()
    }
}

// ---------------------------------------------------------------------------
// Reports

/// The results of one golden file: every case run, and the differences of
/// the failing ones.
#[derive(Debug)]
pub struct Report {
    file: String,
    ran: usize,
    failures: Vec<Vec<Mismatch>>,
}

impl Report {
    /// An empty report for `file` (`elements-rectangle.json`, ...).
    pub fn new(file: &str) -> Self {
        Self {
            file: file.to_owned(),
            ran: 0,
            failures: Vec::new(),
        }
    }

    /// How many cases were compared.
    pub fn ran(&self) -> usize {
        self.ran
    }

    /// How many cases differed.
    pub fn failed(&self) -> usize {
        self.failures.len()
    }

    fn record(&mut self, found: Vec<Mismatch>) -> Vec<Mismatch> {
        self.ran += 1;
        if !found.is_empty() {
            self.failures.push(found.clone());
        }
        found
    }

    /// Compares one drawable of `case` (a rough case, whose golden is
    /// `case.drawable`, or any drawable the caller picked from a case) with
    /// the port's; returns the differences, also kept for [`Self::render`].
    pub fn drawable(
        &mut self,
        case: &Value,
        expected: &Value,
        actual: &Drawable,
        tolerance: Tolerance,
    ) -> Vec<Mismatch> {
        let mut found = Vec::new();
        Walk {
            at: case_location(case),
            tolerance,
            found: &mut found,
        }
        .drawable(expected, &drawable_json(actual));
        self.record(found)
    }

    /// Compares an element case's `shapes` with the port's shapes for that
    /// element; returns the differences, also kept for [`Self::render`].
    pub fn element(
        &mut self,
        case: &Value,
        actual: &[ActualShape<'_>],
        tolerance: Tolerance,
    ) -> Vec<Mismatch> {
        let mut found = Vec::new();
        let mut walk = Walk {
            at: case_location(case),
            tolerance,
            found: &mut found,
        };
        let empty = Vec::new();
        let expected = case["shapes"].as_array().unwrap_or(&empty);
        walk.count("shapes", expected.len(), actual.len());
        for (i, (e, a)) in expected.iter().zip(actual).enumerate() {
            walk.at.shape = Some(i);
            let kind = Value::String(a.kind().to_owned());
            if e.get("type") != Some(&kind) {
                walk.value("type", e.get("type"), Some(&kind));
                continue;
            }
            match a {
                ActualShape::Rough(d) => walk.drawable(&e["drawable"], &drawable_json(d)),
                ActualShape::SvgPath(d) => walk.svg_path(e["d"].as_str().unwrap_or_default(), d),
            }
        }
        self.record(found)
    }

    /// The failure text: a summary line, then each failing case's
    /// differences (at most [`SHOWN_PER_CASE`] each). Empty if every case
    /// matched.
    pub fn render(&self) -> String {
        if self.failures.is_empty() {
            return String::new();
        }
        let mut out = format!(
            "{}: {} of {} cases differ from upstream",
            self.file,
            self.failures.len(),
            self.ran
        );
        for found in &self.failures {
            out.push('\n');
            for m in found.iter().take(SHOWN_PER_CASE) {
                out.push('\n');
                out.push_str(&m.to_string());
            }
            if found.len() > SHOWN_PER_CASE {
                out.push_str(&format!(
                    "\n... and {} more differences in this case",
                    found.len() - SHOWN_PER_CASE
                ));
            }
        }
        out
    }

    /// Returns how many cases ran; panics with [`Self::render`] if any
    /// differed.
    #[track_caller]
    pub fn assert_ok(self) -> usize {
        if !self.failures.is_empty() {
            panic!("{}", self.render());
        }
        self.ran
    }
}

// ---------------------------------------------------------------------------
// Manifest

/// One entry of `goldens/manifest.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestFile {
    pub name: String,
    pub cases: usize,
    pub sha256: String,
}

/// `goldens/manifest.json`: the upstream commit and package versions the
/// goldens were generated from, and every file's case count and sha256.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub upstream_commit: String,
    pub packages: BTreeMap<String, String>,
    pub files: Vec<ManifestFile>,
}

impl Manifest {
    /// Reads the manifest text.
    pub fn parse(text: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("manifest: {e}"))?;
        let upstream_commit = v["upstream"]["commit"]
            .as_str()
            .ok_or("manifest: no upstream.commit")?
            .to_owned();
        let packages = v["packages"]
            .as_object()
            .ok_or("manifest: no packages")?
            .iter()
            .map(|(k, v)| {
                v.as_str()
                    .map(|s| (k.clone(), s.to_owned()))
                    .ok_or_else(|| format!("manifest: package {k} has no version"))
            })
            .collect::<Result<_, _>>()?;
        let files = v["files"]
            .as_array()
            .ok_or("manifest: no files")?
            .iter()
            .map(|f| {
                Ok(ManifestFile {
                    name: f["name"]
                        .as_str()
                        .ok_or("manifest: file without name")?
                        .to_owned(),
                    cases: f["cases"]
                        .as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .ok_or("manifest: file without cases")?,
                    sha256: f["sha256"]
                        .as_str()
                        .ok_or("manifest: file without sha256")?
                        .to_owned(),
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            upstream_commit,
            packages,
            files,
        })
    }

    /// Checks a golden file's bytes against its entry: listed, same sha256,
    /// same number of cases.
    pub fn verify(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let entry = self.files.iter().find(|f| f.name == name).ok_or_else(|| {
            format!("{name} is not in goldens/manifest.json (run node tools/goldens/generate.mjs)")
        })?;
        let sha: String = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if sha != entry.sha256 {
            return Err(format!(
                "{name}: sha256 {sha}, goldens/manifest.json says {}; goldens are \
                 generated, not edited: run node tools/goldens/generate.mjs",
                entry.sha256
            ));
        }
        let doc: Value = serde_json::from_slice(bytes).map_err(|e| format!("{name}: {e}"))?;
        let cases = doc["cases"]
            .as_array()
            .ok_or_else(|| format!("{name}: no cases"))?
            .len();
        if cases != entry.cases {
            return Err(format!(
                "{name}: {cases} cases, manifest says {}",
                entry.cases
            ));
        }
        Ok(())
    }
}
