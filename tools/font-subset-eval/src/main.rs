//! Writes (`--write`) or checks (`--check`) report.json, printing the table
//! either way, or writes browser/render.mjs's input (`--browser FILE`).

use std::path::PathBuf;
use std::process::ExitCode;

use font_subset_eval::decision::{
    candidate_table, decide, load_browser, load_wasm, scene_table, BROWSER, WASM,
};
use font_subset_eval::report::{browser_cases, evaluate, percent, report_text, Totals, REPORT};

fn usage(arg: &str) -> ExitCode {
    eprintln!(
        "font-subset-eval: unexpected argument {arg}\n\
         usage: font-subset-eval [--root DIR] [--write | --check | --browser FILE]"
    );
    ExitCode::from(2)
}

fn row(name: &str, t: &Totals, upstream_bytes: usize) {
    println!(
        "{:<22} {:>9} {:>7} {:>6} {:>8} {:>11} {:>9} {:>9} {:>6} {:>6}",
        name,
        t.bytes,
        percent(t.bytes, upstream_bytes),
        t.failed,
        format!("{}/{}", t.full, t.declarations),
        format!("{}/{}", t.kept, t.code_points),
        t.glyphs_equal,
        format!("{}/{}", t.runs_equal, t.runs),
        t.woff2_identical,
        t.sfnt_identical
    );
}

fn main() -> ExitCode {
    let mut mode = None;
    let mut browser = None;
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--write" | "--check" => mode = Some(a),
            "--browser" => match args.next() {
                Some(file) => browser = Some(PathBuf::from(file)),
                None => return usage("--browser needs a file"),
            },
            "--root" => match args.next() {
                Some(dir) => root = PathBuf::from(dir),
                None => return usage("--root needs a directory"),
            },
            other => return usage(other),
        }
    }
    if let Some(file) = browser {
        // Fonts and runs for browser/render.mjs; not committed.
        let cases = browser_cases(&root);
        std::fs::write(&file, serde_json::to_string(&cases).expect("json"))
            .expect("write the browser cases");
        println!("wrote {}", file.display());
        return ExitCode::SUCCESS;
    }
    let report = evaluate(&root);
    let up = report.upstream_totals.bytes;
    println!(
        "{:<22} {:>9} {:>7} {:>6} {:>8} {:>11} {:>9} {:>9} {:>6} {:>6}",
        "candidate",
        "bytes",
        "% up",
        "failed",
        "full",
        "kept",
        "glyphs=",
        "runs=",
        "woff2=",
        "sfnt="
    );
    row("upstream", &report.upstream_totals, up);
    for (c, t) in &report.candidates {
        row(&c.name(), t, up);
    }
    if root.join(WASM).exists() && root.join(BROWSER).exists() {
        // ADR-010's decision and the tables it quotes.
        let (wasm, browser) = (load_wasm(&root), load_browser(&root));
        let decided = decide(&report, &wasm, &browser);
        println!("\n**Decision: `{}`**\n", decided.name());
        for line in candidate_table(&report, &wasm, &browser) {
            println!("{line}");
        }
        println!();
        for line in scene_table(&report, decided) {
            println!("{line}");
        }
        println!();
    }
    let text = report_text(&report);
    let path = root.join(REPORT);
    match mode.as_deref() {
        Some("--write") => {
            std::fs::write(&path, text).expect("write report.json");
            println!("wrote {REPORT}");
        }
        Some("--check") => {
            if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
                eprintln!(
                    "stale: {REPORT}; run cargo run --release --manifest-path tools/font-subset-eval/Cargo.toml -- --write"
                );
                return ExitCode::FAILURE;
            }
            println!("{REPORT} is current");
        }
        _ => {}
    }
    ExitCode::SUCCESS
}
