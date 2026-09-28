//! ADR-010's rule, applied to report.json, browser.json and wasm.json, and
//! the tables the ADR quotes.
//!
//! The port subsets a face with a candidate when the candidate
//!
//! 1. is faithful on every face of `upstream-subsets.json`: nothing fails,
//!    every code point the original maps is kept with the original's
//!    outline and advance, the font-wide metrics are the original's, and
//!    every run of the scene's text shapes to the same drawn glyphs
//!    ([`crate::report::Totals::faithful`]);
//! 2. in Chromium, loads wherever upstream's own subset loads and draws
//!    every run pixel for pixel as the original face does (browser.json);
//! 3. builds for wasm32-unknown-unknown, a target of excali-svg
//!    (`site/content/architecture/overview.md`), with its subsetter and its
//!    encoder each and together (wasm.json).
//!
//! Of the candidates that pass, the one that inlines the fewest bytes. If
//! none passes, the port keeps inlining whole files.

use std::path::Path;

use serde_json::Value;

use crate::report::{percent, Candidate, Report, Totals};

pub const WASM: &str = "tools/font-subset-eval/wasm.json";
pub const BROWSER: &str = "tools/font-subset-eval/browser.json";

/// One build of wasm.json.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WasmBuild {
    /// Comma-separated Cargo features, or `none`.
    pub features: String,
    pub builds: bool,
    pub bytes: Option<usize>,
    pub gzip: Option<usize>,
}

/// One candidate of browser.json.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrowserTotals {
    pub declarations: usize,
    pub failed: usize,
    pub loaded: usize,
    pub runs: usize,
    pub runs_equal: usize,
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn load_wasm(root: &Path) -> Vec<WasmBuild> {
    read_json(&root.join(WASM))["builds"]
        .as_array()
        .expect("builds")
        .iter()
        .map(|b| WasmBuild {
            features: b["features"].as_str().expect("features").to_string(),
            builds: b["builds"].as_bool().expect("builds"),
            bytes: b["bytes"].as_u64().map(|n| n as usize),
            gzip: b["gzip"].as_u64().map(|n| n as usize),
        })
        .collect()
}

pub fn load_browser(root: &Path) -> Vec<(String, BrowserTotals)> {
    let v = read_json(&root.join(BROWSER));
    let n =
        |t: &Value, k: &str| t[k].as_u64().unwrap_or_else(|| panic!("browser.json: {k}")) as usize;
    v["candidates"]
        .as_object()
        .expect("candidates")
        .iter()
        .map(|(name, t)| {
            (
                name.clone(),
                BrowserTotals {
                    declarations: n(t, "declarations"),
                    failed: n(t, "failed"),
                    loaded: n(t, "loaded"),
                    runs: n(t, "runs"),
                    runs_equal: n(t, "runsEqual"),
                },
            )
        })
        .collect()
}

fn build<'a>(wasm: &'a [WasmBuild], features: &str) -> Option<&'a WasmBuild> {
    wasm.iter().find(|b| b.features == features)
}

/// Rule 3: the candidate's features build for wasm32 each alone and, when
/// wasm.json has the pair, together. Whole files need no feature.
pub fn builds_for_wasm(wasm: &[WasmBuild], candidate: Candidate) -> bool {
    let features = candidate.features();
    if features.is_empty() {
        return true;
    }
    features
        .iter()
        .all(|f| build(wasm, f).is_some_and(|b| b.builds))
        && build(wasm, &features.join(",")).is_none_or(|b| b.builds)
}

/// Rule 2: loads wherever upstream's subset loads and draws every run as the
/// original does. Whole files are the original.
pub fn faithful_in_browser(browser: &[(String, BrowserTotals)], candidate: Candidate) -> bool {
    if candidate == Candidate::Whole {
        return true;
    }
    let get = |name: &str| browser.iter().find(|(n, _)| n == name).map(|(_, t)| t);
    match (get(&candidate.name()), get("upstream")) {
        (Some(t), Some(up)) => {
            t.failed == 0
                && t.loaded == up.loaded
                && t.declarations == up.declarations
                && t.runs_equal == t.runs
        }
        _ => false,
    }
}

/// Rules 1 to 3 for one candidate.
pub fn eligible(
    totals: &Totals,
    wasm: &[WasmBuild],
    browser: &[(String, BrowserTotals)],
    candidate: Candidate,
) -> bool {
    totals.faithful() && faithful_in_browser(browser, candidate) && builds_for_wasm(wasm, candidate)
}

/// The decision: the eligible candidate with the fewest bytes (the first on
/// a tie), whole files when none is eligible.
pub fn decide(
    report: &Report,
    wasm: &[WasmBuild],
    browser: &[(String, BrowserTotals)],
) -> Candidate {
    report
        .candidates
        .iter()
        .filter(|(c, t)| *c != Candidate::Whole && eligible(t, wasm, browser, *c))
        .min_by_key(|(_, t)| t.bytes)
        .map(|(c, _)| *c)
        .unwrap_or(Candidate::Whole)
}

fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The wasm32 cell of a candidate: raw and gzipped bytes of its build, or
/// "does not build".
fn wasm_cell(wasm: &[WasmBuild], candidate: Candidate) -> String {
    let features = candidate.features();
    if features.is_empty() {
        return "nothing to build".into();
    }
    if !builds_for_wasm(wasm, candidate) {
        let failing: Vec<&str> = features
            .iter()
            .copied()
            .filter(|f| !build(wasm, f).is_some_and(|b| b.builds))
            .collect();
        return format!("does not build ({})", failing.join(", "));
    }
    let joined = features.join(",");
    match build(wasm, &joined) {
        Some(WasmBuild {
            bytes: Some(b),
            gzip: Some(g),
            ..
        }) => format!("{} / {}", thousands(*b), thousands(*g)),
        _ => "builds".into(),
    }
}

fn browser_cell(browser: &[(String, BrowserTotals)], candidate: Candidate) -> String {
    if candidate == Candidate::Whole {
        return "(the original)".into();
    }
    match browser.iter().find(|(n, _)| *n == candidate.name()) {
        Some((_, t)) => format!(
            "{}/{} loaded, {}/{}",
            t.loaded, t.declarations, t.runs_equal, t.runs
        ),
        None => "not run".into(),
    }
}

/// The candidate table of ADR-010, one markdown row per line: upstream's
/// own subset, then every candidate in report order.
pub fn candidate_table(
    report: &Report,
    wasm: &[WasmBuild],
    browser: &[(String, BrowserTotals)],
) -> Vec<String> {
    let up = &report.upstream_totals;
    let row = |name: String, t: &Totals, browser_cell: String, wasm_cell: String| {
        format!(
            "| {name} | {} | {} | {}/{} | {}/{} | {}/{} | {}/{} | {browser_cell} | {wasm_cell} |",
            thousands(t.bytes),
            percent(t.bytes, up.bytes),
            t.full,
            t.declarations,
            t.kept,
            t.code_points,
            t.glyphs_equal,
            t.code_points,
            t.runs_equal,
            t.runs,
        )
    };
    let mut out = vec![
        "| candidate | bytes | % of upstream | faces faithful | code points kept | glyphs equal | runs shaped alike | Chromium: loaded, runs pixel-equal | wasm32 bytes (raw / gzip) |".to_string(),
        "|---|---|---|---|---|---|---|---|---|".to_string(),
    ];
    let up_browser = match browser.iter().find(|(n, _)| n == "upstream") {
        Some((_, t)) => format!(
            "{}/{} loaded, {}/{}",
            t.loaded, t.declarations, t.runs_equal, t.runs
        ),
        None => "not run".into(),
    };
    out.push(row(
        "upstream (harfbuzzjs 0.3.6)".into(),
        up,
        up_browser,
        "(its own wasm, below)".into(),
    ));
    for (c, t) in &report.candidates {
        out.push(row(
            format!("`{}`", c.name()),
            t,
            browser_cell(browser, *c),
            wasm_cell(wasm, *c),
        ));
    }
    out
}

/// The scenes ADR-010 quotes one by one: the drawings of ordinary size.
pub const QUOTED_SCENES: [&str; 11] = [
    "fixture-default",
    "labels-excalifont",
    "ascii-excalifont",
    "ascii-virgil",
    "ascii-cascadia",
    "ascii-nunito",
    "ascii-lilita",
    "ascii-comic-shanns",
    "ascii-liberation",
    "latin1-excalifont",
    "cjk-paragraph",
];

/// Per quoted scene: faces inlined and the bytes of upstream, whole files
/// and the decided candidate.
pub fn scene_table(report: &Report, decided: Candidate) -> Vec<String> {
    let index = |c: Candidate| {
        report
            .candidates
            .iter()
            .position(|(x, _)| *x == c)
            .expect("candidate in report")
    };
    let (whole, chosen) = (index(Candidate::Whole), index(decided));
    let mut out = vec![
        format!(
            "| scene | faces | upstream | whole files | `{}` |",
            decided.name()
        ),
        "|---|---|---|---|---|".to_string(),
    ];
    for name in QUOTED_SCENES {
        let i = report
            .scenes
            .iter()
            .position(|s| s.name == name && s.font_face == "browser")
            .unwrap_or_else(|| panic!("no scene {name}"));
        let (up, bytes) = &report.scene_bytes[i];
        out.push(format!(
            "| `{name}` | {} | {} | {} | {} |",
            report.scenes[i].declarations.len(),
            thousands(*up),
            thousands(bytes[whole]),
            thousands(bytes[chosen]),
        ));
    }
    out
}
