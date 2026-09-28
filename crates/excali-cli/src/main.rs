//! Command-line export of `.excalidraw` files.
//!
//! Upstream counterpart: none (new). Targets: native. Internal dependencies
//! allowed by the architecture overview (`site/content/architecture/overview.md`,
//! ADR-008): `excali-raster`, `excali-svg`. Export subcommands arrive with those
//! crates; until then the binary answers `--version` and `--help`.

use std::process::ExitCode;

const USAGE: &str = "usage: excali [--version | --help]";

fn run(args: &[String]) -> Result<String, String> {
    match args {
        [] => Err(USAGE.to_string()),
        [flag] if flag == "--version" || flag == "-V" => {
            Ok(format!("excali {}", env!("CARGO_PKG_VERSION")))
        }
        [flag] if flag == "--help" || flag == "-h" => Ok(USAGE.to_string()),
        [other, ..] => Err(format!("excali: unrecognised argument '{other}'\n{USAGE}")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(out) => {
            println!("{out}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(2)
        }
    }
}
