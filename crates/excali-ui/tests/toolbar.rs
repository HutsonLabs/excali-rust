//! The shapes toolbar (ex-518): upstream's desktop `Toolbar` and its
//! `ExtraToolsDropdown` (`components/Toolbar.tsx`), the tool buttons of
//! `components/Tools.tsx`, held to the DOM upstream leaves.
//!
//! Fixture: `tests/fixtures/toolbar.json`, upstream's Toolbar at the pinned
//! commit rendered by React 19.0.0 with radix-ui 1.4.3 into jsdom
//! (`tools/goldens/toolbar.mjs`): per case the app state, props and whether
//! the dropdown was opened, and the element tree (attributes and inline
//! style as maps, icons as `{icon: name}`, radix's ids as `radix-N`, the
//! host's slots as `<template data-slot>`). `src/toolbar/toolbar.css` is the
//! same generator's compilation of Toolbar.scss and DropdownMenu.scss.

use std::collections::{BTreeMap, HashMap};

use excali_editor::tools::{
    ActiveTool, PointerType, PreferredSelectionTool, SelectionTool, Tool, ToolRefusal, ToolState,
    ToolType,
};
use excali_scene::shape::Theme;
use excali_ui::dom::{Element, Node};
use excali_ui::primitives::Rect;
use excali_ui::toolbar::{
    activate_extra_tool, activate_tool_button, dropdown_position, toolbar, toolbar_text,
    DropdownSide, ToolbarProps, TOOLBAR_CSS,
};
use serde_json::{json, Map, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/toolbar.json")).unwrap()
}

fn cases() -> Vec<Value> {
    fixture()["cases"].as_array().unwrap().clone()
}

// -- the tree the fixture records ---------------------------------------------

/// Renames ids by first appearance (an element's `id` before its other
/// attributes), as the generator renames radix's.
struct Ids(HashMap<String, String>);

impl Ids {
    fn rename(&mut self, id: &str) -> String {
        let n = self.0.len() + 1;
        self.0
            .entry(id.to_owned())
            .or_insert_with(|| format!("radix-{n}"))
            .clone()
    }
}

fn is_id_ref(name: &str) -> bool {
    name == "id" || name == "aria-controls" || name == "aria-labelledby"
}

fn tree(node: &Node, ids: &mut Ids) -> Value {
    match node {
        Node::Text(text) => Value::String(text.clone()),
        Node::Element(el) => {
            // radix's ids; an icon's own (clip path) ids stay
            let rename = !el.is_svg();
            if let Some(id) = el.attribute("id").filter(|_| rename) {
                ids.rename(id);
            }
            let attrs: BTreeMap<_, _> = el
                .attributes()
                .iter()
                .map(|(k, v)| {
                    let v = if rename && is_id_ref(k) {
                        ids.rename(v)
                    } else {
                        v.clone()
                    };
                    (k.clone(), Value::String(v))
                })
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
            let children = el.children().iter().map(|c| tree(c, ids)).collect();
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
            let el = icon.element(Theme::Light).unwrap();
            tree(&Node::Element(el), &mut Ids(HashMap::new()))
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

fn tool(name: &str) -> ToolType {
    ToolType::from_name(name).unwrap_or_else(|| panic!("tool {name}"))
}

fn slot(name: &str) -> Node {
    Node::Element(Element::new("template").attr("data-slot", name))
}

fn tool_state(case: &Value) -> ToolState {
    let app = &case["appState"];
    let props = &case["props"];
    let mut tools = ToolState::default();
    if let Some(active) = app.get("activeTool") {
        tools.active_tool = ActiveTool {
            tool: Tool::Builtin(tool(active["type"].as_str().unwrap())),
            locked: active["locked"].as_bool().unwrap_or(false),
            ..ActiveTool::default()
        };
    }
    if let Some(p) = app.get("preferredSelectionTool") {
        tools.preferred_selection_tool = PreferredSelectionTool {
            tool: match p["type"].as_str().unwrap() {
                "lasso" => SelectionTool::Lasso,
                _ => SelectionTool::Selection,
            },
            initialized: p["initialized"].as_bool().unwrap(),
        };
    }
    tools.pen_mode = app["penMode"].as_bool().unwrap_or(false);
    tools.pen_detected = app["penDetected"].as_bool().unwrap_or(false);
    if let Some(forced) = props.get("activeTool") {
        tools.options.forced_tool = Some(Tool::Builtin(tool(forced["type"].as_str().unwrap())));
    }
    tools.options.image_tool = props["UIOptions"]["tools"]["image"]
        .as_bool()
        .unwrap_or(true);
    tools
}

fn render(case: &Value) -> Element {
    let tools = tool_state(case);
    let props = &case["props"];
    toolbar(ToolbarProps {
        tools: &tools,
        zen_mode: case["appState"]["zenModeEnabled"]
            .as_bool()
            .unwrap_or(false),
        collaborating: props["isCollaborating"].as_bool().unwrap_or(false),
        ai_enabled: props["aiEnabled"].as_bool() != Some(false),
        diagram_to_code: case["plugins"]["diagramToCode"].as_bool().unwrap_or(false),
        extra_tools_open: case["open"].as_bool().unwrap(),
        id_prefix: "excalidraw-id".into(),
        hint_viewer: Some(slot("HintViewer")),
        ttd_trigger: Some(slot("TTDDialogTrigger")),
        on_event: None,
    })
}

// -- DOM parity ---------------------------------------------------------------

#[test]
fn every_case_renders_upstreams_dom() {
    let cases = cases();
    assert!(cases.len() >= 40, "{} cases", cases.len());
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let expected: Vec<Value> = case["dom"].as_array().unwrap().iter().map(expand).collect();
        let actual = vec![tree(&Node::Element(render(case)), &mut Ids(HashMap::new()))];
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
    let fixture = fixture();
    for (key, value) in fixture["locale"].as_object().unwrap() {
        assert_eq!(toolbar_text(key), value.as_str().unwrap(), "{key}");
    }
}

#[test]
fn tool_buttons_and_menu_items_listen() {
    let open = cases().into_iter().find(|c| c["name"] == "open").unwrap();
    let root = Node::Element(render(&open));
    let mut found = BTreeMap::new();
    fn walk(n: &Node, found: &mut BTreeMap<String, Vec<String>>) {
        if let Node::Element(el) = n {
            if let Some(id) = el.attribute("data-testid") {
                let events: Vec<String> = el.listened_events().map(str::to_owned).collect();
                found.entry(id.to_owned()).or_insert(events);
            }
            el.children().iter().for_each(|c| walk(c, found));
        }
    }
    walk(&root, &mut found);
    for id in ["toolbar-hand", "toolbar-rectangle", "toolbar-lock"] {
        assert!(
            found[id].iter().any(|e| e == "click"),
            "{id}: {:?}",
            found[id]
        );
    }
    assert!(found["toolbar-rectangle"]
        .iter()
        .any(|e| e == "pointerdown"));
    assert!(found["dropdown-menu-button"].iter().any(|e| e == "click"));
    assert!(found["dropdown-menu-button"]
        .iter()
        .any(|e| e == "pointerdown"));
    assert!(found["dropdown-menu"].iter().any(|e| e == "keydown"));
    for id in ["toolbar-image", "toolbar-frame", "toolbar-laser"] {
        assert!(
            found[id].iter().any(|e| e == "click"),
            "{id}: {:?}",
            found[id]
        );
    }
}

#[test]
fn the_stylesheet_is_upstreams_toolbar_and_dropdown_scss() {
    assert!(TOOLBAR_CSS.starts_with("/* Generated by tools/goldens/toolbar.mjs"));
    for rule in [
        ".excalidraw .App-toolbar__divider {",
        ".excalidraw .App-toolbar__extra-tools-dropdown {",
        "min-width: 11.875rem;",
        ".excalidraw .dropdown-menu {",
    ] {
        assert!(TOOLBAR_CSS.contains(rule), "{rule}");
    }
}

// -- activation (Tools.tsx:262-352, Toolbar.tsx:124-213) ----------------------

fn active(tools: &ToolState) -> ToolType {
    tools.active_tool.tool.builtin().unwrap()
}

#[test]
fn a_tool_button_activates_its_tool_once() {
    let mut tools = ToolState::default();
    let switched = activate_tool_button(&mut tools, ToolType::Rectangle, Some(PointerType::Mouse));
    assert!(matches!(switched, Some(Ok(_))));
    assert_eq!(active(&tools), ToolType::Rectangle);
    // already active: nothing
    assert!(activate_tool_button(&mut tools, ToolType::Rectangle, None).is_none());
    assert_eq!(active(&tools), ToolType::Rectangle);
}

#[test]
fn pointer_clicking_the_active_selection_tool_switches_to_lasso() {
    let mut tools = ToolState::default();
    assert_eq!(active(&tools), ToolType::Selection);
    // keyboard activation stays on selection
    assert!(activate_tool_button(&mut tools, ToolType::Selection, None).is_none());
    assert_eq!(active(&tools), ToolType::Selection);
    let switched = activate_tool_button(&mut tools, ToolType::Selection, Some(PointerType::Mouse));
    assert!(matches!(switched, Some(Ok(_))));
    assert_eq!(active(&tools), ToolType::Lasso);
    // from lasso the selection button returns to selection
    activate_tool_button(&mut tools, ToolType::Selection, Some(PointerType::Mouse));
    assert_eq!(active(&tools), ToolType::Selection);
}

#[test]
fn the_first_pen_press_on_a_tool_button_turns_pen_mode_on() {
    let mut tools = ToolState::default();
    activate_tool_button(&mut tools, ToolType::Ellipse, Some(PointerType::Pen));
    assert!(tools.pen_detected && tools.pen_mode);
    assert_eq!(active(&tools), ToolType::Ellipse);
    // once detected, a pen press leaves pen mode as it is
    tools.pen_mode = false;
    activate_tool_button(&mut tools, ToolType::Diamond, Some(PointerType::Pen));
    assert!(!tools.pen_mode);
}

#[test]
fn a_forced_tool_refuses_other_buttons_and_items() {
    let mut tools = ToolState::default();
    tools.force_tool(Some(Tool::Builtin(ToolType::Frame)));
    assert_eq!(active(&tools), ToolType::Frame);
    assert_eq!(
        activate_tool_button(&mut tools, ToolType::Rectangle, None),
        Some(Err(ToolRefusal::Forced))
    );
    assert_eq!(
        activate_extra_tool(&mut tools, ToolType::Laser),
        Err(ToolRefusal::Forced)
    );
    assert_eq!(active(&tools), ToolType::Frame);
}

#[test]
fn a_menu_item_activates_its_tool() {
    let mut tools = ToolState::default();
    assert!(activate_extra_tool(&mut tools, ToolType::Bucketfill).is_ok());
    assert_eq!(active(&tools), ToolType::Bucketfill);
    tools.options.image_tool = false;
    assert_eq!(
        activate_extra_tool(&mut tools, ToolType::Image),
        Err(ToolRefusal::Unsupported)
    );
}

// -- dropdown placement (radix Popper: bottom, align end, sideOffset 8) -------

fn rect(left: f64, top: f64, width: f64, height: f64) -> Rect {
    Rect {
        left,
        top,
        width,
        height,
    }
}

#[test]
fn the_dropdown_opens_below_its_trigger_aligned_to_its_end() {
    let trigger = rect(600.0, 16.0, 36.0, 36.0);
    let placed = dropdown_position(trigger, (190.0, 300.0), (1440.0, 900.0));
    assert_eq!(placed.side, DropdownSide::Bottom);
    assert_eq!((placed.x, placed.y), (636.0 - 190.0, 52.0 + 8.0));
}

#[test]
fn the_dropdown_shifts_into_the_viewport_and_flips_when_below_does_not_fit() {
    // the end-aligned menu would start left of the viewport: shifted to 0
    let placed = dropdown_position(
        rect(20.0, 16.0, 36.0, 36.0),
        (190.0, 300.0),
        (1440.0, 900.0),
    );
    assert_eq!((placed.x, placed.side), (0.0, DropdownSide::Bottom));
    // no room below, room above: flipped to the top
    let placed = dropdown_position(
        rect(600.0, 700.0, 36.0, 36.0),
        (190.0, 300.0),
        (1440.0, 900.0),
    );
    assert_eq!(placed.side, DropdownSide::Top);
    assert_eq!(placed.y, 700.0 - 8.0 - 300.0);
    // room on neither side: stays below
    let placed = dropdown_position(
        rect(600.0, 300.0, 36.0, 36.0),
        (190.0, 800.0),
        (1440.0, 900.0),
    );
    assert_eq!(placed.side, DropdownSide::Bottom);
}
