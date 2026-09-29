//! ex-526 harness: excali-ui's library sidebar in the browser.
//!
//! [`start`] makes an editor container (`.excalidraw` with the container's
//! tokens) filling its target, installs the sidebar's stylesheets, and
//! mounts LayerUI's library trigger and the default sidebar for a library
//! and an app state given as JSON. What the user does comes back as
//! events, which run through [`excali_ui::library_sidebar::update`]; the
//! app state keys it patches and the library it stores are applied, its
//! effects logged ([`take_effects`]), and the sidebar rendered again.
//! `scripts/web/library-sidebar.sh` builds it.

use std::cell::RefCell;
use std::rc::Rc;

use excali_core::element::Element as SceneElement;
use excali_core::library::{restore_library_items, LibraryItem, LibraryItemStatus};
use excali_core::restore::TestEnv;
use excali_scene::shape::Theme;
use excali_ui::dom::{mount, Element, Mounted, Node};
use excali_ui::library_sidebar::{
    default_sidebar, default_sidebar_trigger, install_stylesheet, is_sidebar_docked_and_fits,
    update, BrowseLink, LibraryContext, LibraryEffect, LibraryMenuState, LibrarySidebarEvent,
    LibrarySidebarProps, LibraryStatus, OpenSidebar, Previews, SidebarTriggerProps,
};
use serde_json::{json, Map, Value};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

struct Harness {
    container: HtmlElement,
    app: Map<String, Value>,
    can_fit_sidebar: bool,
    phone: bool,
    theme: Theme,
    items: Vec<LibraryItem>,
    pending: Vec<SceneElement>,
    state: LibraryMenuState,
    env: TestEnv,
    effects: Vec<Value>,
    mounted: Vec<Mounted>,
}

thread_local! {
    static HARNESS: RefCell<Option<Harness>> = const { RefCell::new(None) };
}

fn open_sidebar(app: &Map<String, Value>) -> Option<OpenSidebar> {
    let o = app.get("openSidebar")?.as_object()?;
    Some(OpenSidebar {
        name: o.get("name")?.as_str()?.to_owned(),
        tab: o.get("tab").and_then(Value::as_str).map(str::to_owned),
    })
}

fn context(h: &Harness) -> LibraryContext<'_> {
    LibraryContext {
        open_sidebar: open_sidebar(&h.app),
        docked_preference: h.app.get("defaultSidebarDockedPreference") == Some(&json!(true)),
        can_fit_sidebar: h.can_fit_sidebar,
        phone: h.phone,
        status: LibraryStatus::Loaded,
        items: &h.items,
        pending: &h.pending,
    }
}

/// A preview as the host draws one: a square the item's size.
fn preview(id: &str) -> Node {
    Node::Element(
        Element::svg("svg")
            .attr("data-library-item", id)
            .attr("viewBox", "0 0 10 10")
            .child(
                Element::svg("rect")
                    .attr("x", "1")
                    .attr("y", "1")
                    .attr("width", "8")
                    .attr("height", "8")
                    .attr("fill", "none")
                    .attr("stroke", "#1e1e1e"),
            ),
    )
}

fn effect_json(e: &LibraryEffect) -> Value {
    match e {
        LibraryEffect::SetAppState(patch) => json!({ "setAppState": patch }),
        LibraryEffect::TrackEvent(c, a, l) => json!({ "trackEvent": [c, a, l] }),
        LibraryEffect::FocusContainer => json!({ "focusContainer": true }),
        LibraryEffect::Insert(ids) => json!({ "insert": ids }),
        LibraryEffect::SetLibrary(items) => {
            json!({ "setLibrary": items.iter().map(|i| i.id.clone()).collect::<Vec<_>>() })
        }
        LibraryEffect::ExportLibrary(ids) => json!({ "exportLibrary": ids }),
        LibraryEffect::LoadLibrary => json!({ "loadLibrary": true }),
        other => json!({ "other": format!("{other:?}") }),
    }
}

fn on_event() -> Rc<dyn Fn(LibrarySidebarEvent)> {
    Rc::new(|event| {
        let changed = HARNESS.with(|cell| {
            let mut guard = cell.borrow_mut();
            let Some(h) = guard.as_mut() else {
                return false;
            };
            let before = (h.state.clone(), h.app.clone(), h.items.len());
            let cx = LibraryContext {
                open_sidebar: open_sidebar(&h.app),
                docked_preference: h.app.get("defaultSidebarDockedPreference")
                    == Some(&json!(true)),
                can_fit_sidebar: h.can_fit_sidebar,
                phone: h.phone,
                status: LibraryStatus::Loaded,
                items: &h.items,
                pending: &h.pending,
            };
            let out = update(&mut h.state, event, &cx, &mut h.env);
            for effect in &out.effects {
                h.effects.push(effect_json(effect));
                match effect {
                    LibraryEffect::SetAppState(patch) => {
                        for (k, v) in patch {
                            h.app.insert(k.clone(), v.clone());
                        }
                    }
                    LibraryEffect::SetLibrary(items) => h.items = items.clone(),
                    _ => {}
                }
            }
            // a selection added to the library is deselected
            if h.app.get("selectedElementIds") == Some(&json!({})) {
                h.pending.clear();
            }
            before != (h.state.clone(), h.app.clone(), h.items.len())
        });
        // render once the handler has returned (its listener goes with the
        // old tree), and only for a change: a tree built again under the
        // pointer would be entered again
        if changed {
            schedule_render();
        }
    })
}

fn schedule_render() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let callback = Closure::once_into_js(|| {
        let _ = render();
    });
    let _ = window.set_timeout_with_callback(callback.unchecked_ref());
}

fn render() -> Result<(), JsValue> {
    HARNESS.with(|cell| {
        let mut guard = cell.borrow_mut();
        let h = guard.as_mut().ok_or_else(|| js_err("not started"))?;
        h.mounted.drain(..).for_each(Mounted::remove);
        let document = h
            .container
            .owner_document()
            .ok_or_else(|| js_err("no document"))?;
        let previews = Previews {
            items: h
                .items
                .iter()
                .map(|i| (i.id.clone(), preview(&i.id)))
                .collect(),
            pending: Some(preview("pending")),
        };
        let cx = context(h);
        let fits = is_sidebar_docked_and_fits(&cx);
        let open = cx.open_sidebar.as_ref().map(|o| o.name.as_str()) == Some("default");
        let mut nodes = Vec::new();
        // LayerUI hides the trigger while the default sidebar is docked
        if !fits {
            nodes.push(Node::Element(default_sidebar_trigger(
                SidebarTriggerProps {
                    open,
                    theme: h.theme,
                    on_event: Some(on_event()),
                },
            )));
        }
        let sidebar = default_sidebar(LibrarySidebarProps {
            context: cx,
            theme: h.theme,
            previews: &previews,
            state: &h.state,
            browse: BrowseLink {
                app_id: "harness".into(),
                library_return_url: None,
                location: "http://localhost/".into(),
                window_name: String::new(),
            },
            id_prefix: "radix-1".into(),
            menu_ids: ("radix-2".into(), "radix-3".into()),
            search_menu: Some(Node::Element(
                Element::new("div").attr("class", "search-menu-slot"),
            )),
            on_event: Some(on_event()),
        });
        nodes.extend(sidebar.map(Node::Element));
        let mut mounted = Vec::new();
        for node in &nodes {
            mounted.push(mount(node, &document, &h.container)?);
        }
        h.mounted = mounted;
        h.container
            .set_attribute("data-docked-and-fits", if fits { "true" } else { "false" })?;
        Ok(())
    })
}

/// Starts the harness in `target` with `config`: `{ items, pending, tab,
/// docked, canFitSidebar, phone, theme }` (the items as a library file's
/// `libraryItems`, the pending elements as a scene's).
#[wasm_bindgen]
pub fn start(target: &web_sys::Element, config: &str) -> Result<(), JsValue> {
    let config: Value = serde_json::from_str(config).map_err(js_err)?;
    let document = target
        .owner_document()
        .ok_or_else(|| js_err("the target has no document"))?;
    install_stylesheet(&document)?;
    let container: HtmlElement = document.create_element("div")?.unchecked_into();
    container.set_class_name("excalidraw");
    excali_ui::theme::apply_container_tokens(&container)?;
    let theme = if config["theme"] == "dark" {
        Theme::Dark
    } else {
        Theme::Light
    };
    excali_ui::theme::apply_theme(&container, theme)?;
    target.replace_children_with_node_1(&container);
    let mut env = TestEnv::default();
    let items = restore_library_items(
        Some(&config["items"]),
        LibraryItemStatus::Unpublished,
        &mut env,
    )
    .map_err(|e| js_err(format!("{e:?}")))?;
    let pending = config["pending"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|e| SceneElement::from_map(e.as_object()?.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let mut app = Map::new();
    app.insert(
        "openSidebar".into(),
        match config["tab"].as_str() {
            Some(tab) => json!({ "name": "default", "tab": tab }),
            None => Value::Null,
        },
    );
    app.insert(
        "defaultSidebarDockedPreference".into(),
        json!(config["docked"] == true),
    );
    HARNESS.with(|cell| {
        *cell.borrow_mut() = Some(Harness {
            container,
            app,
            can_fit_sidebar: config["canFitSidebar"] != false,
            phone: config["phone"] == true,
            theme,
            items,
            pending,
            state: LibraryMenuState::default(),
            env,
            effects: Vec::new(),
            mounted: Vec::new(),
        })
    });
    render()
}

/// The effects logged since the last call, as JSON.
#[wasm_bindgen]
pub fn take_effects() -> String {
    HARNESS.with(|cell| {
        let mut guard = cell.borrow_mut();
        let effects = guard
            .as_mut()
            .map(|h| std::mem::take(&mut h.effects))
            .unwrap_or_default();
        Value::Array(effects).to_string()
    })
}

/// The menu's state and the app state keys, as JSON.
#[wasm_bindgen]
pub fn snapshot() -> String {
    HARNESS.with(|cell| {
        let guard = cell.borrow();
        let Some(h) = guard.as_ref() else {
            return "null".into();
        };
        json!({
            "app": h.app,
            "selected": h.state.selected_items,
            "search": h.state.search,
            "menuOpen": h.state.menu_open,
            "items": h.items.iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
        })
        .to_string()
    })
}
