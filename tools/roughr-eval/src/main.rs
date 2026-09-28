//! `cargo run --manifest-path tools/roughr-eval/Cargo.toml -- [--write | --check] [--root DIR]`
//!
//! Replays the rough.js goldens through roughr and prints the summary.
//! `--write` writes the report (`tools/roughr-eval/report.json`, or
//! `report-fork.json` when built with `park-miller.patch` by `fork.py`);
//! `--check` exits 1 if the committed report differs from a fresh run.
//! `--root` is the repository root (default: two levels above this crate).

use std::path::PathBuf;
use std::process::ExitCode;

use roughr_eval::{evaluate, recommend, report_name};

fn main() -> ExitCode {
    let mut mode = None;
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--write" | "--check" => mode = Some(a),
            "--root" => match args.next() {
                Some(dir) => root = PathBuf::from(dir),
                None => return usage("--root needs a directory"),
            },
            other => return usage(other),
        }
    }
    let root = match root.canonicalize() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}: {e}", root.display());
            return ExitCode::FAILURE;
        }
    };
    let report = evaluate(&root);
    println!("{}", report_name());
    for f in report.files() {
        println!(
            "{:<24} {:>4} cases  {:>4} exact  {:>4} at two decimals",
            f.name, f.cases, f.exact, f.svg
        );
    }
    let total = report.total();
    println!(
        "{:<24} {:>4} cases  {:>4} exact  {:>4} at two decimals ({}); recommendation: {}",
        "total",
        total.cases,
        total.exact,
        total.svg,
        report.percent_svg_text(),
        recommend(total.percent_svg())
    );
    let mut text = serde_json::to_string_pretty(&report.to_json()).expect("json");
    text.push('\n');
    let out = root.join("tools/roughr-eval").join(report_name());
    match mode.as_deref() {
        Some("--write") => {
            if let Err(e) = std::fs::write(&out, text) {
                eprintln!("{}: {e}", out.display());
                return ExitCode::FAILURE;
            }
            println!("wrote {}", out.display());
            ExitCode::SUCCESS
        }
        Some(_) => {
            if std::fs::read_to_string(&out).ok().as_deref() == Some(text.as_str()) {
                println!("{} is current", out.display());
                ExitCode::SUCCESS
            } else {
                eprintln!("{} is stale; rerun with --write", out.display());
                ExitCode::FAILURE
            }
        }
        None => ExitCode::SUCCESS,
    }
}

fn usage(bad: &str) -> ExitCode {
    eprintln!("usage: roughr-eval [--write | --check] [--root DIR] (got {bad})");
    ExitCode::from(2)
}
