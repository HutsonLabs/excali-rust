//! The phone layout (ex-702): upstream's `MobileToolbar`
//! (`components/MobileToolbar.tsx`, with `ToolPopover.tsx` and the tool
//! buttons and popovers of `Tools.tsx`) and `MobileMenu`
//! (`components/MobileMenu.tsx`), held to the DOM upstream leaves, and the
//! phone rule and mockup 06 (`site/static/mockups/06-phone.html`).
//!
//! Fixture: `tests/fixtures/mobile-menu.json`, upstream's components at the
//! pinned commit rendered by React 19.0.0 with radix-ui 1.4.3 into jsdom
//! (`tools/goldens/mobile-menu.mjs`): per toolbar case the app state and
//! props, the width the toolbar measures, the trigger clicked after the
//! first render, and the element tree (in the form of
//! `tests/fixtures/toolbar.json`); per menu case MobileMenu's tree with
//! the host's slots as `<template data-slot>`. `src/mobile_menu/
//! mobile_menu.css` is the same generator's compilation of the SCSS.

use std::collections::HashMap;

use excali_editor::tools::{
    ActiveTool, PreferredSelectionTool, SelectionTool, Tool, ToolState, ToolType,
};
use excali_scene::shape::Theme;
use excali_ui::dom::{Element, Node};
use excali_ui::editor_interface::{get_form_factor, FormFactor};
use excali_ui::mobile_menu::{
    apply_toolbar_event, init_selection_tool, mobile_menu, mobile_text, mobile_toolbar,
    tool_popover_position, toolbar_width, tools_outside, MobileMenuProps, MobileToolbarProps,
    MobileToolbarState, ToolPopover, MIN_WIDTH, MOBILE_MENU_CSS, SCROLLBAR_MARGIN, SCROLLBAR_WIDTH,
    TOOL_GAP, TOOL_SIZE,
};
use excali_ui::primitives::Rect;
use excali_ui::toolbar::{DropdownSide, ToolbarEvent};
use serde_json::{json, Map, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/mobile-menu.json")).unwrap()
}

// -- the tree the fixture records (as tests/toolbar.rs) -----------------------

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
            let rename = !el.is_svg();
            if let Some(id) = el.attribute("id").filter(|_| rename) {
                ids.rename(id);
            }
            let attrs: Map<String, Value> = el
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
            let style: Map<String, Value> = el
                .style_properties()
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            let children = el.children().iter().map(|c| tree(c, ids)).collect();
            json!({
                "tag": el.tag(),
                "attrs": attrs,
                "style": style,
                "children": merge_text(children),
            })
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
/// markup of that icon.
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

fn trees(nodes: &[Node]) -> Vec<Value> {
    let mut ids = Ids(HashMap::new());
    nodes.iter().map(|n| tree(n, &mut ids)).collect()
}

fn check(name: &str, expected: &Value, actual: Vec<Value>) {
    let expected: Vec<Value> = expected.as_array().unwrap().iter().map(expand).collect();
    if actual != expected {
        panic!(
            "case {name}:\nexpected {}\nactual   {}",
            serde_json::to_string(&expected).unwrap(),
            serde_json::to_string(&actual).unwrap()
        );
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
    tools
}

/// The toolbar after its first render (its effects synced to the tool
/// state) and the case's click, as upstream renders it.
fn render_toolbar(case: &Value) -> (Element, ToolState) {
    let mut tools = tool_state(case);
    let mut state = MobileToolbarState::new(&tools);
    state.width = case["width"].as_f64().unwrap();
    if let Some(click) = case["click"].as_str() {
        let event = match click {
            "extra-tools" => ToolbarEvent::ExtraToolsToggle,
            "toolbar-selection" => ToolbarEvent::ToolPopoverTrigger(ToolPopover::Selection),
            "toolbar-freedraw" => ToolbarEvent::ToolPopoverTrigger(ToolPopover::Freedraw),
            "toolbar-rectangle" => ToolbarEvent::ToolPopoverTrigger(ToolPopover::Shapes),
            "toolbar-arrow" => ToolbarEvent::ToolPopoverTrigger(ToolPopover::Linear),
            other => panic!("click {other}"),
        };
        apply_toolbar_event(&mut state, &mut tools, &event);
    }
    let props = &case["props"];
    let el = mobile_toolbar(MobileToolbarProps {
        tools: &tools,
        state: &state,
        ai_enabled: props["aiEnabled"].as_bool() != Some(false),
        diagram_to_code: case["plugins"]["diagramToCode"].as_bool().unwrap_or(false),
        id_prefix: "excalidraw-id".into(),
        ttd_trigger: Some(slot("TTDDialogTrigger")),
        on_event: None,
    });
    (el, tools)
}

fn menu_props(case: &Value) -> MobileMenuProps {
    let app = &case["appState"];
    let props = &case["props"];
    let flag = |v: &Value| v.as_bool().unwrap_or(false);
    MobileMenuProps {
        view_mode: flag(&app["viewModeEnabled"]),
        pen_mode: flag(&app["penMode"]),
        pen_detected: flag(&app["penDetected"]),
        scrolled_outside: flag(&app["scrolledOutside"]),
        menu_open: !app["openMenu"].is_null(),
        sidebar_open: !app["openSidebar"].is_null(),
        element_link_selector: app["openDialog"]["name"] == "elementLinkSelector",
        interaction_enabled: props["interaction"].as_bool() != Some(false),
        default_ui: props["defaultUIEnabled"].as_bool() != Some(false),
        scroll_back_ui: props["scrollBackToContentUIEnabled"].as_bool() != Some(false),
        render_welcome_screen: flag(&props["renderWelcomeScreen"]),
        sidebars: Some(slot("Sidebars")),
        welcome_screen: Some(slot("WelcomeScreenCenter")),
        main_menu: Some(slot("MainMenu")),
        sidebar_trigger: Some(slot("DefaultSidebarTrigger")),
        top_left_ui: flag(&props["topLeftUI"]).then(|| slot("TopLeftUI")),
        top_right_ui: flag(&props["topRightUI"]).then(|| slot("TopRightUI")),
        shape_actions: Some(slot("MobileShapeActions")),
        toolbar: Some(slot("MobileToolbar")),
        on_event: None,
    }
}

// -- DOM parity ---------------------------------------------------------------

#[test]
fn every_toolbar_case_renders_upstreams_dom() {
    let fixture = fixture();
    let cases = fixture["toolbar"].as_array().unwrap();
    assert!(cases.len() >= 60, "{} cases", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let (el, _) = render_toolbar(case);
        check(name, &case["dom"], trees(&[Node::Element(el)]));
    }
}

#[test]
fn every_menu_case_renders_upstreams_dom() {
    let fixture = fixture();
    let cases = fixture["menu"].as_array().unwrap();
    assert!(cases.len() >= 15, "{} cases", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        check(name, &case["dom"], trees(&mobile_menu(menu_props(case))));
    }
}

#[test]
fn the_locale_strings_are_upstreams() {
    let fixture = fixture();
    for (key, value) in fixture["locale"].as_object().unwrap() {
        assert_eq!(mobile_text(key), value.as_str().unwrap(), "{key}");
    }
}

// -- order, widths and the mockup ----------------------------------------------

/// The toolbar's entries: tool test ids without `toolbar-`, and
/// `extra-tools` for the "…" trigger.
fn entries(el: &Element) -> Vec<String> {
    el.children()
        .iter()
        .filter_map(|n| match n {
            Node::Element(e) => {
                if let Some(id) = e.attribute("data-testid") {
                    return Some(id.trim_start_matches("toolbar-").to_owned());
                }
                let trigger = e.children().iter().any(|c| {
                    matches!(c, Node::Element(t) if t.attribute("class").is_some_and(|c| c.contains("App-toolbar__extra-tools-trigger")))
                });
                trigger.then(|| "extra-tools".to_owned())
            }
            Node::Text(_) => None,
        })
        .collect()
}

/// Research §3.7: hand, selection/lasso, freedraw/autoshape, eraser, the
/// shapes, arrow/line, then text, image and frame as the width allows,
/// then "…".
#[test]
fn toolbar_order_matches_the_research_page() {
    let fixture = fixture();
    let order: Vec<&str> = fixture["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        [
            "hand",
            "selection",
            "freedraw",
            "eraser",
            "rectangle",
            "arrow",
            "text",
            "extra-tools"
        ]
    );
    let base = [
        "hand",
        "selection",
        "freedraw",
        "eraser",
        "rectangle",
        "arrow",
    ];
    for (width, extra) in [
        (0.0, vec![]),
        (MIN_WIDTH, vec![]),
        (MIN_WIDTH + 40.0, vec!["text"]),
        (MIN_WIDTH + 80.0, vec!["text", "image"]),
        (MIN_WIDTH + 120.0, vec!["text", "image", "frame"]),
    ] {
        let tools = ToolState::default();
        let mut state = MobileToolbarState::new(&tools);
        state.width = width;
        let el = mobile_toolbar(MobileToolbarProps {
            tools: &tools,
            state: &state,
            ai_enabled: true,
            diagram_to_code: false,
            id_prefix: "x".into(),
            ttd_trigger: None,
            on_event: None,
        });
        let mut expected: Vec<&str> = base.to_vec();
        expected.extend(extra);
        expected.push("extra-tools");
        assert_eq!(entries(&el), expected, "width {width}");
    }
}

#[test]
fn widths_follow_upstreams_thresholds_and_the_bottom_bar() {
    assert_eq!(TOOL_SIZE, 36.0);
    assert_eq!(TOOL_GAP, 4.0);
    assert_eq!(MIN_WIDTH, 7.0 * 36.0 + 6.0 * 4.0);
    for w in [
        0.0, 275.0, 276.0, 315.0, 316.0, 355.0, 356.0, 395.0, 396.0, 442.0,
    ] {
        let o = tools_outside(w);
        assert_eq!(o.text, w >= 316.0, "{w}");
        assert_eq!(o.image, w >= 356.0, "{w}");
        assert_eq!(o.frame, w >= 396.0, "{w}");
    }
    for row in fixture()["toolbarWidth"].as_array().unwrap() {
        let editor = row["editor"].as_f64().unwrap();
        assert_eq!(
            toolbar_width(editor),
            row["toolbar"].as_f64().unwrap(),
            "{editor}"
        );
    }
    let f = fixture();
    assert_eq!(f["scrollbar"]["width"].as_f64(), Some(SCROLLBAR_WIDTH));
    assert_eq!(f["scrollbar"]["margin"].as_f64(), Some(SCROLLBAR_MARGIN));
    // the compiled rules the widths come from
    assert!(MOBILE_MENU_CSS.contains("width: calc(100% - 28px);\n  max-width: 450px;"));
    assert!(MOBILE_MENU_CSS.contains(".excalidraw .App-bottom-bar > .Island {"));
    assert!(MOBILE_MENU_CSS.contains("  padding: 4px;\n"));
}

/// The phone rule (`isMobileBreakpoint`): width <= 599, or height < 500
/// with width < 1000.
#[test]
fn the_phone_rule() {
    for (w, h, phone) in [
        (390.0, 844.0, true),
        (599.0, 900.0, true),
        (599.5, 900.0, false),
        (600.0, 400.0, true),
        (999.0, 499.0, true),
        (1000.0, 499.0, false),
        (999.0, 500.0, false),
        (844.0, 390.0, true),
    ] {
        assert_eq!(get_form_factor(w, h) == FormFactor::Phone, phone, "{w}x{h}");
    }
}

/// App.tsx:3660-3683: the preferred selection tool starts as the lasso on
/// a phone and the selection elsewhere, and a restored selection-like
/// tool becomes it.
#[test]
fn a_phone_starts_with_the_lasso() {
    let mut tools = ToolState::default();
    init_selection_tool(&mut tools, FormFactor::Phone);
    assert_eq!(tools.preferred_selection_tool.tool, SelectionTool::Lasso);
    assert!(tools.preferred_selection_tool.initialized);
    assert_eq!(tools.active_tool.tool, Tool::Builtin(ToolType::Lasso));

    let mut tools = ToolState::default();
    init_selection_tool(&mut tools, FormFactor::Tablet);
    assert_eq!(
        tools.preferred_selection_tool.tool,
        SelectionTool::Selection
    );
    assert_eq!(tools.active_tool.tool, Tool::Builtin(ToolType::Selection));

    // an initialized preference stays; image restores to it too
    let mut tools = ToolState {
        preferred_selection_tool: PreferredSelectionTool {
            tool: SelectionTool::Selection,
            initialized: true,
        },
        ..ToolState::default()
    };
    tools.active_tool.tool = Tool::Builtin(ToolType::Image);
    init_selection_tool(&mut tools, FormFactor::Phone);
    assert_eq!(tools.active_tool.tool, Tool::Builtin(ToolType::Selection));

    // other tools stay
    let mut tools = ToolState::default();
    tools.active_tool.tool = Tool::Builtin(ToolType::Rectangle);
    init_selection_tool(&mut tools, FormFactor::Phone);
    assert_eq!(tools.active_tool.tool, Tool::Builtin(ToolType::Rectangle));
}

/// The bottom toolbar of mockup 06 (a 390 × 844 phone, lasso by default):
/// its buttons in order, the lasso trigger active.
#[test]
fn matches_mockup_06() {
    let html = include_str!("../../../site/static/mockups/06-phone.html");
    let start = html
        .find("class=\"Stack mob\"")
        .expect("the mockup's toolbar");
    let end = start + html[start..].find("</div>").unwrap();
    let titles: Vec<(&str, bool)> = html[start..end]
        .split("<button ")
        .skip(1)
        .map(|b| {
            let t = b.split("title=\"").nth(1).unwrap();
            (
                &t[..t.find('"').unwrap()],
                b.starts_with("class=\"ToolIcon active\""),
            )
        })
        .collect();
    let mockup: Vec<&str> = titles
        .iter()
        .map(|(t, _)| match *t {
            "Hand" => "hand",
            "Lasso / Selection" => "selection",
            "Draw / Autoshape" => "freedraw",
            "Eraser" => "eraser",
            "Rect / Diamond / Ellipse" => "rectangle",
            "Arrow / Line" => "arrow",
            "Text" => "text",
            "More" => "extra-tools",
            other => panic!("mockup button {other}"),
        })
        .collect();
    let active: Vec<&str> = titles.iter().filter(|(_, a)| *a).map(|(t, _)| *t).collect();
    assert_eq!(active, ["Lasso / Selection"]);

    assert_eq!(get_form_factor(390.0, 844.0), FormFactor::Phone);
    let mut tools = ToolState::default();
    init_selection_tool(&mut tools, FormFactor::Phone);
    let mut state = MobileToolbarState::new(&tools);
    state.width = toolbar_width(390.0);
    let el = mobile_toolbar(MobileToolbarProps {
        tools: &tools,
        state: &state,
        ai_enabled: true,
        diagram_to_code: false,
        id_prefix: "x".into(),
        ttd_trigger: None,
        on_event: None,
    });
    assert_eq!(entries(&el), mockup);
    let trigger = el
        .children()
        .iter()
        .find_map(|n| match n {
            Node::Element(e) if e.attribute("data-testid") == Some("toolbar-selection") => Some(e),
            _ => None,
        })
        .unwrap();
    assert_eq!(trigger.attribute("aria-pressed"), Some("true"));
    assert_eq!(trigger.attribute("title"), Some("Lasso selection"));
    // 36 px buttons (MobileToolbar.scss: 2.25rem icons)
    assert!(MOBILE_MENU_CSS.contains(
        ".excalidraw .mobile-toolbar .ToolIcon .ToolIcon__icon {\n  width: 2.25rem;\n  height: 2.25rem;\n}"
    ));
}

// -- behaviour ----------------------------------------------------------------

fn active(tools: &ToolState) -> ToolType {
    tools.active_tool.tool.builtin().unwrap()
}

#[test]
fn popover_triggers_activate_their_remembered_option() {
    let mut tools = ToolState::default();
    let mut state = MobileToolbarState::new(&tools);
    // the shapes trigger opens its popover and activates the rectangle
    apply_toolbar_event(
        &mut state,
        &mut tools,
        &ToolbarEvent::ToolPopoverTrigger(ToolPopover::Shapes),
    );
    assert_eq!(state.open_popover, Some(ToolPopover::Shapes));
    assert_eq!(active(&tools), ToolType::Rectangle);
    // an option activates its tool and is remembered; the popover stays
    apply_toolbar_event(
        &mut state,
        &mut tools,
        &ToolbarEvent::ToolPopoverOption(ToolPopover::Shapes, ToolType::Ellipse),
    );
    assert_eq!(active(&tools), ToolType::Ellipse);
    assert_eq!(state.open_popover, Some(ToolPopover::Shapes));
    assert_eq!(
        state.displayed(ToolPopover::Shapes, &tools),
        ToolType::Ellipse
    );
    // another trigger: its tool, and the shapes popover closes (its
    // options no longer hold the active tool)
    apply_toolbar_event(
        &mut state,
        &mut tools,
        &ToolbarEvent::ToolPopoverTrigger(ToolPopover::Linear),
    );
    assert_eq!(active(&tools), ToolType::Arrow);
    assert_eq!(state.open_popover, Some(ToolPopover::Linear));
    // a second press closes it
    apply_toolbar_event(
        &mut state,
        &mut tools,
        &ToolbarEvent::ToolPopoverTrigger(ToolPopover::Linear),
    );
    assert_eq!(state.open_popover, None);
    // the shapes trigger now brings back the ellipse
    apply_toolbar_event(
        &mut state,
        &mut tools,
        &ToolbarEvent::ToolPopoverTrigger(ToolPopover::Shapes),
    );
    assert_eq!(active(&tools), ToolType::Ellipse);
    // a tool chosen elsewhere closes the popover and is remembered
    apply_toolbar_event(
        &mut state,
        &mut tools,
        &ToolbarEvent::ExtraTool(ToolType::Frame),
    );
    assert_eq!(state.open_popover, None);
    tools.active_tool.tool = Tool::Builtin(ToolType::Line);
    state.sync(&tools);
    assert_eq!(state.displayed(ToolPopover::Linear, &tools), ToolType::Line);
    // a canvas press closes any popover
    apply_toolbar_event(
        &mut state,
        &mut tools,
        &ToolbarEvent::ToolPopoverTrigger(ToolPopover::Freedraw),
    );
    assert_eq!(state.open_popover, Some(ToolPopover::Freedraw));
    state.close_popovers();
    assert_eq!(state.open_popover, None);
}

#[test]
fn the_selection_popover_sets_the_preferred_selection_tool() {
    let mut tools = ToolState::default();
    let mut state = MobileToolbarState::new(&tools);
    apply_toolbar_event(
        &mut state,
        &mut tools,
        &ToolbarEvent::ToolPopoverOption(ToolPopover::Selection, ToolType::Lasso),
    );
    assert_eq!(active(&tools), ToolType::Lasso);
    assert_eq!(
        tools.preferred_selection_tool,
        PreferredSelectionTool {
            tool: SelectionTool::Lasso,
            initialized: true
        }
    );
    assert_eq!(
        state.displayed(ToolPopover::Selection, &tools),
        ToolType::Lasso
    );
    // the trigger activates the preferred tool
    tools.active_tool.tool = Tool::Builtin(ToolType::Hand);
    apply_toolbar_event(
        &mut state,
        &mut tools,
        &ToolbarEvent::ToolPopoverTrigger(ToolPopover::Selection),
    );
    assert_eq!(active(&tools), ToolType::Lasso);
}

#[test]
fn the_extra_tools_menu_toggles_and_selects() {
    let mut tools = ToolState::default();
    let mut state = MobileToolbarState::new(&tools);
    apply_toolbar_event(&mut state, &mut tools, &ToolbarEvent::ExtraToolsToggle);
    assert!(state.extra_tools_open);
    apply_toolbar_event(
        &mut state,
        &mut tools,
        &ToolbarEvent::ExtraTool(ToolType::Laser),
    );
    apply_toolbar_event(&mut state, &mut tools, &ToolbarEvent::ExtraToolsClose);
    assert!(!state.extra_tools_open);
    assert_eq!(active(&tools), ToolType::Laser);
    apply_toolbar_event(
        &mut state,
        &mut tools,
        &ToolbarEvent::ToolButton {
            tool: ToolType::Eraser,
            pointer_type: None,
        },
    );
    assert_eq!(active(&tools), ToolType::Eraser);
}

/// ToolPopover's radix placement: below the trigger by 16 px, centred on
/// it, flipped above when it overflows the container below, shifted into
/// the container.
#[test]
fn tool_popovers_open_above_the_bottom_bar() {
    let boundary = Rect {
        left: 0.0,
        top: 0.0,
        width: 390.0,
        height: 844.0,
    };
    let trigger = Rect {
        left: 200.0,
        top: 790.0,
        width: 36.0,
        height: 36.0,
    };
    let p = tool_popover_position(trigger, (130.0, 52.0), boundary);
    assert_eq!(p.side, DropdownSide::Top);
    assert_eq!(p.y, 790.0 - 16.0 - 52.0);
    assert_eq!(p.x, 218.0 - 65.0);
    let top = Rect {
        top: 100.0,
        ..trigger
    };
    let p = tool_popover_position(top, (130.0, 52.0), boundary);
    assert_eq!(p.side, DropdownSide::Bottom);
    assert_eq!(p.y, 100.0 + 36.0 + 16.0);
    let edge = Rect {
        left: 350.0,
        ..trigger
    };
    let p = tool_popover_position(edge, (130.0, 52.0), boundary);
    assert_eq!(p.x, 390.0 - 130.0);
    let edge = Rect {
        left: 0.0,
        ..trigger
    };
    let p = tool_popover_position(edge, (130.0, 52.0), boundary);
    assert_eq!(p.x, 0.0);
}

#[test]
fn toolbar_buttons_listen() {
    let mut tools = ToolState::default();
    tools.active_tool.tool = Tool::Builtin(ToolType::Rectangle);
    let mut state = MobileToolbarState::new(&tools);
    state.width = 442.0;
    state.open_popover = Some(ToolPopover::Shapes);
    state.extra_tools_open = true;
    let el = mobile_toolbar(MobileToolbarProps {
        tools: &tools,
        state: &state,
        ai_enabled: true,
        diagram_to_code: false,
        id_prefix: "x".into(),
        ttd_trigger: None,
        on_event: None,
    });
    let mut found: Vec<(String, Vec<String>)> = Vec::new();
    fn walk(n: &Node, found: &mut Vec<(String, Vec<String>)>) {
        if let Node::Element(el) = n {
            if let Some(id) = el.attribute("data-testid") {
                found.push((
                    id.to_owned(),
                    el.listened_events().map(str::to_owned).collect(),
                ));
            }
            el.children().iter().for_each(|c| walk(c, found));
        }
    }
    walk(&Node::Element(el), &mut found);
    for id in [
        "toolbar-hand",
        "toolbar-selection",
        "toolbar-rectangle",
        "toolbar-diamond",
        "toolbar-eraser",
        "toolbar-frame",
        "dropdown-menu-button",
        "toolbar-laser",
    ] {
        let events = found
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, e)| e.clone())
            .unwrap_or_else(|| panic!("no {id}"));
        assert!(events.iter().any(|e| e == "click"), "{id}: {events:?}");
    }
}
