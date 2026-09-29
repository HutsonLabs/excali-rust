//! The full styles panel (ex-519): `getShapeActionPredicates`
//! (`components/shapeActionPredicates.ts`), `showSelectedShapeActions`
//! (`element/src/showSelectedShapeActions.ts`), `SelectedShapeActions`
//! (`components/Actions.tsx:63-217`) and LayerUI's section and island
//! around it (`components/LayerUI.tsx:249-297`), against upstream.
//!
//! Fixture: `tests/fixtures/styles-panel.json`, upstream's own functions
//! and components at the pinned commit (`tools/goldens/styles-panel.mjs`):
//! per case an active tool, a selection and app state over a scene of
//! upstream-built elements, the document direction, the predicates,
//! whether the panel shows, and the tree the panel renders, with
//! `{ action }` where it calls `renderAction`.

use std::collections::HashMap;
use std::sync::OnceLock;

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_editor::actions::{
    full_styles_panel, get_shape_action_predicates, get_target_elements, render_styles_panel,
    show_selected_shape_actions, ActionContext, ActionEnv, ActionManager, ActionName, AppProps,
    PanelGate, ShapeActionPredicates,
};
use excali_ui::styles_panel::{
    legend_text, selected_shape_actions, shape_actions_section, PanelNode,
};
use serde_json::{json, Map, Value};

fn fixture() -> &'static Value {
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let text = include_str!("fixtures/styles-panel.json");
        serde_json::from_str(text).expect("styles-panel.json parses")
    })
}

fn cases() -> &'static [Value] {
    fixture()["cases"].as_array().expect("cases")
}

fn scene(name: &str) -> Vec<Element> {
    fixture()["scenes"][name]
        .as_array()
        .expect("a scene")
        .iter()
        .map(|e| Element::from_map(e.as_object().expect("an element").clone()).expect("element"))
        .collect()
}

/// The case's app state: the defaults with the case's keys, `{ ref }`
/// replaced by the scene's element (`editingTextElement`, `newElement`).
fn app_state(case: &Value) -> AppState {
    let raw = &fixture()["scenes"][case["scene"].as_str().unwrap()];
    let by_id: HashMap<&str, &Value> = raw
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["id"].as_str().unwrap(), e))
        .collect();
    let mut state = AppState::default();
    state.insert("width", json!(1440));
    state.insert("height", json!(900));
    for (k, v) in case["appState"].as_object().unwrap() {
        let v = match v.get("ref").and_then(Value::as_str) {
            Some(id) => (*by_id.get(id).expect("ref")).clone(),
            None => v.clone(),
        };
        state.insert(k.clone(), v);
    }
    state
}

struct Case {
    id: String,
    elements: Vec<Element>,
    app_state: AppState,
    props: AppProps,
    env: ActionEnv,
    rtl: bool,
}

impl Case {
    fn new(case: &Value) -> Case {
        Case {
            id: case["id"].as_str().unwrap().to_string(),
            elements: scene(case["scene"].as_str().unwrap()),
            app_state: app_state(case),
            props: AppProps::default(),
            env: ActionEnv::default(),
            rtl: case["rtl"].as_bool().unwrap(),
        }
    }

    fn ctx(&self) -> ActionContext<'_> {
        ActionContext {
            elements: &self.elements,
            app_state: &self.app_state,
            props: &self.props,
            env: &self.env,
        }
    }
}

/// The predicates under upstream's names.
fn predicates_json(p: &ShapeActionPredicates) -> Value {
    json!({
        "hasSelection": p.has_selection,
        "showExtraActions": p.show_extra_actions,
        "strokeColor": p.stroke_color,
        "backgroundColor": p.background_color,
        "fill": p.fill,
        "strokeWidth": p.stroke_width,
        "freedrawMode": p.freedraw_mode,
        "strokeStyle": p.stroke_style,
        "sloppiness": p.sloppiness,
        "roundness": p.roundness,
        "arrowType": p.arrow_type,
        "arrowheads": p.arrowheads,
        "text": p.text,
        "textAlign": p.text_align,
        "verticalAlign": p.vertical_align,
        "opacity": p.opacity,
        "layers": p.layers,
        "align": p.align,
        "distribute": p.distribute,
        "link": p.link,
        "linkSingleOnly": p.link_single_only,
        "cropEditor": p.crop_editor,
        "lineEditor": p.line_editor,
    })
}

/// React's inline style name: camelCase to kebab-case, custom properties
/// as they are.
fn css_name(key: &str) -> String {
    if key.starts_with("--") {
        return key.to_string();
    }
    let mut out = String::new();
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// A node in the fixture's form: `{ tag, class?, attrs?, style?,
/// children? }`, text, or `{ action }`; the style as CSS names and
/// strings (what React sets), the legends in English.
fn node_json(node: &PanelNode) -> Value {
    match node {
        PanelNode::Element(e) => {
            let mut out = Map::new();
            out.insert("tag".into(), json!(e.tag));
            if let Some(class) = &e.class {
                out.insert("class".into(), json!(class));
            }
            if !e.attrs.is_empty() {
                let attrs: Map<String, Value> =
                    e.attrs.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
                out.insert("attrs".into(), Value::Object(attrs));
            }
            if !e.style.is_empty() {
                let style: Map<String, Value> =
                    e.style.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
                out.insert("style".into(), Value::Object(style));
            }
            if !e.children.is_empty() {
                out.insert(
                    "children".into(),
                    Value::Array(e.children.iter().map(node_json).collect()),
                );
            }
            Value::Object(out)
        }
        PanelNode::Text(key) => json!(legend_text(key)),
        PanelNode::Action(name) => json!({ "action": name.as_str() }),
        PanelNode::Panel => json!({ "action": "<panel>" }),
    }
}

/// The fixture's tree with the style as CSS names and strings.
fn expected_tree(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                let v = match k.as_str() {
                    "style" => Value::Object(
                        v.as_object()
                            .unwrap()
                            .iter()
                            .map(|(k, v)| {
                                let v = match v {
                                    Value::String(s) => s.clone(),
                                    Value::Number(n) => n.to_string(),
                                    other => panic!("style value {other}"),
                                };
                                (css_name(k), json!(v))
                            })
                            .collect(),
                    ),
                    "children" => Value::Array(v.as_array().unwrap().iter().map(expected_tree).collect()),
                    _ => v.clone(),
                };
                out.insert(k.clone(), v);
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn actions_of(node: &PanelNode, out: &mut Vec<ActionName>) {
    match node {
        PanelNode::Element(e) => e.children.iter().for_each(|c| actions_of(c, out)),
        PanelNode::Action(name) => out.push(*name),
        PanelNode::Text(_) | PanelNode::Panel => {}
    }
}

#[test]
fn fixture_covers_every_predicate_both_ways() {
    let cases = cases();
    assert!(cases.len() >= 280, "{} cases", cases.len());
    let keys: Vec<&String> = cases[0]["predicates"].as_object().unwrap().keys().collect();
    assert_eq!(keys.len(), 23);
    for key in keys {
        let on = cases.iter().filter(|c| c["predicates"][key] == json!(true)).count();
        assert!(on > 0 && on < cases.len(), "{key}: {on} of {}", cases.len());
    }
    let shown = cases.iter().filter(|c| c["show"] == json!(true)).count();
    assert!(shown > 0 && shown < cases.len());
    assert!(cases.iter().any(|c| c["rtl"] == json!(true)));
}

#[test]
fn predicates_match_upstream() {
    let mut failures = Vec::new();
    for case in cases() {
        let c = Case::new(case);
        let got = predicates_json(&get_shape_action_predicates(&c.ctx()));
        if got != case["predicates"] {
            let diff: Vec<String> = got
                .as_object()
                .unwrap()
                .iter()
                .filter(|(k, v)| case["predicates"][k.as_str()] != **v)
                .map(|(k, v)| format!("{k}: got {v}"))
                .collect();
            failures.push(format!("{}: {}", c.id, diff.join(", ")));
        }
    }
    assert!(failures.is_empty(), "{} cases differ:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn show_selected_shape_actions_matches_upstream() {
    let mut failures = Vec::new();
    for case in cases() {
        let c = Case::new(case);
        let got = show_selected_shape_actions(&c.ctx());
        if json!(got) != case["show"] {
            failures.push(format!("{}: got {got}", c.id));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn panel_tree_matches_upstream() {
    let mut failures = Vec::new();
    for case in cases() {
        let c = Case::new(case);
        let got = node_json(&selected_shape_actions(&c.ctx(), c.rtl));
        let expected = expected_tree(&case["tree"][0]);
        if got != expected {
            failures.push(format!("{}:\n  got      {got}\n  expected {expected}", c.id));
        }
    }
    assert!(failures.is_empty(), "{} cases differ:\n{}", failures.len(), failures.join("\n"));
}

/// The tree renders the controls of ex-514's `full_styles_panel` whose
/// gates hold, in its order.
#[test]
fn panel_tree_agrees_with_the_registry_layout() {
    let manager = ActionManager::new();
    for case in cases() {
        let c = Case::new(case);
        let ctx = c.ctx();
        let predicates = get_shape_action_predicates(&ctx);
        let bucket_fill = c.app_state.get("activeTool").and_then(|t| t.get("type"))
            == Some(&json!("bucketfill"));
        let layout = full_styles_panel(bucket_fill, c.rtl);
        let expected = render_styles_panel(&layout, |g: PanelGate| predicates.gate(g), &manager, &ctx);
        let mut got = Vec::new();
        actions_of(&selected_shape_actions(&ctx, c.rtl), &mut got);
        assert_eq!(got, expected, "{}", c.id);
    }
}

#[test]
fn target_elements() {
    let by_id = |case: &str| {
        let case = cases().iter().find(|c| c["id"] == json!(case)).unwrap();
        let c = Case::new(case);
        get_target_elements(&c.ctx())
            .iter()
            .map(|e| e.base.id.clone())
            .collect::<Vec<_>>()
    };
    // bound text comes with its container, deleted elements never
    assert_eq!(by_id("select-c1"), ["c1", "ct1"]);
    assert_eq!(by_id("select-x1"), Vec::<String>::new());
    // the element being edited, or being drawn except by the autoshape tool
    assert_eq!(by_id("editing-ct1"), ["ct1"]);
    assert_eq!(by_id("new-r1-selected"), ["r1"]);
    assert_eq!(by_id("new-e1-autoshape-selected"), ["r1"]);
}

#[test]
fn section_and_island_match_upstream() {
    let fixture = fixture();
    let container_id = fixture["containerId"].as_str().unwrap();
    for case in fixture["wrapper"].as_array().unwrap() {
        let height = case["height"].as_f64().unwrap();
        let zen = case["zenModeEnabled"].as_bool().unwrap();
        let got = node_json(&shape_actions_section(height, zen, container_id, PanelNode::Panel));
        assert_eq!(got, expected_tree(&case["tree"][0]), "height {height}, zen {zen}");
    }
}

#[test]
fn legends_are_upstream_english() {
    for (key, text) in fixture()["locale"].as_object().unwrap() {
        assert_eq!(legend_text(key), text.as_str().unwrap(), "{key}");
    }
}
