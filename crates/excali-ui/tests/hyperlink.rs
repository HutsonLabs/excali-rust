//! The hyperlink editor (ex-543) against upstream's own `Hyperlink`
//! rendered by React into jsdom and driven through its input and buttons.
//!
//! Fixture: `tests/fixtures/hyperlink.json`, written by
//! `tools/goldens/hyperlink.mjs` from the pinned checkout: per case the
//! element, the app state keys over the defaults, and per step the DOM
//! (`{tag, attrs, style, children}`, icons as `{icon: name}`), the input's
//! value, the element's link and `showHyperlinkPopup` after it. The
//! stylesheet is the same generator's.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use excali_core::app_state::AppState;
use excali_core::element::Element as SceneElement;
use excali_editor::hyperlink::{hyperlink_panel, submitted_link, HyperlinkMode};
use excali_editor::scene::Scene;
use excali_scene::shape::Theme;
use excali_ui::dom::{Element, EventData, Node};
use excali_ui::hyperlink::{hyperlink, HyperlinkEvent, HyperlinkProps, HYPERLINK_CSS};
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/hyperlink.json")).unwrap()
}

/// The origin the generator's jsdom page has (`isLocalLink`).
const ORIGIN: &str = "http://localhost";

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
            let children: Vec<Value> = el.children().iter().map(tree).collect();
            json!({ "tag": el.tag(), "attrs": attrs, "style": style, "children": children })
        }
    }
}

/// An `{icon: name}` of the fixture as the icon's own tree.
fn expand(v: &Value) -> Value {
    match v {
        Value::Object(o) if o.contains_key("icon") => {
            let name = o["icon"].as_str().unwrap();
            let icon = excali_ui::icons::icon(name).unwrap_or_else(|| panic!("no icon {name}"));
            tree(&Node::Element(icon.element(Theme::Light).unwrap()))
        }
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), expand(x))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(expand).collect()),
        _ => v.clone(),
    }
}

/// The case's app state with `showHyperlinkPopup` as after `step`.
fn app_state(case: &Value, step: &Value) -> AppState {
    let mut state = AppState::default();
    state.insert("offsetLeft", json!(0));
    state.insert("offsetTop", json!(0));
    for (k, v) in case["appState"].as_object().unwrap() {
        state.insert(k.clone(), v.clone());
    }
    let id = case["element"]["id"].as_str().unwrap();
    state.insert("selectedElementIds", json!({ id: true }));
    state.insert("showHyperlinkPopup", step["showHyperlinkPopup"].clone());
    state
}

/// The case's element with its link as after `step`.
fn scene(case: &Value, step: &Value) -> Scene {
    let mut map = case["element"].as_object().unwrap().clone();
    map.insert("link".into(), step["link"].clone());
    Scene::new(vec![SceneElement::from_map(map).unwrap()])
}

type Events = Rc<RefCell<Vec<HyperlinkEvent>>>;

fn render(case: &Value, step: &Value, input: &str) -> Option<(Element, Events)> {
    let panel = hyperlink_panel(&scene(case, step), &app_state(case, step))?;
    let events: Events = Rc::default();
    let sink = events.clone();
    let el = hyperlink(HyperlinkProps {
        panel: &panel,
        input,
        origin: ORIGIN,
        darwin: false,
        select: true,
        on_event: Some(Rc::new(move |e| sink.borrow_mut().push(e))),
    });
    Some((el, events))
}

fn find<'a>(el: &'a Element, tag: &str) -> Option<&'a Element> {
    if el.tag() == tag {
        return Some(el);
    }
    el.children().iter().find_map(|c| match c {
        Node::Element(e) => find(e, tag),
        Node::Text(_) => None,
    })
}

#[test]
fn popup_matches_upstream_at_every_step() {
    let f = fixture();
    let cases = f["cases"].as_array().unwrap();
    assert!(cases.len() >= 12);
    let mut checked = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let steps = case["steps"].as_array().unwrap();
        // the input keeps what was typed while the popup stays mounted
        let mut input = steps[0]["value"].as_str().unwrap_or("").to_owned();
        for (i, step) in steps.iter().enumerate() {
            if let Some(v) = step["value"].as_str() {
                input = v.to_owned();
            }
            let got = render(case, step, &input);
            let dom = step["dom"].as_array().unwrap();
            match (got, dom.first()) {
                (None, None) => {}
                (Some((el, _)), Some(want)) => {
                    assert_eq!(tree(&Node::Element(el)), expand(want), "{name} step {i}");
                    checked += 1;
                }
                (got, want) => panic!(
                    "{name} step {i}: port shows {}, upstream {}",
                    got.is_some(),
                    want.is_some()
                ),
            }
        }
    }
    assert!(checked >= 25, "{checked} trees");
}

#[test]
fn the_input_reports_typing_and_submits_on_enter_and_escape() {
    let f = fixture();
    let mut keys = 0;
    for case in f["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let steps = case["steps"].as_array().unwrap();
        for pair in steps.windows(2) {
            let (before, step) = (&pair[0], &pair[1]);
            let action = &step["step"];
            let Some(value) = before["value"].as_str() else {
                continue;
            };
            let (el, events) = render(case, before, value).unwrap();
            let input = find(&el, "input").unwrap_or_else(|| panic!("{name}: no input"));
            if let Some(text) = action["type"].as_str() {
                let data = EventData {
                    value: Some(text.to_owned()),
                    ..EventData::default()
                };
                input.dispatch("input", &data).expect("an input listener");
                assert_eq!(*events.borrow(), [HyperlinkEvent::Input(text.to_owned())]);
            } else if let Some(key) = action["key"].as_str() {
                let data = EventData {
                    key: key.to_owned(),
                    ctrl_key: action["ctrl"] == true,
                    value: Some(value.to_owned()),
                    ..EventData::default()
                };
                let response = input
                    .dispatch("keydown", &data)
                    .expect("a keydown listener");
                assert!(response.stop_propagation, "{name} {key}");
                assert_eq!(
                    response.prevent_default,
                    step["defaultPrevented"] == true,
                    "{name} {key}"
                );
                if key == "Enter" || key == "Escape" {
                    assert_eq!(
                        *events.borrow(),
                        [HyperlinkEvent::Submit(value.to_owned())],
                        "{name}"
                    );
                    // handleSubmit: normalizeLink(value) || null
                    assert_eq!(
                        json!(submitted_link(value)),
                        step["link"],
                        "{name}: {value:?}"
                    );
                    assert_eq!(step["showHyperlinkPopup"], "info");
                } else {
                    assert!(events.borrow().is_empty(), "{name} {key}");
                }
                keys += 1;
            }
        }
    }
    assert!(keys >= 9, "{keys} keys");
}

#[test]
fn the_panel_mode_follows_show_hyperlink_popup() {
    let f = fixture();
    let case = &f["cases"][0];
    assert_eq!(case["name"], "create");
    let steps = case["steps"].as_array().unwrap();
    let editing = hyperlink_panel(&scene(case, &steps[0]), &app_state(case, &steps[0])).unwrap();
    assert_eq!(editing.mode, HyperlinkMode::Editor);
    assert_eq!(editing.element_id, "r");
    let info = hyperlink_panel(&scene(case, &steps[2]), &app_state(case, &steps[2])).unwrap();
    assert_eq!(info.mode, HyperlinkMode::Info);
    assert_eq!(info.link.as_deref(), Some("excalidraw.com"));
    // two elements selected: no popup (App.tsx:2549-2551)
    let mut state = app_state(case, &steps[0]);
    state.insert("selectedElementIds", json!({ "r": true, "other": true }));
    let mut two = case["element"].as_object().unwrap().clone();
    two.insert("id".into(), json!("other"));
    let scene = Scene::new(vec![
        SceneElement::from_map(case["element"].as_object().unwrap().clone()).unwrap(),
        SceneElement::from_map(two).unwrap(),
    ]);
    assert!(hyperlink_panel(&scene, &state).is_none());
}

#[test]
fn stylesheet_is_upstreams() {
    assert!(HYPERLINK_CSS.starts_with("/* Generated by tools/goldens/hyperlink.mjs"));
    assert!(HYPERLINK_CSS.contains(".excalidraw-hyperlinkContainer {"));
}
