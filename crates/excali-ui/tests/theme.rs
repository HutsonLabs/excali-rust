//! The theme tokens (ex-532): upstream's `css/theme.scss` custom
//! properties, light on `.excalidraw` and dark on `.excalidraw.theme--dark`,
//! as the embedded stylesheet declares them, held to
//! `tests/fixtures/theme-tokens.json` (`tools/goldens/theme-tokens.mjs`
//! compiles theme.scss at the pin), and every token the design system's
//! tokens page (`site/content/design-system/tokens.md`) lists found there
//! with the value the page gives.

use std::collections::BTreeMap;

use excali_scene::shape::Theme;
use excali_ui::theme::{
    container_tokens, dark_tokens, large_screen_tokens, light_tokens, mobile_tokens, theme_class,
    tokens, RIGHT_SIDEBAR_WIDTH, THEME_DARK_CLASS,
};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/theme-tokens.json")).unwrap()
}

fn pairs(v: &Value) -> Vec<(String, String)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let p = p.as_array().unwrap();
            (p[0].as_str().unwrap().into(), p[1].as_str().unwrap().into())
        })
        .collect()
}

#[test]
fn light_tokens_are_theme_scss_on_excalidraw() {
    let f = fixture();
    assert_eq!(light_tokens(), pairs(&f["light"]));
    assert!(light_tokens().len() > 120);
}

#[test]
fn dark_tokens_are_theme_scss_on_excalidraw_theme_dark() {
    let f = fixture();
    assert_eq!(dark_tokens(), pairs(&f["dark"]));
    assert!(dark_tokens().len() > 60);
}

#[test]
fn mobile_and_large_screen_tokens() {
    let f = fixture();
    assert_eq!(mobile_tokens(), pairs(&f["mobile"]));
    assert_eq!(large_screen_tokens(), pairs(&f["largeScreen"]));
}

#[test]
fn container_tokens_are_app_tsx_inline_style() {
    let f = fixture();
    let want: Vec<(String, String)> = f["container"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().into()))
        .collect();
    let got: Vec<(String, String)> = container_tokens()
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    assert_eq!(got, want);
    assert_eq!(RIGHT_SIDEBAR_WIDTH, 302.0);
}

#[test]
fn effective_tokens_overlay_dark_on_light_last_declaration_winning() {
    let light: BTreeMap<_, _> = tokens(Theme::Light).into_iter().collect();
    let dark: BTreeMap<_, _> = tokens(Theme::Dark).into_iter().collect();
    // every light name survives, dark redeclares its own
    assert_eq!(light.len(), dark.len());
    assert_eq!(light["--color-primary"], "#6965db");
    assert_eq!(dark["--color-primary"], "#a8a5ff");
    assert_eq!(light["--space-factor"], dark["--space-factor"]);
    assert_eq!(dark["--color-surface-mid"], "hsl(240 6% 10%)");
    // declared twice (theme.scss:158, 162): one entry
    let n = tokens(Theme::Light)
        .iter()
        .filter(|(k, _)| k == "--color-surface-primary-container")
        .count();
    assert_eq!(n, 1);
    for (k, v) in dark_tokens() {
        assert_eq!(dark[&k], v, "{k}");
    }
}

#[test]
fn theme_classes_follow_app_tsx() {
    assert_eq!(THEME_DARK_CLASS, "theme--dark");
    assert_eq!(theme_class(Theme::Light), None);
    assert_eq!(theme_class(Theme::Dark), Some("theme--dark"));
}

// -- the tokens page ------------------------------------------------------------

/// A value as the page writes it, comparable with sass's output: lower
/// case, commas as spaces, whitespace collapsed, `.5` as `0.5`, three-digit
/// hex expanded, a bare `--name` as `var(--name)`.
fn norm(v: &str) -> String {
    let v = v.trim().to_lowercase().replace(',', " ");
    let v = v.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    let mut prev = ' ';
    for c in v.chars() {
        if c == '.' && !prev.is_ascii_digit() {
            out.push('0');
        }
        out.push(c);
        prev = c;
    }
    if out.len() == 4 && out.starts_with('#') {
        out = out[1..].chars().flat_map(|c| [c, c]).collect();
        out.insert(0, '#');
    }
    if out.starts_with("--") {
        out = format!("var({out})");
    }
    out
}

/// A value cell without its parenthesised notes (` (`2.25rem` at ≥ 1921 px)`),
/// parentheses inside code spans kept.
fn without_notes(cell: &str) -> &str {
    let mut code = false;
    for (i, c) in cell.char_indices() {
        match c {
            '`' => code = !code,
            '(' if !code => return &cell[..i],
            _ => {}
        }
    }
    cell
}

fn backticked(cell: &str) -> Vec<&str> {
    cell.split('`').skip(1).step_by(2).collect()
}

/// The names a table's first cell lists: `--a` / `--b`, `--a` / `-suffix`
/// (a suffix of the first) and `--a-1/2/3`.
fn names(cell: &str) -> Vec<String> {
    let parts: Vec<&str> = backticked(cell);
    let mut out = Vec::new();
    let Some(first) = parts.first() else {
        return out;
    };
    for part in &parts {
        if part.starts_with("--") {
            if let Some((stem, rest)) = part.split_once('/') {
                let base = stem.trim_end_matches(|c: char| c.is_ascii_digit());
                out.push(stem.to_string());
                for n in rest.split('/') {
                    out.push(format!("{base}{n}"));
                }
            } else {
                out.push(part.to_string());
            }
        } else if part.starts_with('-') {
            out.push(format!("{first}{part}"));
        }
    }
    out
}

struct Row {
    names: Vec<String>,
    light: Vec<String>,
    dark: Vec<String>,
}

/// The rows of the page's tables whose first cell names tokens.
fn table_rows(page: &str) -> Vec<Row> {
    page.lines()
        .filter(|l| l.starts_with("| `--"))
        .map(|l| {
            let cells: Vec<&str> = l.trim_matches('|').split(" | ").collect();
            let vals = |i: usize| {
                cells
                    .get(i)
                    .map(|c| backticked(without_notes(c)).into_iter().map(norm).collect())
                    .unwrap_or_default()
            };
            Row {
                names: names(cells[0]),
                light: vals(1),
                dark: vals(2),
            }
        })
        .collect()
}

const PAGE: &str = include_str!("../../../site/content/design-system/tokens.md");

fn check_values(names: &[String], values: &[String], got: &BTreeMap<String, String>, what: &str) {
    if values.is_empty() {
        return;
    }
    assert!(
        values.len() == 1 || values.len() == names.len(),
        "{what} {names:?}: {} values for {} names",
        values.len(),
        names.len()
    );
    for (i, name) in names.iter().enumerate() {
        let want = &values[if values.len() == 1 { 0 } else { i }];
        assert_eq!(&norm(&got[name]), want, "{what} {name}");
    }
}

#[test]
fn every_table_token_on_the_page_is_emitted_light_and_dark() {
    let light: BTreeMap<_, _> = tokens(Theme::Light).into_iter().collect();
    let dark_only: BTreeMap<_, _> = dark_tokens().into_iter().collect();
    let dark: BTreeMap<_, _> = tokens(Theme::Dark).into_iter().collect();
    let container: BTreeMap<String, String> = container_tokens()
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    let rows = table_rows(PAGE);
    assert!(rows.len() > 45, "{} rows", rows.len());
    let mut seen = 0;
    for row in &rows {
        let from_container = row.names.iter().all(|n| container.contains_key(n));
        let source = if from_container { &container } else { &light };
        for n in &row.names {
            assert!(source.contains_key(n), "{n} is not emitted on .excalidraw");
            seen += 1;
        }
        check_values(&row.names, &row.light, source, "light");
        if !row.dark.is_empty() {
            for n in &row.names {
                assert!(
                    dark_only.contains_key(n),
                    "{n} has a dark value on the page but no .excalidraw.theme--dark declaration"
                );
            }
            check_values(&row.names, &row.dark, &dark, "dark");
        }
    }
    assert!(seen > 55, "{seen} names");
}

#[test]
fn the_page_gray_scale_is_emitted() {
    let light: BTreeMap<_, _> = tokens(Theme::Light).into_iter().collect();
    let line = PAGE
        .lines()
        .find(|l| l.starts_with("Gray scale"))
        .expect("the gray scale line");
    let (_, list) = line.split_once(": ").unwrap();
    let mut n = 0;
    for entry in list.trim_end_matches('.').split(", ") {
        let (step, value) = entry.split_once(' ').unwrap();
        let value = value.trim_matches('`');
        assert_eq!(
            norm(&light[&format!("--color-gray-{step}")]),
            norm(value),
            "--color-gray-{step}"
        );
        n += 1;
    }
    assert_eq!(n, 11);
}

#[test]
fn the_page_semantic_sets_are_emitted() {
    let light = tokens(Theme::Light);
    let dark = tokens(Theme::Dark);
    let line = PAGE
        .lines()
        .find(|l| l.starts_with("Semantic sets"))
        .expect("the semantic sets line");
    let (_, list) = line.split_once(": ").unwrap();
    let mut sets = 0;
    // `--color-x*` (values; dark values), in turn
    for chunk in list.split(", `--") {
        let chunk = chunk.trim_start_matches('`');
        let (prefix, values) = chunk.split_once('`').unwrap();
        let prefix = format!("--{}", prefix.trim_start_matches("--").trim_end_matches('*'));
        // `--color-badge` `#0b6513` on `#d3ffd2`: the background's token is
        // `--background-color-badge`
        let background = format!("--background-{}", &prefix[2..]);
        let of = |set: &[(String, String)]| -> Vec<String> {
            set.iter()
                .filter(|(k, _)| k.starts_with(&prefix) || *k == background)
                .map(|(_, v)| norm(v))
                .collect()
        };
        assert!(!of(&light).is_empty(), "{prefix}");
        let (light_part, dark_part) = values.split_once("; dark").unwrap_or((values, ""));
        for v in backticked(light_part) {
            assert!(of(&light).contains(&norm(v)), "{prefix}: {v}");
        }
        for v in backticked(dark_part) {
            assert!(of(&dark).contains(&norm(v)), "{prefix} dark: {v}");
        }
        sets += 1;
    }
    assert_eq!(sets, 5);
}
