//! The subsetter and encoder excali-svg ships are the releases ADR-010
//! measured (site/content/decisions/adr-010-svg-font-subsetting.md, "What
//! would reverse it"). tools/font-subset-eval pins its candidates exactly in
//! its own manifest and lockfile; these tests hold the workspace to the same
//! pins, so a `cargo update` cannot move skera, skrifa, ttf2woff2 or anything
//! they pull in to a version the evaluation never ran.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The crates excali-svg's subsetting depends on, as ADR-010 names them.
const EVALUATED: [&str; 3] = ["skera", "skrifa", "ttf2woff2"];

fn read(path: &str) -> String {
    let path = Path::new(ROOT).join(path);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The version requirement a manifest's `[dependencies]` or
/// `[workspace.dependencies]` declares for `name`, whether written
/// `name = "req"` or `name = { version = "req", ... }`.
fn requirement(manifest: &str, name: &str) -> String {
    let mut section = "";
    let line = manifest
        .lines()
        .filter(|l| {
            if l.starts_with('[') {
                section = l.trim();
            }
            section == "[dependencies]" || section == "[workspace.dependencies]"
        })
        .find(|l| l.split_once('=').is_some_and(|(key, _)| key.trim() == name))
        .unwrap_or_else(|| panic!("{name} is not declared"));
    let value = line.split_once('=').unwrap().1.trim();
    let quoted = match value.strip_prefix('{') {
        Some(table) => {
            let at = table
                .find("version")
                .unwrap_or_else(|| panic!("{name} has no version: {line}"));
            table[at + "version".len()..]
                .trim_start()
                .strip_prefix('=')
                .unwrap()
                .trim_start()
        }
        None => value,
    };
    let quoted = quoted.strip_prefix('"').unwrap();
    quoted[..quoted.find('"').unwrap()].to_string()
}

type Package = (String, String);

/// A Cargo.lock's packages, each with the packages it depends on.
fn lockfile(text: &str) -> BTreeMap<Package, Vec<String>> {
    let mut packages = BTreeMap::new();
    for block in text.split("[[package]]").skip(1) {
        let field = |key: &str| {
            block
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{key} = \"")))
                .map(|v| v.trim_end_matches('"').to_string())
        };
        let (name, version) = (field("name").unwrap(), field("version").unwrap());
        let dependencies = match block.find("dependencies = [") {
            Some(at) => {
                let list = &block[at + "dependencies = [".len()..];
                list[..list.find(']').unwrap()]
                    .split(',')
                    .map(|d| d.trim().trim_matches('"').to_string())
                    .filter(|d| !d.is_empty())
                    .collect()
            }
            None => Vec::new(),
        };
        packages.insert((name, version), dependencies);
    }
    packages
}

/// The package a lockfile dependency entry (`name` or `name version`) names.
fn resolve(packages: &BTreeMap<Package, Vec<String>>, entry: &str) -> Package {
    let mut parts = entry.split(' ');
    let name = parts.next().unwrap();
    let version = parts.next();
    let found: Vec<&Package> = packages
        .keys()
        .filter(|(n, v)| n == name && version.is_none_or(|want| v == want))
        .collect();
    assert_eq!(found.len(), 1, "{entry} is ambiguous or missing: {found:?}");
    found[0].clone()
}

/// Every package reachable from `roots`, with its version.
fn closure(packages: &BTreeMap<Package, Vec<String>>, roots: &[Package]) -> BTreeSet<Package> {
    let mut seen = BTreeSet::new();
    let mut todo = roots.to_vec();
    while let Some(package) = todo.pop() {
        if seen.insert(package.clone()) {
            for entry in &packages[&package] {
                todo.push(resolve(packages, entry));
            }
        }
    }
    seen
}

#[test]
fn the_workspace_pins_the_evaluated_releases_exactly() {
    let workspace = read("Cargo.toml");
    let evaluation = read("tools/font-subset-eval/Cargo.toml");
    for name in EVALUATED {
        let evaluated = requirement(&evaluation, name);
        assert!(
            evaluated.starts_with('='),
            "tools/font-subset-eval declares {name} {evaluated}, not an exact pin"
        );
        assert_eq!(
            requirement(&workspace, name),
            evaluated,
            "the workspace's {name} is not the release ADR-010 evaluated"
        );
    }
}

#[test]
fn the_workspace_lockfile_resolves_what_the_evaluation_resolved() {
    let workspace = lockfile(&read("Cargo.lock"));
    let evaluation = lockfile(&read("tools/font-subset-eval/Cargo.lock"));
    let evaluation_manifest = read("tools/font-subset-eval/Cargo.toml");
    let roots: Vec<Package> = EVALUATED
        .iter()
        .map(|name| {
            let version = requirement(&evaluation_manifest, name);
            (
                name.to_string(),
                version.trim_start_matches('=').to_string(),
            )
        })
        .collect();
    let shipped = closure(&workspace, &roots);
    let evaluated = closure(&evaluation, &roots);
    let missing: Vec<&Package> = evaluated.difference(&shipped).collect();
    assert!(
        missing.is_empty(),
        "the workspace resolves skera, skrifa and ttf2woff2 without these releases \
         tools/font-subset-eval measured: {missing:?}"
    );
    // The workspace may reach more packages than the evaluation: features
    // other workspace crates turn on (hashbrown's default hasher, foldhash,
    // for one) add dependencies. None of them may be another release of a
    // package the evaluation ran.
    let names: BTreeSet<&str> = evaluated.iter().map(|(n, _)| n.as_str()).collect();
    let other: Vec<&Package> = shipped
        .difference(&evaluated)
        .filter(|(n, _)| names.contains(n.as_str()))
        .collect();
    assert!(
        other.is_empty(),
        "the workspace resolves other releases than tools/font-subset-eval measured: {other:?}"
    );
}

#[test]
fn the_checks_read_manifests_and_lockfiles() {
    let manifest = "[features]\nskera = [\"dep:skera\"]\n[dependencies]\na = \"1.0\"\n\
                    skera = { version = \"=0.7.0\", default-features = false }\n\
                    skerax = \"2\"\nskrifa = \"=0.47.0\"\n[dev-dependencies]\nb = \"1\"\n";
    assert_eq!(requirement(manifest, "skera"), "=0.7.0");
    assert_eq!(requirement(manifest, "skrifa"), "=0.47.0");
    let lock = "version = 4\n\n[[package]]\nname = \"a\"\nversion = \"1.0.0\"\n\
                dependencies = [\n \"b 2.0.0\",\n \"c\",\n]\n\n\
                [[package]]\nname = \"b\"\nversion = \"2.0.0\"\n\n\
                [[package]]\nname = \"b\"\nversion = \"3.0.0\"\n\n\
                [[package]]\nname = \"c\"\nversion = \"0.1.0\"\n\
                dependencies = [\n \"b 3.0.0\",\n]\n";
    let packages = lockfile(lock);
    let reached = closure(&packages, &[("a".into(), "1.0.0".into())]);
    let versions: Vec<String> = reached.iter().map(|(n, v)| format!("{n} {v}")).collect();
    assert_eq!(versions, ["a 1.0.0", "b 2.0.0", "b 3.0.0", "c 0.1.0"]);
}
