//! The help dialog (ex-522): upstream's `HelpDialog`
//! (`components/HelpDialog.tsx`) in its `Dialog` and `Modal`, held to the
//! DOM upstream leaves, and the shortcuts page
//! (`site/content/design-system/shortcuts.md`, section "Help dialog") held
//! to what the dialog shows.
//!
//! Fixture: `tests/fixtures/help-dialog.json`, upstream's HelpDialog at the
//! pinned commit rendered by React 19.0.0 into jsdom once per platform
//! (`tools/goldens/help-dialog.mjs`): per case the platform, the theme
//! action's predicate, the form factor and theme, and the element tree the
//! dialog portals to the body (attributes and inline style as maps, icons
//! as `{icon: name}`); the islands as the dialog shows them; and the app
//! state patches of closing it. `src/help_dialog/help_dialog.css` is the
//! same generator's HelpDialog.scss.

use std::collections::BTreeMap;

use excali_core::app_state::AppState;
use excali_scene::shape::Theme;
use excali_ui::dom::Node;
use excali_ui::help_dialog::{
    close_help_dialog, help_dialog, help_text, shortcut_islands, shortcut_keys,
    shortcuts_page_section, HelpDialogProps, Platform, HELP_DIALOG_CSS,
};
use serde_json::{json, Map, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/help-dialog.json")).unwrap()
}

fn cases() -> Vec<Value> {
    fixture()["cases"].as_array().unwrap().clone()
}

// -- the tree the fixture records ---------------------------------------------

fn tree(node: &Node) -> Value {
    match node {
        Node::Text(text) => Value::String(text.clone()),
        Node::Element(el) => {
            let attrs: BTreeMap<_, _> = el
                .attributes()
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            let style: BTreeMap<_, _> = el
                .style_properties()
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            let mut out = Map::new();
            out.insert("tag".into(), json!(el.tag()));
            out.insert("attrs".into(), json!(attrs));
            out.insert("style".into(), json!(style));
            let children = el.children().iter().map(tree).collect();
            out.insert("children".into(), Value::Array(merge_text(children)));
            Value::Object(out)
        }
    }
}

fn merge_text(nodes: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for n in nodes {
        match (out.last_mut(), &n) {
            (Some(Value::String(prev)), Value::String(text)) => prev.push_str(text),
            _ => out.push(n),
        }
    }
    out.retain(|n| n.as_str() != Some(""));
    out
}

/// The fixture's tree with each `{icon: name}` expanded to excali-ui's
/// markup of that icon (held to React's by tests/icons.rs).
fn expand(v: &Value) -> Value {
    match v {
        Value::Object(o) if o.contains_key("icon") => {
            let name = o["icon"].as_str().unwrap();
            let icon = excali_ui::icons::icon(name).unwrap_or_else(|| panic!("no icon {name}"));
            tree(&Node::Element(icon.element(Theme::Light).unwrap()))
        }
        Value::Object(o) => {
            let mut out = Map::new();
            for (k, x) in o {
                if k == "children" {
                    let kids = x.as_array().unwrap().iter().map(expand).collect();
                    out.insert(k.clone(), Value::Array(merge_text(kids)));
                } else {
                    out.insert(k.clone(), x.clone());
                }
            }
            Value::Object(out)
        }
        _ => v.clone(),
    }
}

// -- props from the fixture ---------------------------------------------------

fn platform(case: &Value) -> Platform {
    let flag = |k: &str| case[k].as_bool().unwrap();
    Platform {
        darwin: flag("darwin"),
        windows: flag("windows"),
        firefox: flag("firefox"),
        clipboard_blob: flag("clipboardBlob"),
    }
}

fn props(case: &Value) -> HelpDialogProps {
    HelpDialogProps {
        platform: platform(case),
        toggle_theme_enabled: case["toggleTheme"].as_bool().unwrap(),
        container_id: "excalidraw-id".into(),
        theme: if case["theme"] == "dark" {
            Theme::Dark
        } else {
            Theme::Light
        },
        phone: case["formFactor"] == "phone",
        on_close: None,
    }
}

// -- DOM parity ---------------------------------------------------------------

#[test]
fn every_case_renders_upstreams_dom() {
    let cases = cases();
    assert_eq!(cases.len(), 12);
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let expected: Vec<Value> = case["dom"].as_array().unwrap().iter().map(expand).collect();
        let actual = vec![tree(&Node::Element(help_dialog(props(case))))];
        if actual != expected {
            panic!(
                "case {name}:\nexpected {}\nactual   {}",
                serde_json::to_string(&expected).unwrap(),
                serde_json::to_string(&actual).unwrap()
            );
        }
    }
}

#[test]
fn the_locale_strings_are_upstreams() {
    let locale = fixture()["locale"].as_object().unwrap().clone();
    assert!(locale.len() >= 90, "{} strings", locale.len());
    for (key, value) in locale {
        assert_eq!(help_text(&key), value.as_str().unwrap(), "{key}");
    }
}

/// The islands as the fixture reads them from the DOM: every `kbd` a key,
/// the `or` text between alternatives (a sequence, `isOr={false}`, leaves
/// no text, so its keys run together).
fn islands_as_shown(platform: Platform, toggle_theme: bool) -> Value {
    let islands: Vec<Value> = shortcut_islands(platform, toggle_theme)
        .iter()
        .map(|island| {
            let rows: Vec<Value> = island
                .rows
                .iter()
                .map(|row| {
                    let groups: Vec<Vec<String>> =
                        row.shortcuts.iter().map(|s| shortcut_keys(s)).collect();
                    let (keys, separators) = if row.is_or {
                        let n = groups.len().saturating_sub(1);
                        (groups, vec![help_text("helpDialog.or"); n])
                    } else {
                        (vec![groups.concat()], Vec::new())
                    };
                    json!({ "label": row.label, "keys": keys, "separators": separators })
                })
                .collect();
            json!({
                "className": format!("HelpDialog__island {}", island.class_name),
                "caption": island.caption,
                "rows": rows,
            })
        })
        .collect();
    Value::Array(islands)
}

#[test]
fn the_islands_are_upstreams_on_every_platform() {
    let f = fixture();
    for case in f["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let expected = f["islands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == name)
            .unwrap()["islands"]
            .clone();
        let actual = islands_as_shown(platform(case), case["toggleTheme"].as_bool().unwrap());
        assert_eq!(actual, expected, "case {name}");
    }
}

#[test]
fn shortcut_keys_split_like_upstream() {
    // `Shortcut` (HelpDialog.tsx:104-111): "++" ends in the "+" key, and
    // upperCaseSingleChars upper-cases the first lone lowercase letter only
    assert_eq!(shortcut_keys("Ctrl++"), ["Ctrl", "+"]);
    assert_eq!(shortcut_keys("Ctrl+-"), ["Ctrl", "-"]);
    assert_eq!(shortcut_keys("Shift+s"), ["Shift", "S"]);
    assert_eq!(shortcut_keys("a b"), ["A b"]);
    assert_eq!(shortcut_keys("PgUp/PgDn"), ["PgUp/PgDn"]);
    assert_eq!(shortcut_keys("Space+drag"), ["Space", "drag"]);
}

#[test]
fn closing_clears_the_menu_and_the_dialog() {
    for close in fixture()["close"].as_array().unwrap() {
        let mut app_state = AppState::default();
        app_state.insert("openDialog", json!({ "name": "help" }));
        app_state.insert("openMenu", json!("canvas"));
        close_help_dialog(&mut app_state);
        for patch in close["patches"].as_array().unwrap() {
            for (k, v) in patch.as_object().unwrap() {
                assert_eq!(app_state.get(k), Some(v), "{} {k}", close["how"]);
            }
        }
    }
}

#[test]
fn the_stylesheet_is_help_dialog_scss() {
    assert!(HELP_DIALOG_CSS.starts_with("/* Generated by tools/goldens/help-dialog.mjs"));
    assert!(HELP_DIALOG_CSS.contains(".excalidraw .HelpDialog__islands-container {"));
    assert!(HELP_DIALOG_CSS.contains(".excalidraw .HelpDialog__key {"));
}

// -- the shortcuts page -------------------------------------------------------

const SHORTCUTS_PAGE: &str = include_str!("../../../site/content/design-system/shortcuts.md");

/// The page's "Help dialog" section: from its heading to the next `## `
/// (empty when the page has none).
fn page_section() -> String {
    let Some(start) = SHORTCUTS_PAGE.find("\n## Help dialog\n") else {
        return String::new();
    };
    let rest = &SHORTCUTS_PAGE[start + 1..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |i| i + 3);
    rest[..end].to_owned()
}

#[test]
fn the_shortcuts_page_lists_what_the_dialog_shows() {
    let expected = shortcuts_page_section();
    let actual = page_section();
    assert_eq!(
        actual, expected,
        "site/content/design-system/shortcuts.md's \"Help dialog\" section differs from \
         the dialog; the section should read:\n{expected}"
    );
}

#[test]
fn the_shortcuts_page_section_has_every_row_of_the_dialog() {
    let section = shortcuts_page_section();
    for island in shortcut_islands(
        Platform {
            clipboard_blob: true,
            ..Platform::default()
        },
        true,
    ) {
        assert!(
            section.contains(&format!("\n### {}\n", island.caption)),
            "{}",
            island.caption
        );
        for row in &island.rows {
            assert!(
                section.contains(&format!("\n| {} | ", row.label)),
                "{}",
                row.label
            );
        }
    }
}
