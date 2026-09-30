//! Hints, cursor hints and the welcome screen (ex-528): upstream's
//! `HintViewer` (`components/HintViewer.tsx`), `CursorHint` and
//! `CursorHints` (`components/CursorHint.tsx`,
//! `positionElementBesideCursor.ts`) and `WelcomeScreen`
//! (`components/welcome-screen/*`, `ExcalidrawLogo.tsx`), held to the DOM
//! and decisions upstream's own code makes.
//!
//! Fixture: `tests/fixtures/hints.json`, written by
//! `tools/goldens/hints.mjs` from the pinned checkout (React 19.0.0 into
//! jsdom, once per platform): per case the HintViewer input (app state
//! keys over the defaults, the selected elements, isMobile, canFitSidebar,
//! the gridModeEnabled prop, the resize handle) and its DOM; CursorHints'
//! choices over event sequences; positionElementBesideCursor over a grid;
//! CursorHint's DOM shown and fading; and WelcomeScreen's four parts per
//! form factor, with the actions its menu items execute. The stylesheets
//! and the logo markup are the same generator's.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_editor::actions::ActionName;
use excali_editor::tools::{ArrowType, KeyHintKind, ToolType};
use excali_scene::shape::Theme;
use excali_ui::dom::Node;
use excali_ui::editor_interface::FormFactor;
use excali_ui::hints::{
    cursor_hint, get_hints, hint_text, hint_viewer, position_element_beside_cursor,
    ContainerRect, CursorHints, HintContext, CURSOR_HINT_COOLDOWN, HINTS_CSS,
};
use excali_ui::welcome_screen::{
    excalidraw_logo, help_hint, menu_hint, toolbar_hint, welcome_screen_center, welcome_text,
    WelcomeScreenEvent, WelcomeScreenProps, EXCALIDRAW_LOGO_HTML, WELCOME_SCREEN_CSS,
};
use serde_json::{json, Map, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/hints.json")).unwrap()
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
/// markup of that icon (held to React's by tests/icons.rs) and `{logo:
/// true}` to the logo.
fn expand(v: &Value) -> Value {
    match v {
        Value::Object(o) if o.contains_key("icon") => {
            let name = o["icon"].as_str().unwrap();
            let icon = excali_ui::icons::icon(name).unwrap_or_else(|| panic!("no icon {name}"));
            tree(&Node::Element(icon.element(Theme::Light).unwrap()))
        }
        Value::Object(o) if o.contains_key("logo") => tree(&Node::Element(excalidraw_logo())),
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

fn same(what: &str, expected: &Value, actual: &Value) {
    if expected != actual {
        panic!(
            "{what}:\nexpected {}\nactual   {}",
            serde_json::to_string(expected).unwrap(),
            serde_json::to_string(actual).unwrap()
        );
    }
}

// -- hint viewer ----------------------------------------------------------------

struct HintInput {
    app_state: AppState,
    selected: Vec<Element>,
    is_mobile: bool,
    can_fit_sidebar: bool,
    grid_mode_enabled: Option<bool>,
    active_resize_handle: Option<String>,
}

fn hint_input(case: &Value) -> HintInput {
    let input = &case["input"];
    let mut app_state = AppState::default();
    for (k, v) in input["appState"].as_object().unwrap() {
        app_state.insert(k.clone(), v.clone());
    }
    let selected = input["selected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Element::from_map(e.as_object().unwrap().clone()).unwrap())
        .collect();
    HintInput {
        app_state,
        selected,
        is_mobile: input["isMobile"].as_bool().unwrap(),
        can_fit_sidebar: input["canFitSidebar"].as_bool().unwrap(),
        grid_mode_enabled: input["gridModeEnabled"].as_bool(),
        active_resize_handle: input["activeResizeHandle"].as_str().map(str::to_owned),
    }
}

fn context(input: &HintInput) -> HintContext<'_> {
    HintContext {
        app_state: &input.app_state,
        selected_elements: &input.selected,
        is_mobile: input.is_mobile,
        can_fit_sidebar: input.can_fit_sidebar,
        grid_mode_enabled: input.grid_mode_enabled,
        active_resize_handle: input.active_resize_handle.as_deref(),
    }
}

fn platforms(key: &str) -> Vec<Value> {
    fixture()[key].as_array().unwrap().clone()
}

#[test]
fn every_hint_case_renders_upstreams_dom() {
    let platforms = platforms("hintViewer");
    assert_eq!(platforms.len(), 2);
    let mut seen = 0;
    for platform in &platforms {
        let darwin = platform["darwin"].as_bool().unwrap();
        for case in platform["cases"].as_array().unwrap() {
            let name = format!("{} {}", platform["platform"], case["name"]);
            let input = hint_input(case);
            let actual = hint_viewer(&context(&input), darwin)
                .map(|el| tree(&Node::Element(el)))
                .unwrap_or(Value::Null);
            same(&name, &case["dom"], &actual);
            seen += 1;
        }
    }
    assert!(seen >= 120, "{seen} cases");
}

#[test]
fn every_hint_case_picks_the_labelled_keys() {
    for case in platforms("hintViewer")[0]["cases"].as_array().unwrap() {
        let input = hint_input(case);
        let keys: Vec<&str> = get_hints(&context(&input)).iter().map(|h| h.key).collect();
        let want: Vec<&str> = case["hints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        assert_eq!(keys, want, "case {}", case["name"]);
    }
}

#[test]
fn the_cases_reach_about_thirty_hint_keys() {
    // research ui-design-system.md 3.11: "about 30 contextual hint keys"
    let mut keys: Vec<String> = Vec::new();
    for case in platforms("hintViewer")[0]["cases"].as_array().unwrap() {
        for k in case["hints"].as_array().unwrap() {
            let k = k.as_str().unwrap().to_owned();
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    assert!(keys.len() >= 29, "{keys:?}");
}

#[test]
fn hint_strings_are_upstreams_english() {
    let fixture = fixture();
    for (key, text) in fixture["locale"].as_object().unwrap() {
        let text = text.as_str().unwrap();
        if key.starts_with("hints.") || key == "keys.mmb" {
            assert_eq!(hint_text(key), text, "{key}");
        } else {
            assert_eq!(welcome_text(key), text, "{key}");
        }
    }
}

// -- cursor hints ---------------------------------------------------------------

fn arrow_type(name: &str) -> ArrowType {
    match name {
        "sharp" => ArrowType::Sharp,
        "round" => ArrowType::Round,
        "elbow" => ArrowType::Elbow,
        other => panic!("arrow type {other}"),
    }
}

#[test]
fn cursor_hints_policy_matches_upstream() {
    let fixture = fixture();
    assert_eq!(fixture["cursorHint"]["cooldown"], json!(CURSOR_HINT_COOLDOWN));
    for seq in fixture["cursorHints"].as_array().unwrap() {
        let mut hints = CursorHints::default();
        let events = seq["events"].as_array().unwrap();
        let shown = seq["shown"].as_array().unwrap();
        for (i, (e, want)) in events.iter().zip(shown).enumerate() {
            let now = e["at"].as_f64().unwrap();
            let pos = (e["x"].as_f64().unwrap(), e["y"].as_f64().unwrap());
            let event = e["event"].as_array().unwrap();
            let icon = if event[0] == "cycled" {
                hints.on_arrow_type_cycled(arrow_type(event[1].as_str().unwrap()), now, pos)
            } else {
                let tool = if event[1] == "line" {
                    ToolType::Line
                } else {
                    ToolType::Arrow
                };
                let source = if event[2] == "digit" {
                    KeyHintKind::Digit
                } else {
                    KeyHintKind::Letter
                };
                let current = arrow_type(e["arrowType"].as_str().unwrap());
                hints.on_tool_shortcut(tool, source, current, now, pos)
            };
            assert_eq!(
                icon.map(|i| i.name),
                want.as_str(),
                "{} event {i}",
                seq["name"]
            );
        }
    }
}

#[test]
fn cursor_hint_positions_match_upstream() {
    let fixture = fixture();
    let cases = fixture["position"].as_array().unwrap();
    assert_eq!(cases.len(), 42);
    for c in cases {
        let f = |v: &Value, k: &str| v[k].as_f64().unwrap();
        let container = ContainerRect {
            left: f(&c["container"], "left"),
            top: f(&c["container"], "top"),
            width: f(&c["container"], "width"),
            height: f(&c["container"], "height"),
        };
        let got = position_element_beside_cursor(
            (f(&c["cursor"], "x"), f(&c["cursor"], "y")),
            (f(&c["element"], "width"), f(&c["element"], "height")),
            container,
            f(c, "gap"),
        );
        assert_eq!(got, (f(c, "left"), f(c, "top")), "{c}");
    }
}

#[test]
fn cursor_hint_dom_matches_upstream() {
    let fixture = fixture();
    let icon = excali_ui::icons::icon("roundArrowIcon").unwrap();
    for (key, fading) in [("shown", false), ("fading", true)] {
        let expected = expand(&fixture["cursorHint"][key]);
        let actual = tree(&Node::Element(cursor_hint(icon, fading, 0.0, 0.0)));
        same(key, &expected, &actual);
    }
    let moved = cursor_hint(icon, false, 12.5, 30.0);
    assert_eq!(
        moved.style_properties(),
        &[("transform".to_owned(), "translate(12.5px, 30px)".to_owned())]
    );
}

// -- welcome screen -------------------------------------------------------------

fn welcome_props(case: &Value, darwin: bool) -> WelcomeScreenProps {
    WelcomeScreenProps {
        form_factor: match case["formFactor"].as_str().unwrap() {
            "phone" => FormFactor::Phone,
            "tablet" => FormFactor::Tablet,
            _ => FormFactor::Desktop,
        },
        view_mode_enabled: case["viewModeEnabled"].as_bool().unwrap(),
        is_darwin: darwin,
        on_event: None,
    }
}

#[test]
fn welcome_screen_parts_render_upstreams_dom() {
    let platforms = platforms("welcome");
    assert_eq!(platforms.len(), 2);
    for platform in &platforms {
        let darwin = platform["darwin"].as_bool().unwrap();
        for case in platform["cases"].as_array().unwrap() {
            let name = format!("{} {}", platform["platform"], case["name"]);
            let props = welcome_props(case, darwin);
            let parts = [
                ("center", welcome_screen_center(&props)),
                ("menuHint", menu_hint()),
                ("toolbarHint", toolbar_hint()),
                ("helpHint", help_hint()),
            ];
            for (key, el) in parts {
                same(
                    &format!("{name} {key}"),
                    &expand(&case[key]),
                    &tree(&Node::Element(el)),
                );
            }
        }
    }
}

#[test]
fn welcome_screen_menu_items_run_upstreams_actions() {
    for case in platforms("welcome")[0]["cases"].as_array().unwrap() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        let mut props = welcome_props(case, false);
        props.on_event = Some(Rc::new(move |e: WelcomeScreenEvent| sink.borrow_mut().push(e)));
        let center = welcome_screen_center(&props);
        let items = excali_ui::welcome_screen::menu_items(&props);
        assert_eq!(center.children().len(), 3);
        let want: Vec<(String, String)> = case["clicks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                (
                    c["text"].as_str().unwrap().to_owned(),
                    c["executed"][0].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        let got: Vec<(String, String)> = items
            .iter()
            .map(|e| (welcome_text(e.label_key()).to_owned(), e.action().name().to_owned()))
            .collect();
        assert_eq!(got, want, "case {}", case["name"]);
        assert!(items
            .iter()
            .all(|e| matches!(e.action(), ActionName::LoadScene | ActionName::ToggleShortcuts)));
    }
}

#[test]
fn the_logo_is_reacts_markup() {
    assert_eq!(
        format!("{}\n", Node::Element(excalidraw_logo()).to_html()),
        EXCALIDRAW_LOGO_HTML
    );
}

#[test]
fn stylesheets_are_upstreams() {
    assert!(HINTS_CSS.contains(".excalidraw .HintViewer {"));
    assert!(HINTS_CSS.contains(".excalidraw .CursorHint {"));
    assert!(WELCOME_SCREEN_CSS.contains(".welcome-screen-center"));
    assert!(WELCOME_SCREEN_CSS.contains(".ExcalidrawLogo"));
}
