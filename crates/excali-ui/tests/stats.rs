//! The stats panel (ex-529): upstream's `Stats`
//! (`components/Stats/index.tsx`) with its sections, rows and drag inputs
//! (`Collapsible`, `CanvasGrid`, `Position`, `Dimension`, `Angle`,
//! `FontSize`, `MultiPosition`, `MultiDimension`, `MultiAngle`,
//! `MultiFontSize`, `DragInput`), held to the DOM upstream's own code
//! renders and the app state its section headers write.
//!
//! Fixture: `tests/fixtures/stats.json`, written by
//! `tools/goldens/stats.mjs` from the pinned checkout (React 19.0.0 into
//! jsdom over a real `Scene`): per case the scene, the app state keys over
//! the defaults, the host's `gridModeEnabled` prop and the DOM; per click
//! case and control, whether `onClose` ran and the patch the section
//! header passed to `setAppState`. The stylesheet is the same generator's.

use std::collections::BTreeMap;

use excali_core::app_state::AppState;
use excali_core::constants::{STATS_PANEL_ELEMENT_PROPERTIES, STATS_PANEL_GENERAL_STATS};
use excali_core::element::Element;
use excali_scene::shape::Theme;
use excali_ui::dom::Node;
use excali_ui::stats::{
    canvas_grid_step, should_show_stats, stats_panel, stats_text, toggle_panel, typed_value,
    StatsEvent, StatsProps, STATS_CSS,
};
use serde_json::{json, Map, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/stats.json")).unwrap()
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
            if el.tag() == "input" {
                out.insert("value".into(), json!(el.attribute("value").unwrap_or("")));
            }
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

fn same(what: &str, expected: &Value, actual: &Value) {
    if expected != actual {
        panic!(
            "{what}:\nexpected {}\nactual   {}",
            serde_json::to_string(expected).unwrap(),
            serde_json::to_string(actual).unwrap()
        );
    }
}

// -- cases ----------------------------------------------------------------------

struct Case {
    elements: Vec<Element>,
    app_state: AppState,
    grid_mode_enabled: Option<bool>,
}

fn case_input(fx: &Value, case: &Value) -> Case {
    let ids: Vec<&str> = case["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let elements = fx["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| ids.contains(&e["id"].as_str().unwrap()))
        .map(|e| Element::from_map(e.as_object().unwrap().clone()).unwrap())
        .collect();
    let mut app_state = AppState::default();
    for (k, v) in case["appState"].as_object().unwrap() {
        app_state.insert(k.clone(), v.clone());
    }
    Case {
        elements,
        app_state,
        grid_mode_enabled: case["gridModeEnabled"].as_bool(),
    }
}

fn props(case: &Case) -> StatsProps<'_> {
    StatsProps {
        elements: &case.elements,
        app_state: &case.app_state,
        grid_mode_enabled: case.grid_mode_enabled,
    }
}

#[test]
fn every_case_renders_upstreams_dom() {
    let fx = fixture();
    let cases = fx["cases"].as_array().unwrap();
    assert!(cases.len() >= 35, "{} cases", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let input = case_input(&fx, case);
        let panel = stats_panel(&props(&input), None);
        let expected: Vec<Value> = case["dom"].as_array().unwrap().iter().map(expand).collect();
        same(
            name,
            &json!(expected),
            &json!([tree(&Node::Element(panel.element))]),
        );
    }
}

#[test]
fn every_section_header_and_the_close_button_do_what_upstreams_do() {
    let fx = fixture();
    let clicks = fx["clicks"].as_array().unwrap();
    assert_eq!(clicks.len(), 15);
    for click in clicks {
        let what = format!("{} {}", click["case"], click["control"]);
        let input = case_input(
            &fx,
            &json!({ "elements": fx["elements"].as_array().unwrap().iter().map(|e| e["id"].clone()).collect::<Vec<_>>(), "appState": click["appState"], "gridModeEnabled": null }),
        );
        let panel = stats_panel(&props(&input), None);
        let control = click["control"].as_str().unwrap();
        let event = panel
            .controls
            .iter()
            .find(|(c, _)| *c == control)
            .map(|(_, e)| e.clone())
            .unwrap_or_else(|| panic!("{what}: no control"));
        let (closed, patches) = match event {
            StatsEvent::Close => (1, json!([])),
            StatsEvent::TogglePanel(bit) => (0, json!([toggle_panel(&input.app_state, bit)])),
            other => panic!("{what}: {other:?}"),
        };
        same(&what, &click["closed"], &json!(closed));
        same(&what, &click["patches"], &patches);
    }
}

#[test]
fn the_controls_are_in_document_order() {
    let fx = fixture();
    let rect = fx["elements"][0].as_object().unwrap().clone();
    let elements = vec![Element::from_map(rect).unwrap()];
    let mut app_state = AppState::default();
    app_state.insert("selectedElementIds", json!({ "rect": true }));
    let panel = stats_panel(
        &StatsProps {
            elements: &elements,
            app_state: &app_state,
            grid_mode_enabled: None,
        },
        None,
    );
    let names: Vec<&str> = panel.controls.iter().map(|(c, _)| *c).collect();
    assert_eq!(names, ["close", "generalStats", "elementProperties"]);
    // nothing selected: no element properties section
    let panel = stats_panel(
        &StatsProps {
            elements: &elements,
            app_state: &AppState::default(),
            grid_mode_enabled: None,
        },
        None,
    );
    let names: Vec<&str> = panel.controls.iter().map(|(c, _)| *c).collect();
    assert_eq!(names, ["close", "generalStats"]);
}

#[test]
fn the_panel_bits_are_upstreams() {
    let fx = fixture();
    assert_eq!(
        fx["panels"],
        json!({ "generalStats": STATS_PANEL_GENERAL_STATS, "elementProperties": STATS_PANEL_ELEMENT_PROPERTIES })
    );
    // toggling flips one bit and leaves the other (`index.tsx:201-209, 250-261`)
    let mut state = AppState::default();
    assert_eq!(
        state.get("stats"),
        Some(&json!({ "open": false, "panels": 3 }))
    );
    for (panels, bit, want) in [
        (3, 1, 2),
        (3, 2, 1),
        (0, 1, 1),
        (0, 2, 2),
        (1, 1, 0),
        (2, 1, 3),
    ] {
        state.insert("stats", json!({ "open": false, "panels": panels }));
        assert_eq!(
            toggle_panel(&state, bit),
            json!({ "stats": { "open": true, "panels": want } })
        );
    }
}

#[test]
fn the_locale_is_upstreams() {
    let fx = fixture();
    for (key, text) in fx["locale"].as_object().unwrap() {
        assert_eq!(stats_text(key), text.as_str().unwrap(), "{key}");
    }
}

#[test]
fn the_stylesheet_is_upstreams_stats_and_drag_input() {
    assert!(STATS_CSS.starts_with("/* Generated by tools/goldens/stats.mjs"));
    for selector in [
        ".exc-stats {",
        ".exc-stats__row--heading {",
        ".excalidraw .drag-input-container {",
        ".excalidraw .drag-input {",
    ] {
        assert!(STATS_CSS.contains(selector), "{selector}");
    }
}

/// `LayerUI.tsx:305-310`: open, not in zen or view mode, and not under the
/// element link selector.
#[test]
fn the_panel_shows_as_layer_ui_decides() {
    let with = |patch: Value| {
        let mut s = AppState::default();
        for (k, v) in patch.as_object().unwrap() {
            s.insert(k.clone(), v.clone());
        }
        should_show_stats(&s)
    };
    assert!(!with(json!({})));
    assert!(with(json!({ "stats": { "open": true, "panels": 3 } })));
    assert!(with(json!({ "stats": { "open": true, "panels": 0 } })));
    assert!(!with(
        json!({ "stats": { "open": true, "panels": 3 }, "zenModeEnabled": true })
    ));
    assert!(!with(
        json!({ "stats": { "open": true, "panels": 3 }, "viewModeEnabled": true })
    ));
    assert!(!with(
        json!({ "stats": { "open": true, "panels": 3 }, "openDialog": { "name": "elementLinkSelector" } })
    ));
    assert!(with(
        json!({ "stats": { "open": true, "panels": 3 }, "openDialog": { "name": "help" } })
    ));
}

/// `DragInput.handleInputValue` (`DragInput.tsx:122-173`): a finite number,
/// rounded to two places, applied when the shown value is "Mixed" or it
/// moves by at least `SMALLEST_DELTA`.
#[test]
fn a_typed_value_applies_as_drag_input_decides() {
    assert_eq!(typed_value("12", Some(10.0)), Ok(Some(12.0)));
    assert_eq!(typed_value("12.345", Some(10.0)), Ok(Some(12.35)));
    assert_eq!(typed_value(" 7 ", Some(10.0)), Ok(Some(7.0)));
    assert_eq!(typed_value("1e2", Some(10.0)), Ok(Some(100.0)));
    assert_eq!(typed_value("0x10", Some(10.0)), Ok(Some(16.0)));
    assert_eq!(typed_value("-3.004", Some(10.0)), Ok(Some(-3.0)));
    // Number("") is 0
    assert_eq!(typed_value("", Some(10.0)), Ok(Some(0.0)));
    // within the smallest delta: nothing to apply
    assert_eq!(typed_value("10.004", Some(10.0)), Ok(None));
    assert_eq!(typed_value("10", Some(10.0)), Ok(None));
    // |10.01 - 10| is 0.00999..., under the delta in floating point too
    assert_eq!(typed_value("10.01", Some(10.0)), Ok(None));
    assert_eq!(typed_value("10.02", Some(10.0)), Ok(Some(10.02)));
    // "Mixed" always takes a number
    assert_eq!(typed_value("10", None), Ok(Some(10.0)));
    // not finite: the input goes back to the shown value
    for bad in ["abc", "Infinity", "-Infinity", "1e999", "NaN", "1,5"] {
        assert_eq!(typed_value(bad, Some(10.0)), Err(()), "{bad}");
    }
}

/// `CanvasGrid`'s callback for a typed value (`CanvasGrid.tsx:32-60`) with
/// `getNormalizedGridStep` (`scene/normalize.ts:15-17`).
#[test]
fn a_typed_grid_step_normalizes() {
    assert_eq!(canvas_grid_step(20.0), Some(20.0));
    assert_eq!(canvas_grid_step(12.5), Some(13.0));
    assert_eq!(canvas_grid_step(0.4), Some(1.0));
    assert_eq!(canvas_grid_step(-5.0), Some(1.0));
    assert_eq!(canvas_grid_step(250.0), Some(100.0));
    assert_eq!(canvas_grid_step(0.0), None);
}
