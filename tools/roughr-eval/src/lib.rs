//! ex-206 spike: roughr 0.14.0 against the rough.js 4.6.4 goldens.
//!
//! Every rough case of `goldens/` (`{ id, method, args, options, drawable }`,
//! see `tools/goldens/README.md`) is replayed through roughr's `Generator`
//! with the options rough.js resolved for it (`drawable.options`: rough.js's
//! `defaultOptions` merged with the call's), which is what an adapter from
//! Excalidraw's rough options to roughr would pass. `random.json` compares
//! roughr's random stream with rough.js's `Random.next()`.
//!
//! A case matches at two decimals when every set has the golden's type, every
//! op the golden's kind and every number equals the golden's after
//! `+x.toFixed(2)`: that is what upstream's SVG export writes
//! (`fixedDecimalPlaceDigits` 2, `packages/excalidraw/renderer/staticSvgScene.ts:71`,
//! through rough.js `opsToPath`), so it is the loosest "equal" a user could
//! see. It matches exactly when the numbers are within the relative 1e-10
//! the port's own golden gate allows for platform trig
//! (`excali_rough::goldens::PLATFORM_TOLERANCE`).

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use roughr::core::{Drawable, FillStyle, OpSetType, OpType, Options};
use roughr::generator::Generator;
use roughr::{Point2D, Srgba};
use serde_json::{json, Map, Value};

/// The golden files the spike replays, in report order. The tests check this
/// against `goldens/manifest.json`.
pub const FILES: &[&str] = &[
    "random.json",
    "rough-primitives.json",
    "rough-generator.json",
    "rough-fills.json",
    "rough-options.json",
    "rough-strokes.json",
];

/// The relative tolerance of an "exact" match: the port's golden gate
/// (`excali_rough::goldens::PLATFORM_TOLERANCE`).
pub const EXACT_TOLERANCE: f64 = 1e-10;

/// The share of goldens (percent, at two decimals) from which a divergence
/// counts as small enough to fork rather than port.
pub const FORK_THRESHOLD: f64 = 90.0;

// ---------------------------------------------------------------------------
// Numbers

/// `+x.toFixed(digits)` (ECMA-262 `Number.prototype.toFixed`): the n for
/// which n / 10^digits - x is closest to zero, the larger n on an exact tie,
/// computed on the magnitude and signed afterwards.
pub fn to_fixed(x: f64, digits: u32) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let mag = x.abs();
    // Rust's `{:.N}` formats the exact binary value, correctly rounded,
    // with ties to even; an exact decimal tie at digit N+1 is the only case
    // where that differs from toFixed. Ties are exactly representable, so
    // the exact expansion shows them as a trailing 5.
    let exact = format!("{mag:.1100}");
    let (_, frac) = exact.split_once('.').expect("decimal point");
    let d = digits as usize;
    let tail = &frac[d..];
    let rounded = if tail.starts_with('5') && tail[1..].bytes().all(|b| b == b'0') {
        // A tie is a dyadic fraction with at most a few bits after the
        // point, so scaling by 10^digits is exact and n is its floor + 1;
        // n / 10^digits is the double nearest n·10^-digits, as parsing the
        // string toFixed returns gives.
        let scale = 10f64.powi(digits as i32);
        ((mag * scale).floor() + 1.0) / scale
    } else {
        format!("{mag:.d$}").parse::<f64>().expect("number")
    };
    if x.is_sign_negative() {
        -rounded
    } else {
        rounded
    }
}

/// How numbers are compared.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Precision {
    /// Within [`EXACT_TOLERANCE`] of the larger of 1 and the golden.
    Exact,
    /// Equal after `+x.toFixed(n)`.
    Decimals(u32),
}

impl Precision {
    fn accepts(self, expected: f64, actual: f64) -> bool {
        match self {
            Precision::Exact => {
                expected == actual
                    || (actual - expected).abs() <= EXACT_TOLERANCE * expected.abs().max(1.0)
            }
            Precision::Decimals(n) => to_fixed(expected, n) == to_fixed(actual, n),
        }
    }
}

// ---------------------------------------------------------------------------
// Options

fn num(v: &Value, key: &str) -> Result<f32, String> {
    v.as_f64()
        .map(|x| x as f32)
        .ok_or_else(|| format!("{key}: number expected, got {v}"))
}

fn flag(v: &Value, key: &str) -> Result<bool, String> {
    v.as_bool()
        .ok_or_else(|| format!("{key}: boolean expected, got {v}"))
}

/// A colour for roughr, or `None` for rough.js's `"none"` sentinel and for
/// `"transparent"` (roughr draws a stroke or fill whenever one is set). The
/// value itself never changes geometry; unparsed names become opaque black.
fn colour(v: &Value) -> Option<Srgba> {
    match v.as_str() {
        None | Some("") | Some("none") | Some("transparent") => None,
        Some(_) => Some(Srgba::new(0.0, 0.0, 0.0, 1.0)),
    }
}

/// rough.js 4.6.4's `fillStyle` names (`fillers/filter.js` `getFiller`,
/// which falls back to hachure).
fn fill_style(name: &str) -> FillStyle {
    match name {
        "solid" => FillStyle::Solid,
        "zigzag" => FillStyle::ZigZag,
        "cross-hatch" => FillStyle::CrossHatch,
        "dots" => FillStyle::Dots,
        "dashed" => FillStyle::Dashed,
        "zigzag-line" => FillStyle::ZigZagLine,
        _ => FillStyle::Hachure,
    }
}

/// roughr `Options` for rough.js 4.6.4 resolved options (a golden
/// drawable's `options`). Every key rough.js has is carried, including the
/// defaults where roughr's differ (bowing 1, not 2); keys roughr lacks
/// (`fillShapeRoughnessGain`) are dropped; unknown keys are an error.
pub fn roughr_options(resolved: &Value) -> Result<Options, String> {
    let mut o = Options {
        // rough.js has no default fill; roughr's Options::default has none
        // either. Stroke defaults to '#000' in rough.js.
        fill: None,
        stroke: Some(Srgba::new(0.0, 0.0, 0.0, 1.0)),
        ..Options::default()
    };
    let map = resolved
        .as_object()
        .ok_or_else(|| format!("options object expected, got {resolved}"))?;
    for (k, v) in map {
        match k.as_str() {
            "maxRandomnessOffset" => o.max_randomness_offset = Some(num(v, k)?),
            "roughness" => o.roughness = Some(num(v, k)?),
            "bowing" => o.bowing = Some(num(v, k)?),
            "stroke" => o.stroke = colour(v),
            "strokeWidth" => o.stroke_width = Some(num(v, k)?),
            "curveFitting" => o.curve_fitting = Some(num(v, k)?),
            "curveTightness" => o.curve_tightness = Some(num(v, k)?),
            "curveStepCount" => o.curve_step_count = Some(num(v, k)?),
            "fill" => o.fill = colour(v),
            "fillStyle" => {
                o.fill_style = Some(fill_style(
                    v.as_str().ok_or_else(|| format!("fillStyle: {v}"))?,
                ))
            }
            "fillWeight" => o.fill_weight = Some(num(v, k)?),
            "hachureAngle" => o.hachure_angle = Some(num(v, k)?),
            "hachureGap" => o.hachure_gap = Some(num(v, k)?),
            "simplification" => o.simplification = Some(num(v, k)?),
            "dashOffset" => o.dash_offset = Some(num(v, k)?),
            "dashGap" => o.dash_gap = Some(num(v, k)?),
            "zigzagOffset" => o.zigzag_offset = Some(num(v, k)?),
            "seed" => {
                o.seed = Some(
                    v.as_u64()
                        .ok_or_else(|| format!("seed: non-negative integer expected, got {v}"))?,
                )
            }
            "strokeLineDash" | "fillLineDash" => {
                let dash = v
                    .as_array()
                    .ok_or_else(|| format!("{k}: array expected"))?
                    .iter()
                    .map(|x| x.as_f64().ok_or_else(|| format!("{k}: {x}")))
                    .collect::<Result<Vec<_>, _>>()?;
                if k == "strokeLineDash" {
                    o.stroke_line_dash = Some(dash);
                } else {
                    o.fill_line_dash = Some(dash);
                }
            }
            "strokeLineDashOffset" => o.stroke_line_dash_offset = v.as_f64(),
            "fillLineDashOffset" => o.fill_line_dash_offset = v.as_f64(),
            "disableMultiStroke" => o.disable_multi_stroke = Some(flag(v, k)?),
            "disableMultiStrokeFill" => o.disable_multi_stroke_fill = Some(flag(v, k)?),
            "preserveVertices" => o.preserve_vertices = Some(flag(v, k)?),
            "fixedDecimalPlaceDigits" => o.fixed_decimal_place_digits = Some(num(v, k)?),
            // No roughr equivalent: rough.js adds it to the roughness of
            // solid curve and single-path fills (generator.js curve, path).
            "fillShapeRoughnessGain" => {}
            other => return Err(format!("option {other} is not a rough.js 4.6.4 option")),
        }
    }
    Ok(o)
}

// ---------------------------------------------------------------------------
// Running roughr

/// One op in the goldens' vocabulary (`move`, `lineTo`, `bcurveTo`).
#[derive(Clone, Debug, PartialEq)]
pub struct Op {
    pub op: String,
    pub data: Vec<f64>,
}

/// One op set in the goldens' vocabulary (`path`, `fillPath`, `fillSketch`).
#[derive(Clone, Debug, PartialEq)]
pub struct Set {
    pub kind: String,
    pub ops: Vec<Op>,
}

fn sets_of(d: Drawable<f64>) -> Vec<Set> {
    d.sets
        .into_iter()
        .map(|s| Set {
            kind: match s.op_set_type {
                OpSetType::Path => "path",
                OpSetType::FillPath => "fillPath",
                OpSetType::FillSketch => "fillSketch",
            }
            .to_owned(),
            ops: s
                .ops
                .into_iter()
                .map(|op| Op {
                    op: match op.op {
                        OpType::Move => "move",
                        OpType::LineTo => "lineTo",
                        OpType::BCurveTo => "bcurveTo",
                    }
                    .to_owned(),
                    data: op.data,
                })
                .collect(),
        })
        .collect()
}

fn number(v: &Value) -> Result<f64, String> {
    v.as_f64()
        .ok_or_else(|| format!("number expected, got {v}"))
}

fn points(v: &Value) -> Result<Vec<Point2D<f64>>, String> {
    v.as_array()
        .ok_or_else(|| format!("point list expected, got {v}"))?
        .iter()
        .map(|p| Ok(Point2D::new(number(&p[0])?, number(&p[1])?)))
        .collect()
}

/// `new RoughGenerator()[method](...args, options)` with roughr: the same
/// call on `roughr::generator::Generator` with [`roughr_options`]. A roughr
/// panic is an `Err` with its message.
pub fn run(method: &str, args: &[Value], resolved: &Value) -> Result<Vec<Set>, String> {
    let o = Some(roughr_options(resolved)?);
    let n = |i: usize| {
        args.get(i)
            .map_or(Err(format!("argument {i} missing")), number)
    };
    let g = Generator::default();
    let call = || -> Result<Drawable<f64>, String> {
        Ok(match method {
            "line" => g.line(n(0)?, n(1)?, n(2)?, n(3)?, &o),
            "rectangle" => g.rectangle(n(0)?, n(1)?, n(2)?, n(3)?, &o),
            "ellipse" => g.ellipse(n(0)?, n(1)?, n(2)?, n(3)?, &o),
            "circle" => g.circle(n(0)?, n(1)?, n(2)?, &o),
            "arc" => g.arc(
                n(0)?,
                n(1)?,
                n(2)?,
                n(3)?,
                n(4)?,
                n(5)?,
                args.get(6).and_then(Value::as_bool).unwrap_or(false),
                &o,
            ),
            "linearPath" => g.linear_path(&points(&args[0])?, false, &o),
            "polygon" => g.polygon(&points(&args[0])?, &o),
            "curve" => g.curve(&points(&args[0])?, &o),
            "path" => g.path(
                args[0]
                    .as_str()
                    .ok_or_else(|| format!("path data expected, got {}", args[0]))?
                    .to_owned(),
                &o,
            ),
            other => return Err(format!("unknown method {other}")),
        })
    };
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = catch_unwind(AssertUnwindSafe(call));
    std::panic::set_hook(hook);
    match result {
        Ok(r) => r.map(sets_of),
        Err(p) => Err(format!(
            "roughr panicked: {}",
            p.downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_default()
        )),
    }
}

// ---------------------------------------------------------------------------
// Comparing

/// The first place `actual` differs from the golden `sets`, as
/// `"<kind>: <where> expected ... actual ..."` (kind is `set count`, `set
/// type`, `op count`, `op kind` or `value`), or `None` when they match.
pub fn first_difference(expected: &Value, actual: &[Set], p: Precision) -> Option<String> {
    let empty = Vec::new();
    let exp = expected.as_array().unwrap_or(&empty);
    if exp.len() != actual.len() {
        return Some(format!(
            "set count: expected {} actual {}",
            exp.len(),
            actual.len()
        ));
    }
    for (si, (e, a)) in exp.iter().zip(actual).enumerate() {
        let kind = e["type"].as_str().unwrap_or_default();
        if kind != a.kind {
            return Some(format!(
                "set type: set {si} expected {kind} actual {}",
                a.kind
            ));
        }
        let eops = e["ops"].as_array().unwrap_or(&empty);
        if eops.len() != a.ops.len() {
            return Some(format!(
                "op count: set {si} ({kind}) expected {} actual {}",
                eops.len(),
                a.ops.len()
            ));
        }
        for (oi, (eo, ao)) in eops.iter().zip(&a.ops).enumerate() {
            let ek = eo["op"].as_str().unwrap_or_default();
            if ek != ao.op {
                return Some(format!(
                    "op kind: set {si} ({kind}) op {oi} expected {ek} actual {}",
                    ao.op
                ));
            }
            let edata = eo["data"].as_array().unwrap_or(&empty);
            for (di, (ev, av)) in edata.iter().zip(&ao.data).enumerate() {
                let ev = ev.as_f64().unwrap_or(f64::NAN);
                if !p.accepts(ev, *av) {
                    return Some(format!(
                        "value: set {si} ({kind}) op {oi} ({ek}) data[{di}] expected {ev} actual {av}"
                    ));
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Divergences

/// A reason roughr's output differs from rough.js 4.6.4's, found by reading
/// both sources; [`classify`] and [`Report`] decide which applies to a case.
#[derive(Debug)]
pub struct Divergence {
    pub id: &'static str,
    pub summary: &'static str,
}

/// Every divergence the evaluation attributes mismatches to. A case is
/// attributed to `rng` when roughr with rough.js's generator patched in
/// (`park-miller.patch`, `report-fork.json`) matches it; otherwise to the
/// divergence behind the first difference that remains with that patch.
pub const DIVERGENCES: &[Divergence] = &[
    Divergence {
        id: "rng",
        summary: "roughr draws from rand's StdRng (ChaCha12, seed_from_u64; core.rs Options::random) \
            and narrows every draw to f32; rough.js draws from its Park-Miller Random \
            (bin/math.js). Every random offset, bowing and diverge point differs, at every \
            roughness (the line diverge point 0.2 + random() * 0.2 is not scaled by roughness).",
    },
    Divergence {
        id: "svg-path",
        summary: "SVG path strokes: roughr's svg_path emits an extra move for every M command, \
            draws a move for a path that is only a move (rough.js draws nothing), and parses with \
            svgtypes and svg_path_ops instead of path-data-parser 0.1.0, so a path without a \
            leading M draws nothing and \"- 20\" (which rough.js rewrites to \"-20\") ends the path.",
    },
    Divergence {
        id: "path-simplification",
        summary: "Paths with simplification < 1: roughr's points_on_path samples and simplifies \
            differently from points-on-path 0.2.1, so the linear paths drawn instead of the \
            curve have other points; and roughr simplifies at simplification 0, which rough.js \
            treats as unset.",
    },
    Divergence {
        id: "path-draw-order",
        summary: "generator.path: rough.js sketches the stroke (svgPath) before the fill, roughr \
            the fill before the stroke, so each takes the other's random draws.",
    },
    Divergence {
        id: "solid-fill-shape",
        summary: "Solid fills of curves and single-subpath paths: rough.js 4.6 draws a \
            single-stroke sketch of the outline at roughness + fillShapeRoughnessGain and \
            merges it (_mergedShape); roughr fills the sampled polygon with \
            solid_fill_polygon, and has no fillShapeRoughnessGain.",
    },
    Divergence {
        id: "curve-reseed",
        summary: "Second stroke of a curve: rough.js's cloneOptionsAlterSeed drops the \
            randomizer and restarts at seed + 1; roughr's clone_options_alter_seed copies the \
            live randomizer, so the second stroke continues the first's stream.",
    },
    Divergence {
        id: "pattern-fill",
        summary: "Hachure, cross-hatch, zigzag, dashed and zigzag-line fills: rough.js 4.6.4 \
            computes the lines with hachure-fill 0.5.2 (and at roughness >= 1 draws once to \
            pick a skip offset); roughr keeps the older scan-line hachure, so the lines start \
            and end elsewhere and the dashed and zigzag-line fillers get other counts.",
    },
    Divergence {
        id: "f32",
        summary: "roughr's options are f32 and its constants go through f32 (_c: 0.2, \
            -0.0016668, f32::PI() * 2 as the ellipse and arc loop bound), so ellipses and arcs \
            gain or lose a point and some numbers land on the other side of a 0.005.",
    },
    Divergence {
        id: "seed-range",
        summary: "roughr's seed is a u64; rough.js takes any integer, so negative seeds have no \
            roughr equivalent.",
    },
    Divergence {
        id: "fill-sentinel",
        summary: "rough.js fills a rectangle, ellipse, polygon or arc for any non-empty fill \
            string, \"none\" included; roughr's fill is an Option<Srgba>, and an adapter maps \
            \"none\" and \"transparent\" to no fill.",
    },
];

fn set_kind(difference: &str) -> Option<&str> {
    let start = difference.find('(')? + 1;
    let end = start + difference[start..].find(')')?;
    Some(&difference[start..end])
}

fn values(difference: &str) -> Option<(f64, f64)> {
    let e = difference.split(" expected ").nth(1)?;
    let (e, a) = e.split_once(" actual ")?;
    Some((e.trim().parse().ok()?, a.trim().parse().ok()?))
}

/// The divergence behind `difference`, the first difference roughr with
/// rough.js's generator (`park-miller.patch`) still has on `case`.
pub fn classify(case: &Value, difference: &str) -> Vec<&'static str> {
    let method = case["method"].as_str().unwrap_or_default();
    let options = &case["drawable"]["options"];
    if difference.starts_with("error:") {
        return if options["seed"].as_i64().is_some_and(|s| s < 0) {
            vec!["seed-range"]
        } else {
            Vec::new()
        };
    }
    if difference.starts_with("set count:")
        && matches!(options["fill"].as_str(), Some("none" | "transparent"))
    {
        return vec!["fill-sentinel"];
    }
    if let Some((e, a)) = values(difference) {
        if (e - a).abs() <= 1e-4 * e.abs().max(1.0) {
            return vec!["f32"];
        }
    }
    let one_subpath = |d: &Value| {
        d.as_str()
            .is_some_and(|d| d.bytes().filter(|b| matches!(b, b'M' | b'm')).count() <= 1)
    };
    match (set_kind(difference), method) {
        (Some("fillSketch"), _) => vec!["pattern-fill"],
        (Some("fillPath"), "curve") => vec!["solid-fill-shape"],
        (Some("fillPath"), "path") if one_subpath(&case["args"][0]) => vec!["solid-fill-shape"],
        (Some("fillPath"), "path") => vec!["path-draw-order"],
        (Some("path"), "path") if options["simplification"].as_f64().is_some_and(|s| s < 1.0) => {
            vec!["path-simplification"]
        }
        (Some("path"), "path") => vec!["svg-path"],
        (Some("path"), "curve") => vec!["curve-reseed"],
        (Some("path"), "ellipse" | "circle" | "arc") => vec!["f32"],
        _ => Vec::new(),
    }
}

/// Whether the roughr this was built with draws from rough.js's Park-Miller
/// generator, that is, whether `park-miller.patch` is applied (`fork.py`).
pub fn roughr_is_park_miller() -> bool {
    let mut o = Options {
        seed: Some(1),
        ..Options::default()
    };
    // Random(1).next() in rough.js: (2^31 - 1) & 48271, over 2^31.
    o.random() == 48271.0 / 2147483648.0
}

// ---------------------------------------------------------------------------
// The evaluation

/// One golden case's result.
#[derive(Clone, Debug)]
pub struct CaseResult {
    pub file: String,
    pub id: String,
    pub roughness: Option<f64>,
    pub exact: bool,
    pub svg: bool,
    /// The first difference at two decimals (`None` when `svg`).
    pub difference: Option<String>,
    pub divergences: Vec<&'static str>,
    /// How many ops the golden has (random.json: how many draws).
    pub golden_ops: usize,
}

/// Per-file or total counts.
#[derive(Clone, Debug, PartialEq)]
pub struct Counts {
    pub name: String,
    pub cases: usize,
    pub exact: usize,
    pub svg: usize,
}

impl Counts {
    /// Percent of cases matching at two decimals.
    pub fn percent_svg(&self) -> f64 {
        if self.cases == 0 {
            0.0
        } else {
            100.0 * self.svg as f64 / self.cases as f64
        }
    }
}

/// The whole evaluation.
#[derive(Clone, Debug)]
pub struct Report {
    pub cases: Vec<CaseResult>,
}

fn load(dir: &Path, name: &str) -> Value {
    let path = dir.join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn random_case(case: &Value) -> CaseResult {
    // rough.js: `new Random(seed).next()`; roughr: `Options::random`, a
    // StdRng seeded with seed_from_u64.
    let seed = case["seed"].as_u64().expect("seed");
    let values = case["values"].as_array().expect("values");
    let mut o = Options {
        seed: Some(seed),
        ..Options::default()
    };
    let actual: Vec<f64> = values.iter().map(|_| o.random()).collect();
    let diff = |p: Precision| {
        values
            .iter()
            .zip(&actual)
            .enumerate()
            .find_map(|(i, (e, a))| {
                let e = e.as_f64().expect("value");
                (!p.accepts(e, *a)).then(|| format!("value: draw {i} expected {e} actual {a}"))
            })
    };
    let difference = diff(Precision::Decimals(2));
    CaseResult {
        file: "random.json".to_owned(),
        id: case["id"]
            .as_str()
            .map_or_else(|| format!("seed {seed}"), str::to_owned),
        roughness: None,
        exact: diff(Precision::Exact).is_none(),
        svg: difference.is_none(),
        difference,
        divergences: Vec::new(),
        golden_ops: values.len(),
    }
}

fn rough_case(file: &str, case: &Value) -> CaseResult {
    let id = case["id"].as_str().expect("id").to_owned();
    let method = case["method"].as_str().expect("method");
    let args = case["args"].as_array().expect("args");
    let resolved = &case["drawable"]["options"];
    let expected = &case["drawable"]["sets"];
    let (exact, difference) = match run(method, args, resolved) {
        Ok(sets) => (
            first_difference(expected, &sets, Precision::Exact).is_none(),
            first_difference(expected, &sets, Precision::Decimals(2)),
        ),
        Err(e) => (false, Some(format!("error: {e}"))),
    };
    CaseResult {
        file: file.to_owned(),
        id,
        roughness: resolved["roughness"].as_f64(),
        exact,
        svg: difference.is_none(),
        difference,
        divergences: Vec::new(),
        golden_ops: expected.as_array().map_or(0, |sets| {
            sets.iter()
                .map(|s| s["ops"].as_array().map_or(0, Vec::len))
                .sum()
        }),
    }
}

/// The report `--write` writes for the roughr this was built with:
/// `report.json` for roughr 0.14.0 as published, `report-fork.json` with
/// `park-miller.patch` applied (`fork.py`).
pub fn report_name() -> &'static str {
    if roughr_is_park_miller() {
        "report-fork.json"
    } else {
        "report.json"
    }
}

/// The first differences of the patched roughr, from the committed
/// `report-fork.json`: `(file, id) -> difference`.
fn fork_differences(root: &Path) -> BTreeMap<(String, String), String> {
    let fork = load(&root.join("tools/roughr-eval"), "report-fork.json");
    fork["mismatches"]
        .as_array()
        .expect("report-fork.json mismatches")
        .iter()
        .map(|m| {
            (
                (
                    m["file"].as_str().expect("file").to_owned(),
                    m["id"].as_str().expect("id").to_owned(),
                ),
                m["difference"].as_str().expect("difference").to_owned(),
            )
        })
        .collect()
}

/// Replays every case of [`FILES`] in `<root>/goldens` and attributes each
/// mismatch to a [`Divergence`]. Built with the published roughr, a
/// mismatch the patched roughr does not have (`report-fork.json` under
/// `root`) is `rng`; any other is [`classify`]'d by the difference the
/// patched roughr still has. Built with the patch, every mismatch is
/// classified by its own difference.
pub fn evaluate(root: &Path) -> Report {
    let goldens = root.join("goldens");
    let fork = roughr_is_park_miller();
    let remaining = if fork {
        BTreeMap::new()
    } else {
        fork_differences(root)
    };
    let mut cases = Vec::new();
    for &file in FILES {
        let doc = load(&goldens, file);
        for case in doc["cases"].as_array().expect("cases") {
            let mut r = if file == "random.json" {
                random_case(case)
            } else {
                rough_case(file, case)
            };
            if let Some(d) = &r.difference {
                r.divergences = if fork {
                    classify(case, d)
                } else {
                    match remaining.get(&(r.file.clone(), r.id.clone())) {
                        None => vec!["rng"],
                        Some(left) => classify(case, left),
                    }
                };
            }
            cases.push(r);
        }
    }
    Report { cases }
}

/// ADR-003's rule: adopt at 100 %, fork at a small, fixable divergence
/// (at least [`FORK_THRESHOLD`] percent matching), port otherwise.
pub fn recommend(percent_svg: f64) -> &'static str {
    if percent_svg >= 100.0 {
        "adopt"
    } else if percent_svg >= FORK_THRESHOLD {
        "fork"
    } else {
        "port"
    }
}

fn counts<'a>(name: &str, cases: impl Iterator<Item = &'a CaseResult>) -> Counts {
    let mut c = Counts {
        name: name.to_owned(),
        cases: 0,
        exact: 0,
        svg: 0,
    };
    for r in cases {
        c.cases += 1;
        c.exact += usize::from(r.exact);
        c.svg += usize::from(r.svg);
    }
    c
}

impl Report {
    /// Counts per file, in [`FILES`] order.
    pub fn files(&self) -> Vec<Counts> {
        FILES
            .iter()
            .map(|f| counts(f, self.cases.iter().filter(|c| c.file == *f)))
            .collect()
    }

    /// Counts over every case.
    pub fn total(&self) -> Counts {
        counts("total", self.cases.iter())
    }

    /// How many of the matching cases are goldens with no ops at all
    /// (`stroke: "none"` without fill, empty paths, short point lists).
    pub fn matched_without_ops(&self) -> usize {
        self.cases
            .iter()
            .filter(|c| c.svg && c.golden_ops == 0)
            .count()
    }

    /// The two-decimal match rate as ADR-003 prints it (`"12.3 %"`).
    pub fn percent_svg_text(&self) -> String {
        format!("{:.1} %", self.total().percent_svg())
    }

    /// `report.json`.
    pub fn to_json(&self) -> Value {
        let counts_json = |c: &Counts| {
            json!({
                "name": c.name, "cases": c.cases, "exact": c.exact, "svg": c.svg,
            })
        };
        let total = self.total();
        let mut roughness = Map::new();
        for r in [0.0, 1.0, 2.0] {
            let c = counts(
                &format!("roughness {r}"),
                self.cases.iter().filter(|c| c.roughness == Some(r)),
            );
            roughness.insert(
                format!("{r}"),
                json!({"cases": c.cases, "exact": c.exact, "svg": c.svg}),
            );
        }
        let divergences: Vec<Value> = DIVERGENCES
            .iter()
            .map(|d| {
                json!({
                    "id": d.id,
                    "summary": d.summary,
                    "cases": self.cases.iter().filter(|c| c.divergences.contains(&d.id)).count(),
                })
            })
            .collect();
        let mismatches: Vec<Value> = self
            .cases
            .iter()
            .filter(|c| !c.svg)
            .map(|c| {
                json!({
                    "file": c.file,
                    "id": c.id,
                    "difference": c.difference,
                    "divergences": c.divergences,
                })
            })
            .collect();
        let (description, roughr) = if roughr_is_park_miller() {
            (
                "ex-206: roughr 0.14.0 with park-miller.patch (rough.js's Random in place of \
                 StdRng) against the rough.js 4.6.4 goldens. Written by \
                 `python3 tools/roughr-eval/fork.py --write`; quoted on ADR-003.",
                "0.14.0 + park-miller.patch",
            )
        } else {
            (
                "ex-206: roughr 0.14.0 against the rough.js 4.6.4 goldens. Written by \
                 `cargo run --manifest-path tools/roughr-eval/Cargo.toml -- --write`; \
                 quoted on ADR-003.",
                "0.14.0",
            )
        };
        json!({
            "description": description,
            "roughr": roughr,
            "reference": "roughjs 4.6.4 (goldens/manifest.json)",
            "match": "svg: every set type, op kind and number equal after +x.toFixed(2) \
                (upstream SVG export precision); exact: numbers within relative 1e-10",
            "total": {
                "cases": total.cases, "exact": total.exact, "svg": total.svg,
                "svg_drawing_nothing": self.matched_without_ops(),
                "percent_svg": self.percent_svg_text(),
            },
            "recommendation": recommend(total.percent_svg()),
            "files": self.files().iter().map(counts_json).collect::<Vec<_>>(),
            "by_roughness": roughness,
            "divergences": divergences,
            "mismatches": mismatches,
        })
    }
}
