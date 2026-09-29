//! The context menu (ex-525): `ContextMenu` (`components/ContextMenu.tsx`)
//! in its `Popover`, over the items of `App.getContextMenuItems`
//! (`components/App.tsx:13835-13936`), against upstream.
//!
//! Fixture: `tests/fixtures/context-menu.json`, upstream's own component
//! and actions at the pinned commit (`tools/goldens/context-menu.mjs`): per
//! case the scene, the app state over the defaults, the form factor, the
//! items getContextMenuItems returned, the tree the menu renders and, per
//! handler, its effects in the order upstream runs them.
//! `src/context_menu.css` is the same generator's compilation of
//! `ContextMenu.scss`. The item lists are those of
//! `site/content/research/ui-design-system.md` section 3.10 (view mode and
//! the desktop-only z-order rows among them;
//! `tools/goldens/test/context-menu.test.mjs` holds the fixture to them).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use excali_core::app_state::AppState;
use excali_core::element::Element as SceneElement;
use excali_editor::actions::{
    build_context_menu, get_context_menu_items, ActionContext, ActionEnv, ActionName, AppProps,
    ContextMenuItem, ContextMenuKind, FormFactor, KeyLabels,
};
use excali_ui::context_menu::{
    context_menu, context_menu_text, ContextMenuEffect, ContextMenuProps, CONTEXT_MENU_CSS,
};
use excali_ui::dom::{Element, Node};
use serde_json::{json, Map, Value};

fn fixture() -> &'static Value {
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/context-menu.json")).expect("context-menu.json")
    })
}

fn cases() -> &'static [Value] {
    let cases = fixture()["cases"].as_array().expect("cases");
    assert!(cases.len() >= 20, "the fixture lost cases");
    cases
}

// -- trees ----------------------------------------------------------------------

fn tree(node: &Node) -> Value {
    match node {
        Node::Text(text) => Value::String(text.clone()),
        Node::Element(el) => element_tree(el),
    }
}

fn element_tree(el: &Element) -> Value {
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
    assert!(
        el.style_properties().is_empty(),
        "<{}> has an inline style before it is mounted",
        el.tag()
    );
    let children: Vec<Value> = el.children().iter().map(tree).collect();
    if !children.is_empty() {
        out.insert("children".into(), Value::Array(children));
    }
    Value::Object(out)
}

/// The fixture's tree with attributes sorted and the handlers taken out,
/// in document order.
fn expected(node: &Value, handlers: &mut Vec<(String, Value)>) -> Value {
    let Some(obj) = node.as_object() else {
        return node.clone();
    };
    let mut out = Map::new();
    for key in ["tag", "class"] {
        if let Some(v) = obj.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    if let Some(v) = obj.get("attrs") {
        let sorted: BTreeMap<_, _> = v.as_object().unwrap().clone().into_iter().collect();
        out.insert("attrs".into(), json!(sorted));
    }
    if let Some(on) = obj.get("on").and_then(Value::as_object) {
        for (event, effects) in on {
            handlers.push((event.clone(), effects.clone()));
        }
    }
    if let Some(children) = obj.get("children").and_then(Value::as_array) {
        let children: Vec<Value> = children.iter().map(|c| expected(c, handlers)).collect();
        out.insert("children".into(), Value::Array(children));
    }
    Value::Object(out)
}

fn effect_json(effect: &ContextMenuEffect) -> Value {
    match effect {
        ContextMenuEffect::Close => json!({ "close": true }),
        ContextMenuEffect::ExecuteAction(name) => {
            json!({ "executeAction": name.as_str(), "source": "contextMenu" })
        }
        ContextMenuEffect::PreventDefault => json!({ "preventDefault": true }),
    }
}

// -- cases ----------------------------------------------------------------------

struct Case {
    id: String,
    kind: ContextMenuKind,
    elements: Vec<SceneElement>,
    app_state: AppState,
    props: AppProps,
    env: ActionEnv,
}

impl Case {
    fn new(c: &Value) -> Case {
        let kind = match c["type"].as_str().unwrap() {
            "canvas" => ContextMenuKind::Canvas,
            "element" => ContextMenuKind::Element,
            other => panic!("menu type {other}"),
        };
        let form_factor = match c["formFactor"].as_str().unwrap() {
            "desktop" => FormFactor::Desktop,
            "tablet" => FormFactor::Tablet,
            "phone" => FormFactor::Phone,
            other => panic!("form factor {other}"),
        };
        let elements = c["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| SceneElement::from_map(e.as_object().unwrap().clone()).expect("an element"))
            .collect();
        let mut app_state = AppState::default();
        for (k, v) in c["appState"].as_object().unwrap() {
            app_state.insert(k.clone(), v.clone());
        }
        let env = ActionEnv {
            form_factor,
            // the fixture's browser has the async clipboard
            clipboard_write_text: true,
            clipboard_blob: true,
            ..ActionEnv::default()
        };
        Case {
            id: c["id"].as_str().unwrap().to_owned(),
            kind,
            elements,
            app_state,
            props: AppProps::default(),
            env,
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

    fn view_mode(&self) -> bool {
        self.app_state.get("viewModeEnabled") == Some(&json!(true))
    }

    fn items(&self) -> Vec<ContextMenuItem> {
        get_context_menu_items(self.kind, self.view_mode(), self.env.form_factor)
    }
}

fn item_names(items: &[ContextMenuItem]) -> Vec<&'static str> {
    items
        .iter()
        .map(|i| match i {
            ContextMenuItem::Separator => "|",
            ContextMenuItem::Action(a) => a.as_str(),
        })
        .collect()
}

#[test]
fn items_match_get_context_menu_items() {
    for c in cases() {
        let case = Case::new(c);
        let want: Vec<&str> = c["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(item_names(&case.items()), want, "{}", case.id);
    }
}

#[test]
fn menus_render_as_upstream() {
    for c in cases() {
        let case = Case::new(c);
        let entries = build_context_menu(&case.items(), &case.ctx(), &KeyLabels::EN);
        let menu = context_menu(
            &entries,
            ContextMenuProps {
                top: c["top"].as_f64().unwrap(),
                left: c["left"].as_f64().unwrap(),
                viewport_width: c["viewport"]["width"].as_f64().unwrap(),
                viewport_height: c["viewport"]["height"].as_f64().unwrap(),
                on_effect: None,
            },
        );
        let mut want_handlers = Vec::new();
        let want: Vec<Value> = c["tree"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| expected(n, &mut want_handlers))
            .collect();
        assert_eq!(vec![element_tree(&menu.element)], want, "{}", case.id);
        let got_handlers: Vec<(String, Value)> = menu
            .handlers
            .iter()
            .map(|(event, effects)| {
                (
                    (*event).to_owned(),
                    Value::Array(effects.iter().map(effect_json).collect()),
                )
            })
            .collect();
        assert_eq!(got_handlers, want_handlers, "{}", case.id);
    }
}

#[test]
fn every_label_has_its_english_text() {
    let locale = fixture()["locale"].as_object().unwrap();
    assert!(locale.len() >= 40, "the fixture lost labels");
    for (key, text) in locale {
        assert_eq!(context_menu_text(key), text.as_str().unwrap(), "{key}");
    }
}

#[test]
fn the_menus_actions_label_only_keys_the_locale_holds() {
    // every static label of an action the menus hold (dynamic labels are
    // the rendered cases')
    let locale = fixture()["locale"].as_object().unwrap();
    for kind in [ContextMenuKind::Canvas, ContextMenuKind::Element] {
        for view_mode in [false, true] {
            for item in get_context_menu_items(kind, view_mode, FormFactor::Desktop) {
                let ContextMenuItem::Action(name) = item else {
                    continue;
                };
                if let Some(excali_editor::actions::ActionLabel::Key(key)) = name.spec().label {
                    assert!(locale.contains_key(key), "{} {key}", name.as_str());
                }
            }
        }
    }
    assert_eq!(context_menu_text("no.such.key"), "no.such.key");
}

#[test]
fn delete_is_the_dangerous_row() {
    let c = cases()
        .iter()
        .find(|c| c["id"] == "element-rectangle")
        .unwrap();
    let case = Case::new(c);
    let entries = build_context_menu(&case.items(), &case.ctx(), &KeyLabels::EN);
    let html = context_menu(
        &entries,
        ContextMenuProps {
            top: 0.0,
            left: 0.0,
            viewport_width: 1024.0,
            viewport_height: 768.0,
            on_effect: None,
        },
    )
    .element
    .to_html();
    assert!(html.contains(r#"<button type="button" class="context-menu-item dangerous">"#));
    assert_eq!(html.matches("dangerous").count(), 1);
    let _ = ActionName::DeleteSelectedElements;
}

#[test]
fn the_stylesheet_is_upstreams_context_menu_scss() {
    assert!(CONTEXT_MENU_CSS.starts_with("/* Generated by tools/goldens/context-menu.mjs"));
    for selector in [
        ".excalidraw .context-menu-popover",
        ".excalidraw .context-menu-item.checkmark::before",
        ".excalidraw .context-menu-item.dangerous .context-menu-item__label",
        ".excalidraw .context-menu-item-separator",
    ] {
        assert!(CONTEXT_MENU_CSS.contains(selector), "{selector}");
    }
}
