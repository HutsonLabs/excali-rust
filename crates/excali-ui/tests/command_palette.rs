//! The command palette (ex-527): upstream's `CommandPalette`
//! (`components/CommandPalette/CommandPalette.tsx`) in its `Dialog` and
//! `Modal`, against upstream.
//!
//! Fixture: `tests/fixtures/command-palette.json`, upstream's palette at
//! the pinned commit over its own actions and fuzzy 0.1.3, rendered by
//! React 19.0.0 into jsdom per platform (`tools/goldens/command-palette.mjs`):
//! per case the scene, the app state over the defaults, the props the
//! palette reads, the library, the hosted app's Links items, the command
//! run before (`lastUsed`), the search and the keys pressed; what the
//! palette shows (recents, the categories and their items, the selection
//! after each key) and, for some, the tree it portals to the body; per
//! command of the `perform` cases what a click on it does; and the toggle
//! shortcut per key. `src/command_palette/command_palette.css` is the same
//! generator's CommandPalette.scss. The categories and lists are those of
//! `site/content/research/ui-design-system.md` section 3.14
//! (`tools/goldens/test/command-palette.test.mjs` holds the fixture to them).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use excali_core::app_state::AppState;
use excali_core::element::Element as SceneElement;
use excali_core::library::{LibraryItem, LibraryItemStatus};
use excali_editor::actions::{ActionContext, ActionEnv, AppProps, FormFactor};
use excali_editor::keyboard::{
    command_palette_key_down, is_command_palette_toggle_shortcut, Keystroke,
};
use excali_scene::shape::Theme;
use excali_ui::command_palette::{
    command_palette, hosted_app_links, library_commands, palette_commands, palette_key_down,
    palette_text, palette_view, perform_command, shortcut_keys, CommandPaletteProps,
    PaletteCommand, PaletteEffect, PaletteEnv, PaletteView, COMMAND_PALETTE_CSS,
};
use excali_ui::dom::Node;
use serde_json::{json, Map, Value};

fn fixture() -> &'static Value {
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/command-palette.json"))
            .expect("command-palette.json")
    })
}

fn cases() -> &'static [Value] {
    let cases = fixture()["cases"].as_array().expect("cases");
    assert!(cases.len() >= 30, "the fixture lost cases");
    cases
}

// -- a case's state -------------------------------------------------------------

struct Case {
    name: String,
    elements: Vec<SceneElement>,
    app_state: AppState,
    props: AppProps,
    env: ActionEnv,
    palette: PaletteEnv,
    library: Vec<LibraryItem>,
    links: bool,
}

impl Case {
    fn new(c: &Value) -> Case {
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
        let darwin = c["darwin"].as_bool().unwrap_or(false);
        let phone = c["formFactor"].as_str() == Some("phone");
        let mut props = AppProps::default();
        props.canvas_actions.toggle_theme = c["toggleTheme"].as_bool().unwrap().then_some(true);
        let env = ActionEnv {
            is_darwin: darwin,
            form_factor: if phone {
                FormFactor::Phone
            } else {
                FormFactor::Desktop
            },
            // the fixture's browser has the async clipboard
            clipboard_write_text: true,
            clipboard_blob: true,
            ..ActionEnv::default()
        };
        let library = c["library"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let mut item = LibraryItem::new(
                    format!("lib{i}"),
                    LibraryItemStatus::Unpublished,
                    Vec::new(),
                    1.0,
                );
                item.name = name.as_str().map(str::to_owned);
                item
            })
            .collect();
        Case {
            name: c["name"].as_str().unwrap().to_owned(),
            elements,
            app_state,
            props,
            env,
            palette: PaletteEnv {
                is_darwin: darwin,
                phone,
                ai_enabled: c["aiEnabled"].as_bool().unwrap_or(true),
                image_tool: c["tools"]["image"].as_bool().unwrap_or(true),
            },
            library,
            links: c["links"].as_bool().unwrap(),
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

    fn commands(&self) -> Vec<PaletteCommand> {
        let custom = if self.links {
            hosted_app_links()
        } else {
            Vec::new()
        };
        palette_commands(&self.ctx(), &self.palette, custom)
    }

    fn view(&self, search: &str, last_used: Option<&str>) -> PaletteView {
        palette_view(
            &self.commands(),
            &library_commands(&self.library),
            search,
            last_used,
        )
    }
}

fn item_json(view: &PaletteView, c: &PaletteCommand, phone: bool, disabled: bool) -> Value {
    json!({
        "label": c.label,
        "shortcut": if phone { None } else { c.shortcut.as_deref().map(shortcut_keys) },
        "icon": c.icon.unwrap_or("library-item"),
        "selected": view.current.as_deref() == Some(c.label.as_str()),
        "disabled": disabled,
        "large": c.library_item.is_some(),
    })
}

// -- lists, searches and keys ---------------------------------------------------

#[test]
fn every_case_lists_upstreams_commands() {
    for c in cases() {
        let case = Case::new(c);
        let search = c["search"].as_str().unwrap();
        let last_used = c["lastUsed"].as_str();
        let mut view = case.view(search, last_used);
        // the keys first: the fixture records the palette after them
        let mut selections = Vec::new();
        for key in c["keys"].as_array().unwrap() {
            let out = palette_key_down(&view, key.as_str().unwrap(), false, false);
            if let Some(current) = out.current {
                view.current = Some(current);
            }
            selections.push(json!(view.current));
        }
        let phone = case.palette.phone;
        let recents = view
            .recents
            .as_ref()
            .map(|r| item_json(&view, &r.command, phone, !r.available));
        assert_eq!(
            recents.unwrap_or(Value::Null),
            c["recents"],
            "{}: recents",
            case.name
        );
        let categories: Vec<Value> = view
            .categories
            .iter()
            .map(|(title, items)| {
                json!({
                    "title": title,
                    "items": items.iter().map(|i| item_json(&view, i, phone, false)).collect::<Vec<_>>(),
                })
            })
            .collect();
        assert_eq!(
            Value::Array(categories),
            c["categories"],
            "{}: categories",
            case.name
        );
        assert_eq!(
            json!(view.categories.is_empty()),
            c["noMatch"],
            "{}: no match",
            case.name
        );
        assert_eq!(
            Value::Array(selections),
            c["selections"],
            "{}: selections",
            case.name
        );
    }
}

#[test]
fn keys_other_than_arrows_and_enter() {
    let case = Case::new(&cases()[0]);
    let view = case.view("", None);
    // a letter outside the input focuses the input; the editor's shortcuts
    // are kept from it
    let out = palette_key_down(&view, "a", false, false);
    assert!(out.focus_input && out.stop_propagation && !out.prevent_default);
    assert_eq!(out.current, None);
    // any other key outside the input is swallowed
    let out = palette_key_down(&view, "Tab", false, false);
    assert!(!out.focus_input && out.stop_propagation && out.prevent_default);
    // typing in the input, Escape and the toggle are left alone
    for (key, writable, toggle) in [
        ("a", true, false),
        ("Escape", false, false),
        ("/", false, true),
    ] {
        let out = palette_key_down(&view, key, writable, toggle);
        assert!(
            !out.stop_propagation && !out.prevent_default && !out.execute,
            "{key}"
        );
    }
    // Enter runs the current command (after a timeout, as upstream)
    let out = palette_key_down(&view, "Enter", true, false);
    assert!(out.execute && !out.prevent_default);
    // arrows move the selection even from the input
    let out = palette_key_down(&view, "ArrowDown", true, false);
    assert!(out.prevent_default);
    assert_eq!(out.current.as_deref(), Some("Find on canvas"));
}

// -- DOM ------------------------------------------------------------------------

fn tree(node: &Node) -> Value {
    match node {
        Node::Text(text) => Value::String(text.clone()),
        Node::Element(el) => {
            let attrs: BTreeMap<_, _> = el
                .attributes()
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            // jsdom 22's CSSStyleDeclaration drops `width: var(...)`, which
            // InlineIcon sets (see `inline_icons_take_the_icon_size`)
            let style: BTreeMap<_, _> = el
                .style_properties()
                .iter()
                .filter(|(k, v)| !(k == "width" && v.starts_with("var(")))
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

fn render(c: &Value) -> excali_ui::dom::Element {
    let case = Case::new(c);
    let search = c["search"].as_str().unwrap();
    let view = case.view(search, c["lastUsed"].as_str());
    command_palette(CommandPaletteProps {
        view,
        search: search.to_owned(),
        is_darwin: case.palette.is_darwin,
        phone: case.palette.phone,
        theme: Theme::Light,
        container_id: "excalidraw-id".into(),
        ..CommandPaletteProps::default()
    })
}

#[test]
fn dom_cases_render_upstreams_tree() {
    let mut rendered = 0;
    for c in cases().iter().filter(|c| c.get("dom").is_some()) {
        let name = c["name"].as_str().unwrap();
        let expected: Vec<Value> = c["dom"].as_array().unwrap().iter().map(expand).collect();
        let actual = vec![tree(&Node::Element(render(c)))];
        if actual != expected {
            panic!(
                "case {name}:\nexpected {}\nactual   {}",
                serde_json::to_string(&expected).unwrap(),
                serde_json::to_string(&actual).unwrap()
            );
        }
        rendered += 1;
    }
    assert!(rendered >= 8, "the fixture lost DOM cases");
}

fn find_all<'a>(
    el: &'a excali_ui::dom::Element,
    class: &str,
    out: &mut Vec<&'a excali_ui::dom::Element>,
) {
    if el
        .attribute("class")
        .is_some_and(|c| c.split(' ').any(|x| x == class))
    {
        out.push(el);
    }
    for child in el.children() {
        if let Node::Element(e) = child {
            find_all(e, class, out);
        }
    }
}

#[test]
fn inline_icons_take_the_icon_size() {
    // InlineIcon (components/InlineIcon.tsx) with size="var(--icon-size, 1rem)"
    let root = render(&cases()[0]);
    let mut names = Vec::new();
    find_all(&root, "name", &mut names);
    assert!(names.len() > 40);
    for name in names {
        let Some(Node::Element(icon)) = name.children().first() else {
            panic!("a command without an icon");
        };
        let width = icon
            .style_properties()
            .iter()
            .find(|(k, _)| k == "width")
            .map(|(_, v)| v.as_str());
        assert_eq!(width, Some("var(--icon-size, 1rem)"));
    }
}

// -- what a click does ------------------------------------------------------------

fn effect_json(effect: &PaletteEffect, app_state: &AppState) -> Value {
    match effect {
        PaletteEffect::SetAppState(patch) => {
            let changed: Map<String, Value> = patch
                .iter()
                .filter(|(k, v)| app_state.get(k) != Some(*v))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            json!({ "setAppState": changed })
        }
        PaletteEffect::ExecuteAction(name, source) => {
            json!({ "executeAction": name.as_str(), "source": source.as_str() })
        }
        PaletteEffect::ConfirmDialog(name) => json!({ "confirmDialog": name }),
        PaletteEffect::SetActiveTool(tool) => json!({ "setActiveTool": tool.as_str() }),
        PaletteEffect::ToggleLock => json!({ "toggleLock": true }),
        PaletteEffect::InsertLibraryItem(_) => json!({ "insertElements": 1 }),
        PaletteEffect::OpenUrl(url) => json!({ "openUrl": url }),
    }
}

#[test]
fn clicks_perform_as_upstream() {
    let perform = fixture()["perform"].as_array().unwrap();
    let mut checked = 0;
    for p in perform {
        let mut c = p.clone();
        c["darwin"] = json!(false);
        c["formFactor"] = json!("desktop");
        let case = Case::new(&c);
        let view = case.view(p["search"].as_str().unwrap(), None);
        let shown: Vec<&PaletteCommand> = view.categories.iter().flat_map(|(_, i)| i).collect();
        let labels: Vec<&str> = shown.iter().map(|c| c.label.as_str()).collect();
        let want: Vec<&str> = p["commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["label"].as_str().unwrap())
            .collect();
        assert_eq!(labels, want, "{}", case.name);
        for (command, expected) in shown.iter().zip(p["commands"].as_array().unwrap()) {
            let effects: Vec<Value> = perform_command(command, &case.app_state)
                .iter()
                .map(|e| effect_json(e, &case.app_state))
                .collect();
            assert_eq!(
                Value::Array(effects),
                expected["effects"],
                "{}: {}",
                case.name,
                command.label
            );
            checked += 1;
        }
    }
    assert!(checked >= 100, "the fixture lost commands");
}

// -- the toggle -------------------------------------------------------------------

#[test]
fn the_toggle_shortcut_is_upstreams() {
    let toggles = fixture()["toggle"].as_array().unwrap();
    assert!(toggles.len() >= 30);
    for t in toggles {
        let flag = |k: &str| t[k].as_bool().unwrap();
        let mut stroke = Keystroke::new(t["key"].as_str().unwrap(), "");
        stroke.modifiers.ctrl_key = flag("ctrlKey");
        stroke.modifiers.meta_key = flag("metaKey");
        stroke.modifiers.shift_key = flag("shiftKey");
        stroke.modifiers.alt_key = flag("altKey");
        let mut state = AppState::default();
        if flag("open") {
            state.insert("openDialog", json!({ "name": "commandPalette" }));
        }
        let before = state.get("openDialog").cloned();
        let out = command_palette_key_down(&mut state, &stroke, flag("darwin"));
        // while open, the palette's own handler sees the key too
        let inner = flag("open")
            && palette_key_down(
                &PaletteView::default(),
                &stroke.key,
                false,
                is_command_palette_toggle_shortcut(&stroke, flag("darwin")),
            )
            .prevent_default;
        assert_eq!(out.prevent_default || inner, flag("prevented"), "{t}");
        let after = state.get("openDialog").cloned();
        let got = if after == before {
            json!("unchanged")
        } else {
            after.unwrap_or(Value::Null)
        };
        assert_eq!(got, t["openDialog"], "{t}");
    }
}

// -- text and styles ----------------------------------------------------------------

#[test]
fn every_label_has_its_english_text() {
    let locale = fixture()["locale"].as_object().unwrap();
    assert!(locale.len() >= 80, "the fixture lost labels");
    for (key, text) in locale {
        assert_eq!(palette_text(key), text.as_str().unwrap(), "{key}");
    }
}

#[test]
fn the_stylesheet_is_upstreams() {
    assert!(COMMAND_PALETTE_CSS.contains(".excalidraw .command-palette-dialog"));
    assert!(COMMAND_PALETTE_CSS.contains("command-item-large"));
}

#[test]
fn shortcut_keys_split_like_command_shortcut_hint() {
    assert_eq!(shortcut_keys("Ctrl+Shift+E"), vec!["Ctrl", "Shift", "E"]);
    assert_eq!(shortcut_keys("Ctrl++"), vec!["Ctrl", "+"]);
    assert_eq!(shortcut_keys("↑↓"), vec!["↑↓"]);
}
