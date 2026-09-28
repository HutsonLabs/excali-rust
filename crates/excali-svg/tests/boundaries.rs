//! ADR-008: backends know nothing about elements. The SVG writer's library
//! code depends on `excali-scene` only (by package name, as `cargo metadata`
//! resolves it), reaches into it only through `excali_scene::display`, and
//! no identifier in it names an element. The tests may build their scenes
//! from elements (`excali-core`, through `excali_scene::export`) and draw
//! rough.js shapes (`excali-rough`), as the callers of the writer do.

#[test]
fn no_element_knowledge() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let (normal, all) = workspace_dependencies(std::path::Path::new(manifest));
    assert_eq!(normal, ["excali-scene"], "dependencies: {normal:?}");
    assert_eq!(
        all,
        ["excali-core", "excali-rough", "excali-scene"],
        "all: {all:?}"
    );
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut sources = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let code = code_without_comments(&std::fs::read_to_string(&path).unwrap());
        sources += 1;
        for p in paths(&code) {
            for s in p.split("::") {
                assert!(!s.contains("Element"), "{} names {s}", path.display());
            }
        }
        for r in roots(&code, |w| w.starts_with("excali_")) {
            assert_eq!(r, "excali_scene::display", "{} uses {r}", path.display());
        }
    }
    assert!(sources > 0);
}

#[test]
fn the_checks_see_code_and_skip_comments() {
    let code = code_without_comments(
        "use excali_core::element::TextElement; // renderElement.ts\n/* ElementKind */ let x = 1;",
    );
    assert_eq!(
        paths(&code),
        ["use", "excali_core::element::TextElement", "let", "x", "1"]
    );
    assert_eq!(
        roots(
            "use excali_scene::display::{Path}; use excali_scene::{export::X}; use excali_core::x;",
            |w| w.starts_with("excali_")
        ),
        ["excali_scene::display", "excali_scene::{", "excali_core::x"]
    );
}

/// The workspace packages the package at `manifest` depends on: its normal
/// dependencies, and all of them (dev, build and target-specific too), by
/// package name, from `cargo metadata --format-version 1 --no-deps`.
fn workspace_dependencies(manifest: &std::path::Path) -> (Vec<String>, Vec<String>) {
    let out = std::process::Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "cargo metadata: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let members: Vec<&str> = packages
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    let manifest = manifest.canonicalize().unwrap();
    let this = packages
        .iter()
        .find(|p| {
            std::path::Path::new(p["manifest_path"].as_str().unwrap())
                .canonicalize()
                .is_ok_and(|m| m == manifest)
        })
        .unwrap();
    let internal: Vec<&serde_json::Value> = this["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| members.contains(&d["name"].as_str().unwrap()) || d.get("path").is_some())
        .collect();
    let names = |normal_only: bool| {
        let mut v: Vec<String> = internal
            .iter()
            .filter(|d| !normal_only || (d["kind"].is_null() && d["target"].is_null()))
            .map(|d| d["name"].as_str().unwrap().to_string())
            .collect();
        v.sort();
        v.dedup();
        v
    };
    (names(true), names(false))
}

/// The code of a Rust source with `//` and `/* */` comments removed.
fn code_without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let line = rest.find("//");
        let block = rest.find("/*");
        match (line, block) {
            (Some(l), b) if b.is_none_or(|b| l < b) => {
                out.push_str(&rest[..l]);
                rest = rest[l..].find('\n').map_or("", |e| &rest[l + e..]);
            }
            (_, Some(b)) => {
                out.push_str(&rest[..b]);
                rest = rest[b..].find("*/").map_or("", |e| &rest[b + e + 2..]);
            }
            _ => {
                out.push_str(rest);
                rest = "";
            }
        }
    }
    out
}

/// Every identifier and `::` path in `code`.
fn paths(code: &str) -> Vec<String> {
    code.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
        .map(|p| p.trim_matches(':'))
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Where every path that starts at a root `is_root` accepts leads:
/// `root::next`, `root)` for a visibility, or `root` alone.
fn roots(code: &str, is_root: impl Fn(&str) -> bool) -> Vec<String> {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut rest = code;
    while let Some(start) = rest.find(ident) {
        let len = rest[start..]
            .find(|c: char| !ident(c))
            .unwrap_or(rest.len() - start);
        let word = &rest[start..start + len];
        rest = &rest[start + len..];
        if !is_root(word) {
            continue;
        }
        let after = rest.trim_start();
        out.push(match after.strip_prefix("::").map(str::trim_start) {
            Some(next) => {
                let n = next.find(|c: char| !ident(c)).unwrap_or(next.len());
                let segment = match n {
                    0 => next.chars().next().map_or(String::new(), String::from),
                    _ => next[..n].to_owned(),
                };
                format!("{word}::{segment}")
            }
            None if after.starts_with(')') => format!("{word})"),
            None => word.to_owned(),
        });
    }
    out
}
