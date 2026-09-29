//! The library sidebar (ex-526): upstream's `DefaultSidebar`
//! (`components/DefaultSidebar.tsx`) with the Sidebar parts
//! (`components/Sidebar/*`: the island, its header with the Search and
//! Library tab triggers, the dock and close buttons, the tabs), the
//! `LibraryMenu` in the library tab (`LibraryMenu.tsx`,
//! `LibraryMenuItems.tsx`, `LibraryMenuSection.tsx`, `LibraryUnit.tsx`,
//! `LibraryMenuHeaderContent.tsx`, `LibraryMenuControlButtons.tsx`,
//! `LibraryMenuBrowseButton.tsx`) and LayerUI's library trigger.
//!
//! Fixture: `tests/fixtures/library-sidebar.json`, written by
//! `tools/goldens/library-sidebar.mjs` from upstream at the pinned commit
//! (React 19.0.0, radix-ui 1.4.3, jsdom 22.1.0): every case's DOM, and
//! every interaction replayed step by step through
//! [`update`], its effects compared with what upstream's handlers did and
//! the DOM after each step with upstream's. `src/library_sidebar/
//! library_sidebar.css` is the same generator's stylesheets.

use std::collections::{BTreeMap, HashMap};

use excali_core::element::Element as SceneElement;
use excali_core::library::{LibraryItem, LibraryItemStatus};
use excali_scene::shape::Theme;
use excali_ui::dom::{Element, Node};
use excali_ui::library_sidebar::{
    default_sidebar, default_sidebar_trigger, drag_data, filter_library_items, is_sidebar_docked,
    is_sidebar_docked_and_fits, library_menu_actions, library_text, update, BrowseLink, DragStart,
    KeyTarget, LibraryContext, LibraryEffect, LibraryMenuAction, LibraryMenuState,
    LibrarySidebarEvent, LibrarySidebarProps, LibraryStatus, OpenSidebar, Previews,
    SidebarTriggerProps, UnitKey, CANVAS_SEARCH_TAB, DEFAULT_SIDEBAR_NAME, LIBRARY_SIDEBAR_CSS,
    LIBRARY_SIDEBAR_TAB, LIBRARY_URL,
};
use serde_json::{json, Map, Value};

fn fixture() -> &'static Value {
    use std::sync::OnceLock;
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/library-sidebar.json")).unwrap()
    })
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_owned())
        .collect()
}

fn scene_element(v: &Value) -> SceneElement {
    SceneElement::from_map(v.as_object().unwrap().clone()).unwrap()
}

/// The fixture's items as the library holds them.
fn all_items() -> Vec<LibraryItem> {
    fixture()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            let mut item = json!({
                "id": i["id"],
                "status": i["status"],
                "created": i["created"],
                "elements": i["elements"],
            });
            if let Some(name) = i["name"].as_str() {
                item["name"] = json!(name);
            }
            let items = excali_core::library::restore_library_items(
                Some(&json!([item])),
                LibraryItemStatus::Unpublished,
                &mut excali_core::restore::TestEnv::default(),
            )
            .unwrap();
            items.into_iter().next().unwrap()
        })
        .collect()
}

fn items_of(case: &Value) -> Vec<LibraryItem> {
    let all = all_items();
    strs(&case["items"])
        .iter()
        .map(|id| all.iter().find(|i| &i.id == id).unwrap().clone())
        .collect()
}

fn canvas() -> Vec<SceneElement> {
    fixture()["canvas"]
        .as_array()
        .unwrap()
        .iter()
        .map(scene_element)
        .collect()
}

/// The pending elements: the canvas elements the case selects.
fn pending_of(selected: &[String]) -> Vec<SceneElement> {
    canvas()
        .into_iter()
        .filter(|e| selected.contains(&e.base.id))
        .collect()
}

fn preview(id: Option<&str>) -> Node {
    Node::Element(Element::svg("svg").attr("data-library-item", id.unwrap_or("pending")))
}

fn previews_of(case: &Value) -> Previews {
    let mut previews = Previews::default();
    for id in strs(&case["previews"]) {
        if id == "pending" {
            previews.pending = Some(preview(None));
        } else {
            previews.items.insert(id.clone(), preview(Some(&id)));
        }
    }
    previews
}

/// The app state keys the sidebar reads, as a case starts.
fn app_of(case: &Value) -> Map<String, Value> {
    let mut app = Map::new();
    app.insert(
        "openSidebar".into(),
        match case["tab"].as_str() {
            Some(tab) => json!({"name": "default", "tab": tab}),
            None => Value::Null,
        },
    );
    app.insert(
        "defaultSidebarDockedPreference".into(),
        case["docked"].clone(),
    );
    app.insert(
        "selectedElementIds".into(),
        Value::Object(
            strs(&case["selected"])
                .into_iter()
                .map(|id| (id, json!(true)))
                .collect(),
        ),
    );
    app
}

fn open_sidebar(app: &Map<String, Value>) -> Option<OpenSidebar> {
    let o = app.get("openSidebar")?.as_object()?;
    Some(OpenSidebar {
        name: o["name"].as_str().unwrap().to_owned(),
        tab: o.get("tab").and_then(Value::as_str).map(str::to_owned),
    })
}

fn selected_ids(app: &Map<String, Value>) -> Vec<String> {
    app["selectedElementIds"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

struct World {
    case: Value,
    app: Map<String, Value>,
    items: Vec<LibraryItem>,
    state: LibraryMenuState,
}

impl World {
    fn new(case: &Value) -> World {
        World {
            case: case.clone(),
            app: app_of(case),
            items: items_of(case),
            state: LibraryMenuState {
                menu_open: case["menuOpen"].as_bool().unwrap(),
                ..LibraryMenuState::default()
            },
        }
    }

    fn status(&self) -> LibraryStatus {
        match self.case["status"].as_str().unwrap() {
            "loaded" => LibraryStatus::Loaded,
            _ => LibraryStatus::Loading {
                initialized: self.case["initialized"].as_bool().unwrap(),
            },
        }
    }

    fn with_context<R>(&self, f: impl FnOnce(LibraryContext<'_>) -> R) -> R {
        let pending = pending_of(&selected_ids(&self.app));
        f(LibraryContext {
            open_sidebar: open_sidebar(&self.app),
            docked_preference: self.app["defaultSidebarDockedPreference"]
                .as_bool()
                .unwrap(),
            can_fit_sidebar: self.case["canFitSidebar"].as_bool().unwrap(),
            phone: self.case["formFactor"] == "phone",
            status: self.status(),
            items: &self.items,
            pending: &pending,
        })
    }

    fn render(&self) -> Vec<Value> {
        let previews = previews_of(&self.case);
        let return_url = self.case["libraryReturnUrl"].as_str().map(str::to_owned);
        let theme = if self.case["theme"] == "dark" {
            Theme::Dark
        } else {
            Theme::Light
        };
        let mut out = Vec::new();
        self.with_context(|context| {
            if self.case["trigger"].as_bool().unwrap() {
                let open = context.open_sidebar.as_ref().map(|o| o.name.as_str())
                    == Some(DEFAULT_SIDEBAR_NAME);
                out.push(tree(&Node::Element(default_sidebar_trigger(
                    SidebarTriggerProps {
                        open,
                        theme,
                        on_event: None,
                    },
                ))));
            }
            let sidebar = default_sidebar(LibrarySidebarProps {
                context,
                theme,
                previews: &previews,
                state: &self.state,
                browse: BrowseLink {
                    app_id: "app-id".into(),
                    library_return_url: return_url,
                    location: "http://localhost/".into(),
                    window_name: String::new(),
                },
                id_prefix: "radix-1".into(),
                menu_ids: ("radix-2".into(), "radix-3".into()),
                search_menu: Some(Node::Element(
                    Element::new("template").attr("data-slot", "SearchMenu"),
                )),
                on_event: None,
            });
            if let Some(sidebar) = sidebar {
                out.push(tree(&Node::Element(sidebar)));
            }
        });
        out
    }

    fn docked(&self) -> bool {
        self.with_context(|cx| is_sidebar_docked(&cx))
    }
}

// -- the tree the fixture records ---------------------------------------------

fn tree(node: &Node) -> Value {
    match node {
        Node::Text(text) => Value::String(text.clone()),
        Node::Element(el) => {
            if el.tag() == "svg" {
                if let Some(id) = el.attribute("data-library-item") {
                    return json!({ "preview": if id == "pending" { Value::Null } else { json!(id) } });
                }
            }
            // floating-ui's placement from layout, which the fixture leaves
            // out (jsdom has none)
            let placed = el.attribute("data-radix-menu-content").is_some();
            let attrs: BTreeMap<_, _> = el
                .attributes()
                .iter()
                .filter(|(k, _)| !(placed && (k == "data-side" || k == "data-align")))
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            let style: BTreeMap<_, _> =
                if el.attribute("data-radix-popper-content-wrapper").is_some() {
                    BTreeMap::new()
                } else {
                    el.style_properties()
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                        .collect()
                };
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
fn expand(v: &Value, theme: Theme) -> Value {
    match v {
        Value::Object(o) if o.contains_key("icon") => {
            let name = o["icon"].as_str().unwrap();
            let icon = excali_ui::icons::icon(name).unwrap_or_else(|| panic!("no icon {name}"));
            tree(&Node::Element(icon.element(theme).unwrap()))
        }
        Value::Object(o) => {
            let mut out = Map::new();
            for (k, x) in o {
                if k == "children" {
                    let kids = x
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| expand(c, theme))
                        .collect();
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

fn expected_dom(dom: &Value, case: &Value) -> Vec<Value> {
    let theme = if case["theme"] == "dark" {
        Theme::Dark
    } else {
        Theme::Light
    };
    dom.as_array()
        .unwrap()
        .iter()
        .map(|n| expand(n, theme))
        .collect()
}

/// Where two trees first differ, as a path and both values.
fn first_difference(expected: &Value, actual: &Value, path: String) -> String {
    match (expected, actual) {
        (Value::Array(a), Value::Array(b)) => {
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                if x != y {
                    return first_difference(x, y, format!("{path}[{i}]"));
                }
            }
            format!("{path}: {} children expected, {} actual", a.len(), b.len())
        }
        (Value::Object(a), Value::Object(b)) if a.keys().eq(b.keys()) => {
            for (k, x) in a {
                if x != &b[k] {
                    return first_difference(x, &b[k], format!("{path}.{k}"));
                }
            }
            unreachable!()
        }
        _ => format!("{path}:\n  expected {expected}\n  actual   {actual}"),
    }
}

fn assert_dom(expected: &[Value], actual: &[Value], what: &str) {
    if expected != actual {
        panic!(
            "{what}: {}",
            first_difference(
                &Value::Array(expected.to_vec()),
                &Value::Array(actual.to_vec()),
                String::new()
            )
        );
    }
}

// -- DOM parity ---------------------------------------------------------------

#[test]
fn every_case_renders_upstreams_dom() {
    let cases = fixture()["cases"].as_array().unwrap();
    assert!(cases.len() >= 25, "{} cases", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let world = World::new(case);
        assert_dom(&expected_dom(&case["dom"], case), &world.render(), name);
        assert_eq!(
            world.docked(),
            case["sidebarDocked"].as_bool().unwrap(),
            "{name}: isSidebarDockedAtom"
        );
    }
}

#[test]
fn the_locale_strings_are_upstreams() {
    for (key, value) in fixture()["locale"].as_object().unwrap() {
        assert_eq!(library_text(key), value.as_str().unwrap(), "{key}");
    }
    assert_eq!(LIBRARY_URL, fixture()["libraryUrl"].as_str().unwrap());
}

/// Research §3.6: the Search and Library tab triggers, then dock and close;
/// the Personal and Excalidraw sections; the header menu's Load, Export and
/// Reset.
#[test]
fn the_order_is_the_research_pages() {
    let order = &fixture()["order"];
    assert_eq!(
        strs(&order["header"]),
        [
            "excalidraw-button sidebar-tab-trigger",
            "excalidraw-button sidebar-tab-trigger",
            "sidebar-dock",
            "sidebar-close"
        ]
    );
    assert_eq!(
        strs(&order["sections"]),
        ["Personal Library", "Excalidraw Library"]
    );
    assert_eq!(
        strs(&order["menu"]),
        ["Open", "Save to...", "Reset library"]
    );
    assert_eq!(LIBRARY_SIDEBAR_TAB, "library");
    assert_eq!(CANVAS_SEARCH_TAB, "search");
}

// -- interactions ---------------------------------------------------------------

/// A fixture tree node with its ancestors.
fn find_nth<'a>(roots: &'a [Value], selector: &str, index: usize) -> Vec<&'a Value> {
    let mut found: Vec<Vec<&Value>> = Vec::new();
    fn walk<'a>(n: &'a Value, path: &mut Vec<&'a Value>, sel: &str, out: &mut Vec<Vec<&'a Value>>) {
        if n.get("tag").is_none() {
            return;
        }
        path.push(n);
        if matches(path, sel) {
            out.push(path.clone());
        }
        for c in n["children"].as_array().unwrap() {
            walk(c, path, sel, out);
        }
        path.pop();
    }
    for r in roots {
        walk(r, &mut Vec::new(), selector, &mut found);
    }
    found
        .into_iter()
        .nth(index)
        .unwrap_or_else(|| panic!("no {selector}[{index}]"))
}

fn has_class(n: &Value, class: &str) -> bool {
    n["attrs"]["class"]
        .as_str()
        .is_some_and(|c| c.split_whitespace().any(|x| x == class))
}

fn simple(n: &Value, sel: &str) -> bool {
    if let Some(class) = sel.strip_prefix('.') {
        has_class(n, class)
    } else if let Some(attr) = sel.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let (k, v) = attr.split_once('=').unwrap();
        n["attrs"][k] == v
    } else {
        n["tag"] == sel
    }
}

/// The CSS selectors the fixture uses: simple ones and descendants.
fn matches(path: &[&Value], sel: &str) -> bool {
    let parts: Vec<&str> = sel.split_whitespace().collect();
    let (last, rest) = parts.split_last().unwrap();
    if !simple(path[path.len() - 1], last) {
        return false;
    }
    let mut i = path.len() - 1;
    for part in rest.iter().rev() {
        loop {
            if i == 0 {
                return false;
            }
            i -= 1;
            if simple(path[i], part) {
                break;
            }
        }
    }
    true
}

fn preview_in(n: &Value) -> Option<UnitKey> {
    if let Some(p) = n.get("preview") {
        return Some(match p.as_str() {
            Some(id) => UnitKey::Item(id.to_owned()),
            None => UnitKey::Pending,
        });
    }
    n.get("children")?.as_array()?.iter().find_map(preview_in)
}

/// The unit a node belongs to: the preview in its `.library-unit`.
fn unit_of(path: &[&Value]) -> UnitKey {
    let unit = path
        .iter()
        .rev()
        .find(|n| has_class(n, "library-unit"))
        .expect("inside a unit");
    preview_in(unit).expect("a unit with a preview")
}

/// The event excali-ui's handler on the step's target reports, read from
/// the DOM before the step (`every_target_listens_for_its_event` holds the
/// handlers to the DOM events).
fn event_of(step: &Value, before: &[Value], world: &World) -> LibrarySidebarEvent {
    let sel = step["selector"].as_str();
    let index = step["index"].as_u64().unwrap() as usize;
    let event = step["event"].as_str().unwrap();
    let Some(sel) = sel else {
        // a key on the document, outside the sidebar
        return LibrarySidebarEvent::KeyDown {
            key: step["key"].as_str().unwrap().to_owned(),
            target: KeyTarget::Outside {
                cursor_over_sidebar: false,
            },
            dialog_open: false,
        };
    };
    let path = find_nth(before, sel, index);
    let node = path[path.len() - 1];
    let shift = step["shiftKey"].as_bool().unwrap_or(false);
    let e = match (event, sel) {
        ("mousedown", ".sidebar-tab-trigger") => {
            let id = node["attrs"]["id"].as_str().unwrap();
            LibrarySidebarEvent::SelectTab(id.rsplit("-trigger-").next().unwrap().to_owned())
        }
        ("click", ".sidebar__close") => LibrarySidebarEvent::Close,
        ("click", ".sidebar__dock") => LibrarySidebarEvent::Dock(!world.docked()),
        ("click", ".sidebar-trigger__label-element") => LibrarySidebarEvent::ToggleTrigger,
        ("click", ".library-unit__dragger") => LibrarySidebarEvent::UnitClick {
            unit: unit_of(&path),
            shift,
        },
        ("click", ".library-unit__checkbox") => LibrarySidebarEvent::CheckboxToggle {
            id: match unit_of(&path) {
                UnitKey::Item(id) => id,
                UnitKey::Pending => panic!("a pending unit has no checkbox"),
            },
            shift,
        },
        ("mouseenter", ".library-unit") => LibrarySidebarEvent::UnitHover {
            unit: unit_of(&path),
            hovered: true,
        },
        ("mouseleave", ".library-unit") => LibrarySidebarEvent::UnitHover {
            unit: unit_of(&path),
            hovered: false,
        },
        ("input", _) => {
            LibrarySidebarEvent::SearchInput(step["value"].as_str().unwrap().to_owned())
        }
        ("keydown", _) => LibrarySidebarEvent::KeyDown {
            key: step["key"].as_str().unwrap().to_owned(),
            target: KeyTarget::SearchInput {
                value: node["attrs"]["value"].as_str().unwrap().to_owned(),
            },
            dialog_open: false,
        },
        ("click", ".library-menu-items__no-items button")
        | ("click", ".library-menu-items-container__header__hint") => {
            LibrarySidebarEvent::ClearSearch
        }
        ("click", ".dropdown-menu-button") => LibrarySidebarEvent::MenuToggle,
        ("click", sel) if sel.starts_with("[data-testid=lib-dropdown--") => {
            let action = match sel {
                "[data-testid=lib-dropdown--load]" => LibraryMenuAction::Load,
                "[data-testid=lib-dropdown--export]" => LibraryMenuAction::Export,
                other => panic!("menu item {other}"),
            };
            LibrarySidebarEvent::MenuSelect(action)
        }
        other => panic!("step {other:?}"),
    };
    e
}

/// The fixture's effects, in excali-ui's terms (atoms are state).
fn expected_effects(step: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let mut distribute: Option<Vec<String>> = None;
    for e in step["effects"].as_array().unwrap() {
        let (k, v) = e.as_object().unwrap().iter().next().unwrap();
        match k.as_str() {
            "atom" | "dataTransfer" => {}
            "distribute" => distribute = Some(strs(v)),
            "insert" => {
                let ids = distribute.take().expect("distributed before inserting");
                out.push(json!({"insert": ids}));
            }
            "setLibrary" => out.push(json!({"setLibrary": v})),
            _ => out.push(e.clone()),
        }
    }
    out
}

fn effect_json(e: &LibraryEffect) -> Value {
    match e {
        LibraryEffect::SetAppState(patch) => json!({"setAppState": patch}),
        LibraryEffect::TrackEvent(c, a, l) => json!({"trackEvent": [c, a, l]}),
        LibraryEffect::FocusContainer => json!({"focusContainer": true}),
        LibraryEffect::Insert(ids) => json!({"insert": ids}),
        LibraryEffect::SetLibrary(items) => json!({"setLibrary": items.iter().map(|i| json!({
            "id": i.id,
            "status": i.status.as_str(),
            // an integral time, as JSON.stringify writes it
            "created": if i.created.fract() == 0.0 { json!(i.created as i64) } else { json!(i.created) },
            "elements": i.elements.iter().map(|e| e.base.id.clone()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>()}),
        LibraryEffect::LoadLibrary => {
            json!({"updateLibrary": {"libraryItems": "fileOpen", "merge": true, "openLibraryMenu": true}})
        }
        LibraryEffect::ExportLibrary(ids) => json!({"exportLibrary": ids}),
        other => json!({"other": format!("{other:?}")}),
    }
}

/// Effects in a comparable order: the export's `getLatestLibrary` resolves
/// after the menu closes upstream; no other effects interleave with it.
fn normalized(mut v: Vec<Value>) -> Vec<Value> {
    v.sort_by_key(|e| e.to_string());
    v
}

#[test]
fn every_interaction_replays_as_upstream() {
    let interactions = fixture()["interactions"].as_array().unwrap();
    assert!(interactions.len() >= 25, "{}", interactions.len());
    let mut env = excali_core::restore::TestEnv::default();
    for interaction in interactions {
        let name = interaction["name"].as_str().unwrap();
        let mut world = World::new(interaction);
        let mut before = world.render();
        for (i, step) in interaction["steps"].as_array().unwrap().iter().enumerate() {
            let what = format!("{name} step {i} ({} {})", step["event"], step["selector"]);
            // the target listens for the event
            if step["event"] == "dragstart" {
                let unit = unit_of(&find_nth(
                    &before,
                    step["selector"].as_str().unwrap(),
                    step["index"].as_u64().unwrap() as usize,
                ));
                let got = drag_data(&unit, &world.state.selected_items);
                let want: Vec<&Value> = step["effects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|e| e.get("dataTransfer"))
                    .collect();
                match got {
                    DragStart::Prevent => {
                        assert!(want.is_empty(), "{what}");
                        assert!(step["defaultPrevented"].as_bool().unwrap(), "{what}");
                    }
                    DragStart::Data { mime, data } => {
                        assert_eq!(want, [&json!([mime, data])], "{what}");
                    }
                }
                // the handler leaves the unit un-hovered
                let pending = pending_of(&selected_ids(&world.app));
                let cx = LibraryContext {
                    open_sidebar: open_sidebar(&world.app),
                    docked_preference: world.app["defaultSidebarDockedPreference"]
                        .as_bool()
                        .unwrap(),
                    can_fit_sidebar: world.case["canFitSidebar"].as_bool().unwrap(),
                    phone: world.case["formFactor"] == "phone",
                    status: world.status(),
                    items: &world.items,
                    pending: &pending,
                };
                let out = update(
                    &mut world.state,
                    LibrarySidebarEvent::UnitHover {
                        unit,
                        hovered: false,
                    },
                    &cx,
                    &mut env,
                );
                assert!(out.effects.is_empty(), "{what}");
            } else {
                let event = event_of(step, &before, &world);
                let out = {
                    let pending = pending_of(&selected_ids(&world.app));
                    let cx = LibraryContext {
                        open_sidebar: open_sidebar(&world.app),
                        docked_preference: world.app["defaultSidebarDockedPreference"]
                            .as_bool()
                            .unwrap(),
                        can_fit_sidebar: world.case["canFitSidebar"].as_bool().unwrap(),
                        phone: world.case["formFactor"] == "phone",
                        status: world.status(),
                        items: &world.items,
                        pending: &pending,
                    };
                    update(&mut world.state, event, &cx, &mut env)
                };
                let got: Vec<Value> = out.effects.iter().map(effect_json).collect();
                // ids of new library items are upstream's randomId, a
                // counter shared with the duplicates before: compare shape
                let got = normalized(renumber(got));
                let want = normalized(renumber(expected_effects(step)));
                assert_eq!(got, want, "{what}: effects");
                assert_eq!(
                    out.prevent_default,
                    step["defaultPrevented"].as_bool().unwrap(),
                    "{what}: preventDefault"
                );
                for effect in &out.effects {
                    if let LibraryEffect::SetAppState(patch) = effect {
                        for (k, v) in patch {
                            world.app.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            let after = world.render();
            assert_dom(&expected_dom(&step["dom"], &world.case), &after, &what);
            assert_eq!(
                world.docked(),
                step["sidebarDocked"].as_bool().unwrap(),
                "{what}: isSidebarDockedAtom"
            );
            before = after;
        }
    }
}

/// New items' ids (`randomId`) as `new`.
fn renumber(v: Vec<Value>) -> Vec<Value> {
    v.into_iter()
        .map(|mut e| {
            if let Some(items) = e.get_mut("setLibrary").and_then(Value::as_array_mut) {
                for item in items {
                    let id = item["id"].as_str().unwrap().to_owned();
                    if id.starts_with("id") && id[2..].chars().all(|c| c.is_ascii_digit()) {
                        item["id"] = json!("new");
                    }
                }
            }
            e
        })
        .collect()
}

/// Every interactive element listens for the DOM event whose handler the
/// fixture ran.
#[test]
fn every_target_listens_for_its_event() {
    for case in ["both", "trigger-open", "pending-with-items", "menu-open"] {
        let case = fixture()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == case)
            .unwrap();
        let world = World::new(case);
        let previews = previews_of(case);
        world.with_context(|context| {
            let sidebar = default_sidebar(LibrarySidebarProps {
                context,
                theme: Theme::Light,
                previews: &previews,
                state: &world.state,
                browse: BrowseLink {
                    app_id: "app-id".into(),
                    library_return_url: None,
                    location: "http://localhost/".into(),
                    window_name: String::new(),
                },
                id_prefix: "radix-1".into(),
                menu_ids: ("radix-2".into(), "radix-3".into()),
                search_menu: None,
                on_event: None,
            })
            .unwrap();
            let mut listening: HashMap<String, Vec<String>> = HashMap::new();
            fn walk(e: &Element, out: &mut HashMap<String, Vec<String>>) {
                if let Some(class) = e.attribute("class") {
                    for c in class.split_whitespace() {
                        out.entry(c.to_owned())
                            .or_default()
                            .extend(e.listened_events().map(str::to_owned));
                    }
                }
                if let Some(t) = e.attribute("data-testid") {
                    out.entry(format!("[data-testid={t}]"))
                        .or_default()
                        .extend(e.listened_events().map(str::to_owned));
                }
                for c in e.children() {
                    if let Node::Element(c) = c {
                        walk(c, out);
                    }
                }
            }
            walk(&sidebar, &mut listening);
            let listens = |key: &str, ev: &str| {
                listening
                    .get(key)
                    .is_some_and(|evs| evs.iter().any(|e| e == ev))
            };
            assert!(listens("sidebar-tab-trigger", "mousedown"));
            assert!(listens("sidebar__close", "click"));
            assert!(listens("library-unit__dragger", "click"));
            assert!(listens("library-unit__dragger", "dragstart"));
            assert!(listens("library-unit", "mouseenter"));
            assert!(listens("library-unit", "mouseleave"));
            assert!(listens("dropdown-menu-button", "click"));
            if world.case["name"] == "both" || world.case["name"] == "trigger-open" {
                assert!(listens("sidebar__dock", "click"));
            }
            if world.case["name"] == "menu-open" {
                assert!(listens("[data-testid=lib-dropdown--load]", "click"));
                assert!(listens("[data-testid=lib-dropdown--export]", "click"));
            }
        });
    }
    let trigger = default_sidebar_trigger(SidebarTriggerProps {
        open: false,
        theme: Theme::Light,
        on_event: None,
    });
    assert!(trigger.listened_events().any(|e| e == "click"));
}

// -- helpers ------------------------------------------------------------------

/// `LibraryDropdownMenuButton`'s items (`LibraryMenuHeaderContent.tsx:
/// 206-238`): Load without a selection, Export with items, Publish with a
/// selection, then Reset (Remove with a selection) with items.
#[test]
fn the_header_menu_offers_upstreams_items() {
    use LibraryMenuAction::*;
    assert_eq!(library_menu_actions(0, 0), [Load]);
    assert_eq!(library_menu_actions(0, 3), [Load, Export, Reset]);
    assert_eq!(library_menu_actions(2, 5), [Export, Publish, Remove]);
}

/// The search filter (`LibraryMenuItems.tsx:86-98`): deburred, lower-cased,
/// trimmed query in the deburred, lower-cased names; unnamed and
/// blank-named items never match.
#[test]
fn search_filters_names_as_upstream() {
    let items = all_items();
    let ids = |q: &str| -> Vec<String> {
        filter_library_items(&items, q)
            .into_iter()
            .map(|i| i.id.clone())
            .collect()
    };
    assert_eq!(ids("caf"), ["u3", "p2"]);
    assert_eq!(ids("  CAFÉ "), ["u3", "p2"]);
    assert_eq!(ids("box"), ["u1"]);
    assert!(ids("zzz").is_empty());
    assert!(ids("   ").is_empty());
    assert!(ids("").is_empty());
}

/// LayerUI: the UI makes room for the sidebar only when it is open, docked
/// and fits (`LayerUI.tsx:462-466`); the sidebar docks when the search tab
/// forces it or by the preference (`DefaultSidebar.tsx:77-83`).
#[test]
fn docking_as_layer_ui_reads_it() {
    let items = all_items();
    let cx = |tab: Option<&str>, pref: bool, fits: bool| LibraryContext {
        open_sidebar: tab.map(|t| OpenSidebar {
            name: "default".into(),
            tab: Some(t.into()),
        }),
        docked_preference: pref,
        can_fit_sidebar: fits,
        phone: false,
        status: LibraryStatus::Loaded,
        items: &items,
        pending: &[],
    };
    assert!(is_sidebar_docked_and_fits(&cx(Some("library"), true, true)));
    assert!(!is_sidebar_docked_and_fits(&cx(
        Some("library"),
        true,
        false
    )));
    assert!(!is_sidebar_docked_and_fits(&cx(
        Some("library"),
        false,
        true
    )));
    assert!(is_sidebar_docked_and_fits(&cx(Some("search"), false, true)));
    assert!(!is_sidebar_docked_and_fits(&cx(None, true, true)));
}

/// Adding a selection with an image to the library is refused
/// (`LibraryMenu.tsx:79-86`, `LIBRARY_DISABLED_TYPES`).
#[test]
fn disabled_types_are_not_added_to_the_library() {
    let items = all_items();
    let mut image = canvas().remove(0);
    let mut map = image.to_map();
    map.insert("type".into(), json!("image"));
    map.insert("fileId".into(), Value::Null);
    map.insert("status".into(), json!("pending"));
    map.insert("scale".into(), json!([1, 1]));
    map.insert("crop".into(), Value::Null);
    image = SceneElement::from_map(map).unwrap();
    let pending = vec![image];
    let cx = LibraryContext {
        open_sidebar: Some(OpenSidebar {
            name: "default".into(),
            tab: Some("library".into()),
        }),
        docked_preference: false,
        can_fit_sidebar: true,
        phone: false,
        status: LibraryStatus::Loaded,
        items: &items,
        pending: &pending,
    };
    let mut state = LibraryMenuState::default();
    let out = update(
        &mut state,
        LibrarySidebarEvent::UnitClick {
            unit: UnitKey::Pending,
            shift: false,
        },
        &cx,
        &mut excali_core::restore::TestEnv::default(),
    );
    let got: Vec<Value> = out.effects.iter().map(effect_json).collect();
    assert_eq!(
        got,
        [
            json!({"trackEvent": ["element", "addToLibrary", "ui"]}),
            json!({"setAppState": {"errorMessage": library_text("errors.libraryElementTypeError.image")}}),
        ]
    );
}

/// The sidebar is 302 px wide docked (`RIGHT_SIDEBAR_WIDTH`) less the
/// island's outer margin, per its stylesheet; the stylesheet is the
/// sidebar's and the library's rules.
#[test]
fn the_stylesheet_is_the_sidebars() {
    assert!(LIBRARY_SIDEBAR_CSS.starts_with("/* Generated by tools/goldens/library-sidebar.mjs"));
    assert_eq!(excali_ui::theme::RIGHT_SIDEBAR_WIDTH, 302.0);
    for rule in [
        "width: calc(var(--right-sidebar-width) - var(--space-factor) * 2);",
        ".excalidraw .sidebar--docked {",
        ".excalidraw .sidebar__header {",
        ".excalidraw .sidebar-trigger {",
        ".excalidraw .layer-ui__library {",
        ".excalidraw .library-menu-items-container__grid {",
        ".excalidraw .library-unit {",
        ".excalidraw .Checkbox",
        ".excalidraw .Spinner {",
    ] {
        assert!(LIBRARY_SIDEBAR_CSS.contains(rule), "{rule}");
    }
}

/// `deburr` (`packages/excalidraw/deburr.ts`) on every code point of its
/// ranges, the combining marks' range ends and a few names.
#[test]
fn deburr_is_upstreams() {
    let cases = fixture()["deburr"].as_array().unwrap();
    assert!(cases.len() > 190);
    for case in cases {
        let input = case[0].as_str().unwrap();
        assert_eq!(
            excali_ui::library_sidebar::deburr(input),
            case[1].as_str().unwrap(),
            "{input:?}"
        );
    }
}

/// Escape with the header menu open (`DropdownMenuContent.tsx:62-85`, a
/// document capture listener bound after LibraryMenu's): the menu closes,
/// the event stops there (an undocked sidebar stays open), and a selection
/// is cleared first by LibraryMenu's listener.
#[test]
fn escape_closes_the_open_header_menu_first() {
    let items = all_items();
    let cx = LibraryContext {
        open_sidebar: Some(OpenSidebar {
            name: "default".into(),
            tab: Some("library".into()),
        }),
        docked_preference: false,
        can_fit_sidebar: true,
        phone: false,
        status: LibraryStatus::Loaded,
        items: &items,
        pending: &[],
    };
    let escape = || LibrarySidebarEvent::KeyDown {
        key: "Escape".into(),
        target: KeyTarget::InSidebar { empty_input: false },
        dialog_open: false,
    };
    let mut env = excali_core::restore::TestEnv::default();
    let mut state = LibraryMenuState {
        menu_open: true,
        ..LibraryMenuState::default()
    };
    let out = update(&mut state, escape(), &cx, &mut env);
    assert!(out.effects.is_empty());
    assert!(out.prevent_default && out.stop_propagation);
    assert!(!state.menu_open);

    let mut state = LibraryMenuState {
        menu_open: true,
        selected_items: vec!["u1".into()],
        ..LibraryMenuState::default()
    };
    let out = update(&mut state, escape(), &cx, &mut env);
    assert!(out.effects.is_empty());
    assert!(out.prevent_default && out.stop_propagation);
    assert!(!state.menu_open && state.selected_items.is_empty());

    // closed, the same key closes the undocked sidebar (Sidebar's listener)
    let out = update(&mut state, escape(), &cx, &mut env);
    assert!(!out.stop_propagation);
    assert_eq!(
        out.effects.iter().map(effect_json).collect::<Vec<_>>(),
        [json!({"setAppState": {"openSidebar": null}})]
    );
}
