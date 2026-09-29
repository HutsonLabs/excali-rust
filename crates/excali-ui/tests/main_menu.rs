//! The main menu (ex-520): `MainMenu` (`components/main-menu/MainMenu.tsx`),
//! the default menu LayerUI renders (`components/LayerUI.tsx:111-136`), the
//! items of `main-menu/DefaultItems.tsx` (the Light/Dark/System theme radio
//! and the Preferences submenu among them) and the DropdownMenu components
//! under them (`components/dropdownMenu/*`), against upstream.
//!
//! Fixture: `tests/fixtures/main-menu.json`, upstream's own components at
//! the pinned commit (`tools/goldens/main-menu.mjs`): per case the app
//! state, `UIOptions.canvasActions`, the scene size and form factor, the
//! tree the menu renders (`{ icon }` for an icons.tsx export, `{ action }`
//! where it calls `renderAction`) and, per handler, its effects in the
//! order upstream runs them. `src/main_menu.css` is the same generator's
//! compilation of upstream's SCSS.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::OnceLock;

use excali_core::app_state::AppState;
use excali_core::element::{Element as SceneElement, ElementBase, ElementKind};
use excali_editor::actions::{
    ActionContext, ActionEnv, ActionManager, ActionName, AppProps, CanvasActions, FormFactor,
    KeyLabels,
};
use excali_scene::shape::Theme;
use excali_ui::dom::{Element, Node};
use excali_ui::icons;
use excali_ui::main_menu::{
    command_palette, default_main_menu, dropdown_menu, live_collaboration_trigger, menu_content,
    menu_text, preferences, toggle_theme_radio, Handler, MenuContext, MenuEffect, ThemeChoice,
    MAIN_MENU_CSS,
};
use serde_json::{json, Map, Value};

fn fixture() -> &'static Value {
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/main-menu.json")).expect("main-menu.json")
    })
}

// -- trees ----------------------------------------------------------------------

/// A Rust node as the fixture records one: class apart from the other
/// attributes, style as a map, `{ action }` for the render-action slot.
fn tree(node: &Node) -> Value {
    match node {
        Node::Text(text) => Value::String(text.clone()),
        Node::Element(el) => element_tree(el),
    }
}

fn element_tree(el: &Element) -> Value {
    if el.tag() == "x-action" {
        return json!({ "action": el.attribute("name").unwrap() });
    }
    let mut out = Map::new();
    out.insert("tag".into(), json!(el.tag()));
    if let Some(class) = el.attribute("class") {
        if !class.is_empty() {
            out.insert("class".into(), json!(class));
        }
    }
    let attrs: BTreeMap<_, _> = el
        .attributes()
        .iter()
        .filter(|(k, _)| k != "class")
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();
    if !attrs.is_empty() {
        out.insert("attrs".into(), json!(attrs));
    }
    let style: BTreeMap<_, _> = el
        .style_properties()
        .iter()
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();
    if !style.is_empty() {
        out.insert("style".into(), json!(style));
    }
    let children: Vec<Value> = el.children().iter().map(tree).collect();
    if !children.is_empty() {
        out.insert("children".into(), Value::Array(children));
    }
    Value::Object(out)
}

/// The fixture's tree with `{ icon }` replaced by the icon's own tree,
/// attributes and style sorted, and the handlers taken out (in document
/// order, `select` read as `click`: Radix selects an item on click).
fn expected(nodes: &Value, handlers: &mut Vec<(String, Value)>) -> Vec<Value> {
    nodes
        .as_array()
        .unwrap()
        .iter()
        .map(|n| expected_node(n, handlers))
        .collect()
}

fn expected_node(node: &Value, handlers: &mut Vec<(String, Value)>) -> Value {
    let Some(obj) = node.as_object() else {
        return node.clone();
    };
    if let Some(name) = obj.get("icon").and_then(Value::as_str) {
        let icon = icons::icon(name).unwrap_or_else(|| panic!("icon {name}"));
        return element_tree(&icon.element(Theme::Light).unwrap());
    }
    if obj.contains_key("action") {
        return node.clone();
    }
    let mut out = Map::new();
    for key in ["tag", "class"] {
        if let Some(v) = obj.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    for key in ["attrs", "style"] {
        if let Some(v) = obj.get(key) {
            let sorted: BTreeMap<_, _> = v.as_object().unwrap().clone().into_iter().collect();
            out.insert(key.into(), json!(sorted));
        }
    }
    if let Some(on) = obj.get("on").and_then(Value::as_object) {
        for (event, effects) in on {
            let event = if event == "select" { "click" } else { event };
            handlers.push((event.to_owned(), effects.clone()));
        }
    }
    if let Some(children) = obj.get("children") {
        out.insert(
            "children".into(),
            Value::Array(expected(children, handlers)),
        );
    }
    Value::Object(out)
}

// -- effects --------------------------------------------------------------------

fn effect_json(effect: &MenuEffect) -> Value {
    match effect {
        MenuEffect::ExecuteAction(name) => json!({ "executeAction": name.as_str() }),
        MenuEffect::SetAppState(patch) => json!({ "setAppState": patch }),
        MenuEffect::ToggleLock => json!({ "toggleLock": null }),
        MenuEffect::ConfirmDialog(name) => json!({ "confirmDialog": name }),
        MenuEffect::ConfirmOverwrite { .. } => json!({
            "openConfirmModal": menu_text("overwriteConfirm.modal.loadFromFile.title")
        }),
        MenuEffect::ThemeChange(theme) => json!({ "onThemeChange": theme.as_str() }),
        MenuEffect::TrackEvent {
            category,
            action,
            label,
        } => json!({ "trackEvent": [category, action, label] }),
        MenuEffect::Select => json!({ "onSelect": null }),
        MenuEffect::Warn(message) => json!({ "warn": message }),
    }
}

/// What a handler does in upstream's order: its effects, then the menu's
/// close (the content's `onSelect`, unless the item prevented it), then
/// what waits on a confirmation (confirmed).
fn sequence(handler: &Handler) -> (String, Value) {
    let mut now = Vec::new();
    let mut later = Vec::new();
    for effect in &handler.effects {
        now.push(effect_json(effect));
        if let MenuEffect::ConfirmOverwrite { then } = effect {
            later.push(effect_json(&MenuEffect::ExecuteAction(*then)));
        }
    }
    if handler.close_menu {
        now.push(json!({ "setAppState": { "openMenu": null } }));
    }
    now.extend(later);
    (handler.event.to_owned(), Value::Array(now))
}

// -- cases ----------------------------------------------------------------------

fn scene(count: u64) -> Vec<SceneElement> {
    (0..count)
        .map(|i| {
            let base = ElementBase::new(format!("e{i}"), 0.0, 0.0, 1.0, 1.0);
            SceneElement::new(base, ElementKind::Rectangle)
        })
        .collect()
}

fn app_state(case: &Value) -> AppState {
    let mut state = AppState::default();
    state.insert("openMenu", json!("canvas"));
    for (k, v) in case["appState"].as_object().unwrap() {
        state.insert(k.clone(), v.clone());
    }
    state
}

fn canvas_actions(v: &Value) -> CanvasActions {
    let flag = |k: &str| v[k].as_bool().unwrap();
    CanvasActions {
        change_view_background_color: flag("changeViewBackgroundColor"),
        clear_canvas: flag("clearCanvas"),
        export: v["export"]
            .as_object()
            .map(|o| o["saveFileToDisk"].as_bool().unwrap()),
        load_scene: flag("loadScene"),
        save_to_active_file: flag("saveToActiveFile"),
        toggle_theme: Some(flag("toggleTheme")),
        save_as_image: flag("saveAsImage"),
    }
}

struct Setup {
    elements: Vec<SceneElement>,
    app_state: AppState,
    props: AppProps,
    env: ActionEnv,
}

impl Setup {
    fn new(case: &Value, canvas: CanvasActions) -> Setup {
        let phone = case["phone"].as_bool().unwrap();
        Setup {
            elements: scene(case.get("elements").and_then(Value::as_u64).unwrap_or(0)),
            app_state: app_state(case),
            props: AppProps {
                canvas_actions: canvas,
                view_mode_enabled: case
                    .get("viewModeProp")
                    .and_then(Value::as_bool)
                    .filter(|on| *on),
                ..AppProps::default()
            },
            env: ActionEnv {
                form_factor: if phone {
                    FormFactor::Phone
                } else {
                    FormFactor::Desktop
                },
                ..ActionEnv::default()
            },
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

    fn menu(&self) -> MenuContext<'static> {
        let sink: Rc<RefCell<Vec<MenuEffect>>> = Rc::default();
        MenuContext::new(
            self.env.form_factor == FormFactor::Phone,
            false,
            &KeyLabels::EN,
            Theme::Light,
            Rc::new(move |e| sink.borrow_mut().push(e)),
        )
    }
}

fn render_action(name: ActionName) -> Option<Node> {
    Some(Element::new("x-action").attr("name", name.as_str()).into())
}

/// Where two trees first differ: the path and both values.
fn first_diff(got: &Value, want: &Value, path: String) -> Option<String> {
    match (got, want) {
        (Value::Object(a), Value::Object(b)) => {
            let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
            keys.into_iter().find_map(|k| {
                let (x, y) = (
                    a.get(k).unwrap_or(&Value::Null),
                    b.get(k).unwrap_or(&Value::Null),
                );
                first_diff(x, y, format!("{path}.{k}"))
            })
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => a
            .iter()
            .zip(b)
            .enumerate()
            .find_map(|(i, (x, y))| first_diff(x, y, format!("{path}[{i}]"))),
        _ if got == want => None,
        _ => Some(format!("{path}: got {got}, want {want}")),
    }
}

fn check(id: &str, got: &Node, cx: &MenuContext<'_>, want: &Value) {
    let mut want_handlers = Vec::new();
    let want_tree = expected(want, &mut want_handlers);
    let got_tree = vec![tree(got)];
    if let Some(diff) = first_diff(&json!(got_tree), &json!(want_tree), String::new()) {
        panic!("{id}: tree differs at {diff}");
    }
    let got_handlers: Vec<(String, Value)> = cx.handlers().iter().map(sequence).collect();
    assert_eq!(got_handlers, want_handlers, "{id}: handlers");
}

#[test]
fn default_menus_match_upstream() {
    let manager = ActionManager::new();
    let menus = fixture()["menus"].as_array().unwrap();
    assert!(menus.len() >= 9);
    for case in menus {
        let id = case["id"].as_str().unwrap();
        let setup = Setup::new(case, canvas_actions(&case["canvasActions"]));
        let cx = setup.menu();
        let menu = default_main_menu(&cx, &manager, &setup.ctx(), &render_action);
        check(id, &menu.into(), &cx, &case["tree"]);
    }
}

#[test]
fn items_match_upstream() {
    let manager = ActionManager::new();
    let items = fixture()["items"].as_array().unwrap();
    let defaults = json!({
        "changeViewBackgroundColor": true, "clearCanvas": true,
        "export": { "saveFileToDisk": true }, "loadScene": true,
        "saveToActiveFile": true, "toggleTheme": true, "saveAsImage": true
    });
    for case in items {
        let id = case["id"].as_str().unwrap();
        let setup = Setup::new(case, canvas_actions(&defaults));
        let cx = setup.menu();
        let props = &case["props"];
        let item: Vec<Node> = match case["component"].as_str().unwrap() {
            "ToggleTheme" => {
                let theme = match props["theme"].as_str().unwrap() {
                    "light" => ThemeChoice::Light,
                    "dark" => ThemeChoice::Dark,
                    _ => ThemeChoice::System,
                };
                let handler = case["themeHandler"].as_bool().unwrap();
                toggle_theme_radio(&cx, theme, handler)
            }
            "Preferences" => preferences(&cx, &manager, &setup.ctx(), true),
            "CommandPalette" => vec![command_palette(&cx, None).into()],
            "LiveCollaborationTrigger" => {
                let active = props["isCollaborating"].as_bool().unwrap();
                vec![live_collaboration_trigger(&cx, active).into()]
            }
            other => panic!("component {other}"),
        };
        let menu = dropdown_menu(None, Some(menu_content(&cx, "main-menu", item)));
        check(id, &menu.into(), &cx, &case["tree"]);
    }
}

#[test]
fn english_strings_match_upstream() {
    for (key, text) in fixture()["locale"].as_object().unwrap() {
        assert_eq!(menu_text(key), text.as_str().unwrap(), "{key}");
    }
}

#[test]
fn theme_radio_offers_light_dark_and_system() {
    assert_eq!(
        [ThemeChoice::Light, ThemeChoice::Dark, ThemeChoice::System].map(ThemeChoice::as_str),
        ["light", "dark", "system"]
    );
}

#[test]
fn stylesheet_is_upstreams() {
    for rule in [
        ".excalidraw .dropdown-menu {",
        ".excalidraw .dropdown-menu .dropdown-menu-item__shortcut {",
        ".excalidraw .dropdown-menu .dropdown-menu-group-title {",
        ".excalidraw .ActiveFile .ActiveFile__fileName {",
    ] {
        assert!(MAIN_MENU_CSS.contains(rule), "{rule}");
    }
}

#[test]
fn closed_preferences_submenu_is_hidden() {
    let manager = ActionManager::new();
    let case = json!({ "appState": {}, "phone": false });
    let setup = Setup::new(&case, CanvasActions::default());
    let cx = setup.menu();
    let nodes = preferences(&cx, &manager, &setup.ctx(), false);
    let [Node::Element(trigger), Node::Element(content)] = &nodes[..] else {
        panic!("a trigger and a content");
    };
    assert_eq!(trigger.attribute("aria-expanded"), Some("false"));
    assert_eq!(content.attribute("hidden"), Some(""));
    let events: Vec<&str> = trigger.listened_events().collect();
    assert_eq!(events, ["click", "pointerenter"]);
    // its items are bound all the same: two radios of two, nine checkboxes
    let handlers = cx.handlers();
    assert_eq!(handlers.len(), 13);
    assert!(handlers.iter().all(|h| !h.close_menu));
}
