//! The footer (ex-521): upstream's desktop `Footer`
//! (`components/footer/Footer.tsx`) with `ZoomActions`, `UndoRedoActions`
//! and `ExitZenModeButton` (`components/Actions.tsx`), the `HelpButton`,
//! and the zoom and history actions' panels, held to the DOM upstream
//! leaves.
//!
//! Fixture: `tests/fixtures/footer.json`, upstream's Footer at the pinned
//! commit rendered through its ActionManager by React 19.0.0 into jsdom
//! (`tools/goldens/footer.mjs`): per case the props, app state and history,
//! and the element tree (attributes and inline style as maps, icons as
//! `{icon: name}`, the host's slots as `<template data-slot>`); the label
//! each tooltip shows; and per control the action a click ran with the app
//! state it returned. `src/footer/footer.css` is the same generator's
//! footer rules of styles.scss, LayerUI.scss, Actions.scss,
//! FooterCenter.scss and app.scss.

use std::collections::BTreeMap;

use excali_core::app_state::AppState;
use excali_editor::actions::ActionName;
use excali_editor::viewport::{perform_zoom_action, Offsets, ViewportState};
use excali_scene::shape::Theme;
use excali_ui::dom::{Element, Node};
use excali_ui::footer::{
    footer, footer_text, reset_zoom_label, toggle_shortcuts, toggle_zen_mode, FooterControl,
    FooterProps, FOOTER_CSS,
};
use serde_json::{json, Map, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/footer.json")).unwrap()
}

fn cases() -> Vec<Value> {
    fixture()["cases"].as_array().unwrap().clone()
}

fn case(name: &str) -> Value {
    cases().into_iter().find(|c| c["name"] == name).unwrap()
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

fn slot(name: &str) -> Node {
    Node::Element(Element::new("template").attr("data-slot", name))
}

fn props(case: &Value) -> FooterProps {
    let p = &case["props"];
    let app = &case["appState"];
    let flag = |v: &Value| v.as_bool().unwrap();
    FooterProps {
        zoom: app["zoom"].as_f64().unwrap(),
        zen_mode: flag(&app["zenModeEnabled"]),
        view_mode: flag(&app["viewModeEnabled"]),
        undo_stack_empty: !flag(&case["undo"]),
        redo_stack_empty: !flag(&case["redo"]),
        navigation_enabled: flag(&case["navigation"]),
        show_exit_zen_mode_button: flag(&p["showExitZenModeBtn"]),
        render_welcome_screen: flag(&p["renderWelcomeScreen"]),
        default_ui_enabled: flag(&p["defaultUIEnabled"]),
        zoom_ui_enabled: flag(&p["zoomUIEnabled"]),
        is_darwin: false,
        container_id: "excalidraw-id".into(),
        footer_center: Some(slot("FooterCenter")),
        welcome_screen_help_hint: Some(slot("WelcomeScreenHelpHint")),
        on_event: None,
    }
}

fn render(case: &Value) -> Element {
    footer(props(case))
}

// -- DOM parity ---------------------------------------------------------------

#[test]
fn every_case_renders_upstreams_dom() {
    let cases = cases();
    assert!(cases.len() >= 25, "{} cases", cases.len());
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let expected: Vec<Value> = case["dom"].as_array().unwrap().iter().map(expand).collect();
        let actual = vec![tree(&Node::Element(render(case)))];
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
    for (key, value) in fixture()["locale"].as_object().unwrap() {
        assert_eq!(footer_text(key), value.as_str().unwrap(), "{key}");
    }
}

#[test]
fn the_zoom_label_is_to_fixed_0() {
    // toFixed takes the larger neighbour on a tie: 0.125 * 100 = 12.5
    assert_eq!(reset_zoom_label(0.125), "13%");
    assert_eq!(reset_zoom_label(1.235), "124%");
    assert_eq!(reset_zoom_label(0.996), "100%");
    assert_eq!(reset_zoom_label(1.0), "100%");
    assert_eq!(reset_zoom_label(30.0), "3000%");
}

// -- controls -----------------------------------------------------------------

fn control(name: &str) -> FooterControl {
    FooterControl::ALL
        .into_iter()
        .find(|c| c.name() == name)
        .unwrap_or_else(|| panic!("control {name}"))
}

#[test]
fn tooltips_are_upstreams() {
    let labels: Vec<(String, String)> = fixture()["tooltips"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["button"].as_str().unwrap().to_owned(),
                t["label"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(labels.len(), 6);
    for (button, label) in labels {
        let c = FooterControl::ALL
            .into_iter()
            .find(|c| c.aria_label() == Some(button.as_str()))
            .unwrap_or_else(|| panic!("no control labelled {button}"));
        assert_eq!(c.tooltip(false).as_deref(), Some(label.as_str()), "{button}");
    }
    // getShortcutKey: Cmd on a Mac
    assert_eq!(
        FooterControl::ZoomIn.tooltip(true).as_deref(),
        Some("Zoom in — Cmd++")
    );
    assert_eq!(
        FooterControl::ZoomOut.tooltip(true).as_deref(),
        Some("Zoom out — Cmd+-")
    );
    assert_eq!(FooterControl::ExitZenMode.tooltip(false), None);
}

#[test]
fn every_button_listens_for_clicks() {
    let root = Node::Element(render(&case("zen-mode-exit-button")));
    let mut buttons = Vec::new();
    fn walk<'a>(n: &'a Node, out: &mut Vec<&'a Element>) {
        if let Node::Element(el) = n {
            if el.tag() == "button" {
                out.push(el);
            }
            el.children().iter().for_each(|c| walk(c, out));
        }
    }
    walk(&root, &mut buttons);
    assert_eq!(buttons.len(), 7);
    for b in buttons {
        assert!(
            b.listened_events().any(|e| e == "click"),
            "{:?}",
            b.attributes()
        );
    }
}

#[test]
fn each_control_runs_upstreams_action() {
    let fixture = fixture();
    for click in fixture["clicks"].as_array().unwrap() {
        let c = control(click["control"].as_str().unwrap());
        let name = click["action"].as_str().unwrap();
        assert_eq!(c.action(), ActionName::from_name(name).unwrap(), "{click}");
    }
}

fn app_state(viewport: &Value, state: &Value) -> AppState {
    let mut map = viewport.as_object().unwrap().clone();
    for (k, v) in state.as_object().unwrap() {
        map.insert(
            k.clone(),
            if k == "zoom" {
                json!({ "value": v })
            } else {
                v.clone()
            },
        );
    }
    AppState::from_map(map)
}

#[test]
fn clicks_change_the_app_state_as_upstream() {
    let fixture = fixture();
    let viewport = &fixture["viewport"];
    for click in fixture["clicks"].as_array().unwrap() {
        let c = control(click["control"].as_str().unwrap());
        let mut state = app_state(viewport, &click["appState"]);
        let expected = click["result"].as_object().unwrap();
        let mut focused = false;
        match c {
            FooterControl::ZoomOut | FooterControl::ResetZoom | FooterControl::ZoomIn => {
                let v = perform_zoom_action(
                    c.zoom_action().unwrap(),
                    &ViewportState::from_app_state(&state),
                    &[],
                    &Map::new(),
                    &Offsets::default(),
                );
                assert_eq!(v.zoom, expected["zoom"].as_f64().unwrap(), "{click}");
                assert_eq!(v.scroll_x, expected["scrollX"].as_f64().unwrap(), "{click}");
                assert_eq!(v.scroll_y, expected["scrollY"].as_f64().unwrap(), "{click}");
                continue;
            }
            FooterControl::Undo | FooterControl::Redo => {
                assert!(expected.is_empty());
                assert!(c.zoom_action().is_none());
                continue;
            }
            FooterControl::Help => focused = toggle_shortcuts(&mut state),
            FooterControl::ExitZenMode => toggle_zen_mode(&mut state),
        }
        for (k, v) in expected {
            assert_eq!(state.get(k).unwrap_or(&Value::Null), v, "{k} of {click}");
        }
        assert_eq!(focused, click["focusContainer"].as_bool().unwrap(), "{click}");
    }
}

#[test]
fn the_stylesheet_is_the_footers_rules() {
    assert!(FOOTER_CSS.starts_with("/* Generated by tools/goldens/footer.mjs"));
    for rule in [
        ".excalidraw .App-menu_bottom {",
        ".excalidraw .help-icon {",
        ".zoom-actions,\n.undo-redo-buttons {",
        ".excalidraw .layer-ui__wrapper .disable-zen-mode--visible {",
        ".excalidraw.excalidraw--zen-mode .disable-zen-mode:not(:hover, :active) {",
        ".footer-center {",
        ".visually-hidden {",
    ] {
        assert!(FOOTER_CSS.contains(rule), "{rule}");
    }
    // only the footer's: none of the toolbar's or the menus'
    for other in [".App-toolbar", ".dropdown-menu", ".main-menu-trigger"] {
        assert!(!FOOTER_CSS.contains(other), "{other}");
    }
}
