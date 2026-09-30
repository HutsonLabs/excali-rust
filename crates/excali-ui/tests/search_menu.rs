//! The search menu (ex-708): upstream's `SearchMenu`
//! (`components/SearchMenu.tsx`), the default sidebar's search tab, and
//! `actionToggleSearchMenu` (Ctrl/Cmd+F).
//!
//! Fixture: `tests/fixtures/search-menu.json`, written by
//! `tools/goldens/search-menu.mjs` from upstream at the pinned commit
//! (React 19.0.0, jsdom 22.1.0): the module's search helpers on their own,
//! the toggle action's results, and every interaction replayed step by step
//! through [`mount`], [`run_pending`], [`update`], [`key_down`] and
//! [`unmount`], the app state patches and viewport navigations compared with
//! what upstream's effects did, and the DOM after each step with upstream's.
//! `src/search_menu/search_menu.css` is the same generator's stylesheet.

use std::collections::BTreeMap;

use excali_core::element::Element as SceneElement;
use excali_editor::viewport::{Offsets, ViewportState};
use excali_scene::shape::Theme;
use excali_text::text_measurements::TextMetricsProvider;
use excali_ui::dom::{Element, Node};
use excali_ui::search_menu::{
    focus_search_matches, get_match_in_frame, get_match_preview, get_matched_lines, handle_search,
    key_down, mount, normalize_wrapped_text, run_pending, search_menu, search_text,
    toggle_search_menu, unmount, update, InputFocus, MatchKind, SearchContext, SearchEffect,
    SearchKey, SearchMenuEvent, SearchMenuProps, SearchMenuState, SearchToggle, SEARCH_MENU_CSS,
};
use serde_json::{json, Map, Value};

fn fixture() -> &'static Value {
    use std::sync::OnceLock;
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| serde_json::from_str(include_str!("fixtures/search-menu.json")).unwrap())
}

/// `METRICS.scaled` of tools/goldens/text-element-sizing.mjs: each UTF-16
/// code unit u is 3 + (u * 7) % 11 wide, 0.5 less for every adjacent pair
/// whose sum is a multiple of 5, the sum scaled by the font size over 20.
struct Scaled;

impl TextMetricsProvider for Scaled {
    fn get_line_width(&self, text: &str, font: &str) -> f64 {
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut width = 0.0;
        for (i, &u) in units.iter().enumerate() {
            width += f64::from(3 + (u32::from(u) * 7) % 11);
            if i > 0 && (u32::from(units[i - 1]) + u32::from(u)) % 5 == 0 {
                width -= 0.5;
            }
        }
        let size: f64 = font
            .split("px")
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap();
        width * size / 20.0
    }
}

fn scene() -> Vec<SceneElement> {
    fixture()["scene"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| SceneElement::from_map(v.as_object().unwrap().clone()).unwrap())
        .collect()
}

fn element(id: &str) -> SceneElement {
    scene().into_iter().find(|e| e.base.id == id).unwrap()
}

/// Every number as an f64, so 50 and 50.0 compare equal.
fn norm(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), norm(x))).collect()),
        _ => v.clone(),
    }
}

fn lines_json(lines: &[excali_ui::search_menu::MatchedLine]) -> Value {
    norm(&Value::Array(lines.iter().map(|l| l.to_json()).collect()))
}

// -- units --------------------------------------------------------------------

#[test]
fn match_previews_are_upstreams() {
    for case in fixture()["units"]["preview"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let index = case["index"].as_u64().unwrap() as usize;
        let query = case["query"].as_str().unwrap();
        let p = get_match_preview(text, index, query);
        let actual = json!({
            "indexInSearchQuery": p.index_in_search_query,
            "previewText": p.preview_text,
            "moreBefore": p.more_before,
            "moreAfter": p.more_after,
        });
        assert_eq!(
            norm(&actual),
            norm(&case["result"]),
            "{text:?} at {index} for {query:?}"
        );
    }
}

#[test]
fn wrapped_text_normalizes_as_upstream() {
    for case in fixture()["units"]["normalize"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let original = case["originalText"].as_str().unwrap();
        assert_eq!(
            normalize_wrapped_text(text, original),
            case["result"].as_str().unwrap(),
            "{text:?} / {original:?}"
        );
    }
}

#[test]
fn matched_lines_are_upstreams() {
    for case in fixture()["units"]["matchedLines"].as_array().unwrap() {
        let el = element(case["id"].as_str().unwrap());
        let query = case["query"].as_str().unwrap();
        let index = case["index"].as_u64().unwrap() as usize;
        let lines = get_matched_lines(&el, query, index, &Scaled);
        assert_eq!(lines_json(&lines), norm(&case["result"]), "{case}");
    }
}

#[test]
fn frame_name_matches_are_upstreams() {
    for case in fixture()["units"]["frame"].as_array().unwrap() {
        let el = element(case["id"].as_str().unwrap());
        let query = case["query"].as_str().unwrap();
        let index = case["index"].as_u64().unwrap() as usize;
        let zoom = case["zoom"].as_f64().unwrap();
        let lines = get_match_in_frame(&el, query, index, zoom, &Scaled);
        assert_eq!(lines_json(&lines), norm(&case["result"]), "{case}");
    }
}

// -- the toggle ----------------------------------------------------------------

#[test]
fn ctrl_f_toggles_as_upstreams_action() {
    for case in fixture()["toggle"].as_array().unwrap() {
        let mut app = Map::new();
        app.insert("openSidebar".into(), case["openSidebar"].clone());
        app.insert("openDialog".into(), case["openDialog"].clone());
        let out = toggle_search_menu(&app);
        let focused = case["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == "focus");
        match (&case["result"], out) {
            (Value::Bool(false), SearchToggle::FocusInput) => assert!(focused, "{case}"),
            (Value::Bool(false), SearchToggle::None) => assert!(!focused, "{case}"),
            (Value::Object(r), SearchToggle::Open(patch)) => {
                assert_eq!(Value::Object(patch), r["appState"], "{case}");
                assert_eq!(r["captureUpdate"], "EVENTUALLY");
            }
            (expected, actual) => panic!("{}: expected {expected}, got {actual:?}", case["name"]),
        }
    }
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
            let children = el.children().iter().map(tree).collect();
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

fn assert_same(expected: &Value, actual: &Value, what: &str) {
    if expected != actual {
        panic!(
            "{what}: {}",
            first_difference(expected, actual, String::new())
        );
    }
}

/// The elements matching a class, depth first.
fn find_all<'a>(el: &'a Element, class: &str, out: &mut Vec<&'a Element>) {
    if el
        .attribute("class")
        .is_some_and(|c| c.split(' ').any(|x| x == class))
    {
        out.push(el);
    }
    for c in el.children() {
        if let Node::Element(e) = c {
            find_all(e, class, out);
        }
    }
}

// -- interactions ---------------------------------------------------------------

/// The stand-in host: the app state keys the menu writes, the input's focus
/// and selection as the browser keeps them.
struct Host {
    app: Map<String, Value>,
    focused: bool,
    selection: (usize, usize),
}

impl Host {
    /// The effects as the fixture records them, applied to the app state.
    fn apply(&mut self, effects: Vec<SearchEffect>, state: &SearchMenuState) -> Vec<Value> {
        let mut out = Vec::new();
        for effect in effects {
            match effect {
                SearchEffect::SetAppState(patch) => {
                    for (k, v) in &patch {
                        self.app.insert(k.clone(), v.clone());
                    }
                    out.push(json!({ "setAppState": patch }));
                }
                SearchEffect::FocusMatches(index) => {
                    let current = self
                        .app
                        .get("searchMatches")
                        .cloned()
                        .unwrap_or(Value::Null);
                    if let Some(next) = focus_search_matches(&current, index) {
                        self.app.insert("searchMatches".into(), next.clone());
                        out.push(json!({ "setAppState": { "searchMatches": next } }));
                    }
                }
                SearchEffect::SetViewport { target, fit } => {
                    let fit = match fit {
                        excali_editor::viewport::Fit::ScaleDown => "scale-down",
                        excali_editor::viewport::Fit::Contain => "contain",
                        excali_editor::viewport::Fit::None => "none",
                    };
                    out.push(json!({
                        "setViewport": {
                            "target": target,
                            "fit": fit,
                            "animation": { "duration": 300 },
                            "offsets": { "ui": true },
                        }
                    }));
                }
                SearchEffect::FocusInput => {
                    self.focused = true;
                    self.selection = (0, state.input.encode_utf16().count());
                }
                SearchEffect::ScheduleSearch => {}
            }
        }
        out
    }
}

/// The fixture's effects without the ones the Rust side keeps elsewhere:
/// the atoms are [`SearchMenuState`]'s fields, and the highlighted item's
/// scrollIntoView is its mount hook (checked in the DOM).
fn expected_effects(step: &Value) -> Vec<Value> {
    step["effects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e.get("atom").is_none() && e.get("scrollIntoView").is_none())
        .cloned()
        .collect()
}

fn context<'a>(
    case: &'a Value,
    elements: &'a [SceneElement],
    visible: &'a [String],
) -> SearchContext<'a> {
    let n = |k: &str| case[k].as_f64().unwrap();
    SearchContext {
        elements,
        scene_nonce: 1,
        visible_ids: visible,
        view: ViewportState {
            scroll_x: n("scrollX"),
            scroll_y: n("scrollY"),
            zoom: n("zoom"),
            width: case["canvas"][0].as_f64().unwrap(),
            height: case["canvas"][1].as_f64().unwrap(),
            offset_left: n("offsetLeft"),
            offset_top: n("offsetTop"),
            scroll_constraints: None,
        },
        padding: Offsets::from_json(&case["offsets"]),
        metrics: &Scaled,
    }
}

#[test]
fn every_interaction_replays_as_upstream() {
    let elements = scene();
    for case in fixture()["interactions"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let visible: Vec<String> = case["visible"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let cx = context(case, &elements, &visible);
        let mut state = SearchMenuState {
            input: case["query"].as_str().unwrap().to_owned(),
            ..SearchMenuState::default()
        };
        let mut host = Host {
            app: Map::new(),
            focused: false,
            selection: (0, 0),
        };
        host.app.insert("searchMatches".into(), Value::Null);
        let open_dialog = !case["openDialog"].is_null();
        let open_popup = !case["openPopup"].is_null();
        let mut first = true;
        for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let what = format!("{name} step {i} ({})", step["event"]);
            let mut prevented = false;
            let before = search_menu(SearchMenuProps {
                state: &state,
                focus: InputFocus::None,
                on_event: None,
            });
            let effects = match step["event"].as_str().unwrap() {
                "mount" => {
                    host.focused = true;
                    host.selection = (0, state.input.encode_utf16().count());
                    mount(&mut state, 1)
                }
                "debounce" => {
                    assert_eq!(state.pending.is_some(), step["ran"] == true, "{what}");
                    run_pending(&mut state, &cx)
                }
                "input" => {
                    let value = step["value"].as_str().unwrap();
                    let n = value.encode_utf16().count();
                    host.selection = (n, n);
                    update(&mut state, SearchMenuEvent::Input(value.into()), &cx)
                }
                "click" => {
                    let selector = step["selector"].as_str().unwrap();
                    let index = step["index"].as_u64().unwrap() as usize;
                    let class = selector.trim_start_matches('.');
                    let mut found = Vec::new();
                    find_all(&before, class, &mut found);
                    let target = found
                        .get(index)
                        .unwrap_or_else(|| panic!("{what}: no {selector}"));
                    let event = match class {
                        "result-nav-btn" if index == 0 => Some(SearchMenuEvent::Next),
                        "result-nav-btn" => Some(SearchMenuEvent::Previous),
                        "layer-ui__result-item" => Some(SearchMenuEvent::Select(index)),
                        _ => None,
                    };
                    match event {
                        Some(event) => {
                            assert!(
                                target.listened_events().any(|e| e == "click"),
                                "{what}: {selector} does not listen for clicks"
                            );
                            update(&mut state, event, &cx)
                        }
                        None => Vec::new(),
                    }
                }
                "keydown" => {
                    let key = SearchKey {
                        key: step["key"].as_str().unwrap(),
                        ctrl_or_cmd: step["ctrlKey"] == true,
                        in_search: step["on"] == "input",
                        input_focused: host.focused,
                        open_dialog,
                        open_popup,
                    };
                    let out = key_down(&mut state, &key, &cx);
                    prevented = out.prevent_default;
                    out.effects
                }
                "blur" => {
                    host.focused = false;
                    Vec::new()
                }
                "unmount" => {
                    host.focused = false;
                    unmount(&mut state)
                }
                other => panic!("{what}: event {other}"),
            };
            let actual = host.apply(effects, &state);
            assert_same(
                &norm(&Value::Array(expected_effects(step))),
                &norm(&Value::Array(actual)),
                &format!("{what} effects"),
            );
            assert_eq!(
                prevented,
                step["defaultPrevented"] == true,
                "{what}: defaultPrevented"
            );
            assert_eq!(host.focused, step["focused"] == true, "{what}: focus");
            if host.focused {
                let sel = &step["selection"];
                assert_eq!(
                    (host.selection.0 as u64, host.selection.1 as u64),
                    (sel[0].as_u64().unwrap(), sel[1].as_u64().unwrap()),
                    "{what}: selection"
                );
            }
            if step["event"] == "unmount" {
                continue;
            }
            let focus = if first {
                InputFocus::Select
            } else {
                InputFocus::None
            };
            first = false;
            let dom = search_menu(SearchMenuProps {
                state: &state,
                focus,
                on_event: None,
            });
            let expected: Vec<Value> = step["dom"].as_array().unwrap().iter().map(expand).collect();
            assert_same(
                &Value::Array(expected),
                &Value::Array(vec![tree(&Node::Element(dom))]),
                &format!("{what} DOM"),
            );
        }
    }
}

#[test]
fn the_highlighted_result_scrolls_into_view() {
    let elements = scene();
    let visible = vec!["t1".to_owned()];
    let case = &fixture()["interactions"][0];
    let cx = context(case, &elements, &visible);
    let mut state = SearchMenuState::default();
    mount(&mut state, 1);
    run_pending(&mut state, &cx);
    update(&mut state, SearchMenuEvent::Input("hello".into()), &cx);
    run_pending(&mut state, &cx);
    let dom = search_menu(SearchMenuProps {
        state: &state,
        focus: InputFocus::None,
        on_event: None,
    });
    let mut items = Vec::new();
    find_all(&dom, "layer-ui__result-item", &mut items);
    let active: Vec<_> = items
        .iter()
        .filter(|e| e.attribute("class").unwrap().contains("active"))
        .collect();
    assert_eq!(active.len(), 1);
    assert!(
        active[0].has_mount_hook(),
        "the active item scrolls itself into view"
    );
    assert!(items.iter().filter(|e| e.has_mount_hook()).count() == 1);
}

#[test]
fn every_result_item_and_nav_button_listens_for_clicks() {
    let elements = scene();
    let visible = vec![];
    let case = &fixture()["interactions"][0];
    let cx = context(case, &elements, &visible);
    let mut state = SearchMenuState::default();
    update(&mut state, SearchMenuEvent::Input("o".into()), &cx);
    run_pending(&mut state, &cx);
    let dom = search_menu(SearchMenuProps {
        state: &state,
        focus: InputFocus::None,
        on_event: Some(std::rc::Rc::new(|_| {})),
    });
    for class in ["layer-ui__result-item", "result-nav-btn"] {
        let mut found = Vec::new();
        find_all(&dom, class, &mut found);
        assert!(!found.is_empty());
        for el in found {
            assert!(el.listened_events().any(|e| e == "click"), "{class}");
        }
    }
    let mut inputs = Vec::new();
    find_all(&dom, "layer-ui__search-inputWrapper", &mut inputs);
    assert_eq!(inputs.len(), 1);
}

// -- research 3.6 ------------------------------------------------------------

/// "Search menu shows Frames and Texts result groups"
/// (`site/content/research/ui-design-system.md` §3.6, `SearchMenu.tsx:
/// 340-519`): frames first, then texts, each under its title.
#[test]
fn results_group_frames_then_texts() {
    let elements = scene();
    let visible: Vec<String> = Vec::new();
    let case = &fixture()["interactions"][0];
    let cx = context(case, &elements, &visible);
    let (items, focus) = handle_search("o", &cx);
    assert_eq!(focus, Some(-1));
    let kinds: Vec<MatchKind> = items.iter().map(|m| m.kind).collect();
    let first_text = kinds.iter().position(|k| *k == MatchKind::Text).unwrap();
    assert!(kinds[..first_text].iter().all(|k| *k == MatchKind::Frame));
    assert!(kinds[first_text..].iter().all(|k| *k == MatchKind::Text));
    assert!(first_text > 0);

    let mut state = SearchMenuState::default();
    update(&mut state, SearchMenuEvent::Input("o".into()), &cx);
    run_pending(&mut state, &cx);
    let dom = search_menu(SearchMenuProps {
        state: &state,
        focus: InputFocus::None,
        on_event: None,
    });
    let mut titles = Vec::new();
    find_all(&dom, "layer-ui__search-result-title", &mut titles);
    let titles: Vec<String> = titles
        .iter()
        .map(|t| {
            let v = tree(&Node::Element((*t).clone()));
            v["children"][1]["children"][0].as_str().unwrap().to_owned()
        })
        .collect();
    assert_eq!(
        titles,
        [search_text("search.frames"), search_text("search.texts")]
    );
    // the deleted text never matches
    let (items, _) = handle_search("deleted", &cx);
    assert!(items.is_empty());
}

#[test]
fn the_locale_strings_are_upstreams() {
    for (k, v) in fixture()["locale"].as_object().unwrap() {
        assert_eq!(search_text(k), v.as_str().unwrap(), "{k}");
    }
}

#[test]
fn the_stylesheet_is_search_menu_scss() {
    assert!(SEARCH_MENU_CSS.starts_with("/* Generated by tools/goldens/search-menu.mjs"));
    for rule in [
        ".excalidraw .layer-ui__search {",
        ".excalidraw .layer-ui__search-result-title {",
        ".excalidraw .layer-ui__result-item.active {",
        ".excalidraw.theme--dark.excalidraw .layer-ui__search-header .ExcTextField__input {",
    ] {
        assert!(SEARCH_MENU_CSS.contains(rule), "{rule}");
    }
}
