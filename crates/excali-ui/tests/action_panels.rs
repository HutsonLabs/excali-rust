//! The action panels (ex-540): what the `PanelComponent` of each action
//! the styles panels render through `renderAction` renders
//! (`packages/excalidraw/actions/*.tsx`, with `RadioSelection`,
//! `RadioButton`, `IconButton`, `Range` and `IconPicker`), and what its
//! handlers pass to `updateData`, against upstream.
//!
//! Fixture: `tests/fixtures/action-panels.json`, upstream's own
//! components at the pinned commit (`tools/goldens/action-panels.mjs`):
//! per case app state over a scene of upstream-built elements, the
//! language's direction, IconPicker's and the history's state, and per
//! styles panel mode what each action's panel component renders (see the
//! generator's header), with each handler's `updateData` calls.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use excali_core::app_state::AppState;
use excali_core::element::{Element, FontFamily};
use excali_editor::actions::{ActionContext, ActionEnv, ActionName, AppProps};
use excali_scene::shape::Theme;
use excali_ui::action_panels::{
    bucket_fill_color_panel, font_family_panel, render_action_panel, ActionPanelOptions,
    IconPickerState, ICON_PICKER_ALIGN_OFFSET, ICON_PICKER_SIDE_OFFSET,
};
use excali_ui::color_picker::StylesPanelMode;
use excali_ui::dom::{EventData, Node};
use serde_json::{json, Map, Value};

fn fixture() -> &'static Value {
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let text = include_str!("fixtures/action-panels.json");
        serde_json::from_str(text).expect("action-panels.json parses")
    })
}

fn icons() -> &'static HashMap<String, Value> {
    static ICONS: OnceLock<HashMap<String, Value>> = OnceLock::new();
    ICONS.get_or_init(|| {
        let text = include_str!("fixtures/icons.json");
        let v: Value = serde_json::from_str(text).expect("icons.json parses");
        v["icons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| (i["name"].as_str().unwrap().to_string(), i.clone()))
            .collect()
    })
}

fn cases() -> &'static [Value] {
    fixture()["cases"].as_array().expect("cases")
}

fn columns() -> Vec<&'static str> {
    fixture()["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect()
}

fn tree(index: &Value) -> &'static Value {
    &fixture()["trees"][index.as_u64().expect("a tree index") as usize]
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
/// replaced by the scene's element.
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

fn mode(name: &str) -> StylesPanelMode {
    match name {
        "full" => StylesPanelMode::Full,
        "compact" => StylesPanelMode::Compact,
        "mobile" => StylesPanelMode::Mobile,
        other => panic!("mode {other}"),
    }
}

fn action_name(name: &str) -> ActionName {
    ActionName::ALL
        .into_iter()
        .find(|n| n.as_str() == name)
        .unwrap_or_else(|| panic!("action {name}"))
}

struct Case {
    id: String,
    elements: Vec<Element>,
    app_state: AppState,
    props: AppProps,
    env: ActionEnv,
    rtl: bool,
    icon_picker: IconPickerState,
    undo_empty: bool,
    redo_empty: bool,
}

impl Case {
    fn new(case: &Value) -> Case {
        let picker = &case["iconPicker"];
        Case {
            id: case["id"].as_str().unwrap().to_string(),
            elements: scene(case["scene"].as_str().unwrap()),
            app_state: app_state(case),
            props: AppProps::default(),
            env: ActionEnv::default(),
            rtl: case["rtl"].as_bool().unwrap(),
            icon_picker: IconPickerState {
                open: picker["open"].as_str().map(str::to_string),
                more_options: picker["more"].as_bool().unwrap(),
            },
            undo_empty: case["history"]["undoEmpty"].as_bool().unwrap(),
            redo_empty: case["history"]["redoEmpty"].as_bool().unwrap(),
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

    fn theme(&self) -> Theme {
        match self.app_state.get("theme").and_then(Value::as_str) {
            Some("dark") => Theme::Dark,
            _ => Theme::Light,
        }
    }

    /// The options renderAction passes, recording the callbacks in `calls`
    /// as the fixture writes them.
    fn options(
        &self,
        mode: StylesPanelMode,
        cycle: bool,
        calls: &Rc<RefCell<Vec<Value>>>,
    ) -> ActionPanelOptions {
        let updates = calls.clone();
        let pickers = calls.clone();
        let mut opts = ActionPanelOptions::new(
            mode,
            Rc::new(move |name: ActionName, value: Value| {
                updates
                    .borrow_mut()
                    .push(json!({ "update": value, "action": name.as_str() }));
            }),
        );
        opts.theme = self.theme();
        opts.rtl = self.rtl;
        opts.is_darwin = false;
        opts.cycle = cycle;
        opts.open_popup = self
            .app_state
            .get("openPopup")
            .and_then(Value::as_str)
            .map(str::to_string);
        opts.icon_picker = self.icon_picker.clone();
        opts.undo_stack_empty = self.undo_empty;
        opts.redo_stack_empty = self.redo_empty;
        opts.on_icon_picker = Some(Rc::new(move |s: IconPickerState| {
            pickers
                .borrow_mut()
                .push(json!({ "iconPicker": { "open": s.open, "more": s.more_options } }));
        }));
        opts
    }
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

/// The markup of an icon node: icons.json's (by theme) or, flipped,
/// the fixture's `flippedIcons`.
fn icon_markup(node: &Map<String, Value>) -> String {
    let name = node["icon"].as_str().unwrap();
    if node.get("flip") == Some(&json!(true)) {
        return fixture()["flippedIcons"][name]
            .as_str()
            .unwrap_or_else(|| panic!("flipped {name}"))
            .to_string();
    }
    let icon = icons().get(name).unwrap_or_else(|| panic!("icon {name}"));
    match icon["kind"].as_str().unwrap() {
        "static" => icon["markup"].as_str().unwrap().to_string(),
        "themed" => {
            let theme = node.get("theme").and_then(Value::as_str).unwrap_or("light");
            icon[theme].as_str().unwrap().to_string()
        }
        other => panic!("{name}: {other}"),
    }
}

/// A golden tree with each `{ action }` replaced by that action's golden
/// tree for the same case and mode (upstream's renderAction), each node
/// tagged with the action whose `updateData` its handlers call.
fn expand(nodes: &Value, owner: &str, case: &Value, mode: &str) -> Vec<(Value, String)> {
    let mut out = Vec::new();
    for node in nodes.as_array().unwrap() {
        if let Some(action) = node.get("action").and_then(Value::as_str) {
            assert!(node.get("data").is_none(), "nested renderAction with data");
            let i = columns().iter().position(|c| *c == action).unwrap();
            let nested = tree(&case["panels"][mode][i]);
            out.extend(expand(nested, action, case, mode));
        } else {
            out.push((node.clone(), owner.to_string()));
        }
    }
    out
}

/// Compares `got` with the golden `want` (owned by `owner`), firing each
/// recorded handler and checking what it calls.
struct Checker<'a> {
    case: &'a Case,
    calls: Rc<RefCell<Vec<Value>>>,
    failures: Vec<String>,
    fired: usize,
}

impl Checker<'_> {
    fn fail(&mut self, path: &str, what: String) {
        self.failures.push(format!("{} {path}: {what}", self.case.id));
    }

    fn nodes(&mut self, path: &str, got: &[Node], want: &[(Value, String)]) {
        if got.len() != want.len() {
            self.fail(
                path,
                format!(
                    "{} nodes, upstream {}: got {:?}, want {}",
                    got.len(),
                    want.len(),
                    got,
                    Value::Array(want.iter().map(|(v, _)| v.clone()).collect())
                ),
            );
            return;
        }
        for (i, (g, (w, owner))) in got.iter().zip(want).enumerate() {
            self.node(&format!("{path}/{i}"), g, w, owner);
        }
    }

    fn node(&mut self, path: &str, got: &Node, want: &Value, owner: &str) {
        match want {
            Value::String(text) => match got {
                Node::Text(t) if t == text => {}
                other => self.fail(path, format!("got {other:?}, want text {text:?}")),
            },
            Value::Object(o) if o.contains_key("icon") => {
                let markup = icon_markup(o);
                if got.to_html() != markup {
                    self.fail(path, format!("icon {}: got {}", o["icon"], got.to_html()));
                }
            }
            Value::Object(o) if o.get("tag") == Some(&json!("popover")) => {
                self.popover(path, got, o, owner)
            }
            Value::Object(o) if o.contains_key("tag") => {
                let Some(el) = got.as_element() else {
                    return self.fail(path, format!("got {got:?}, want <{}>", o["tag"]));
                };
                let mut attrs: Map<String, Value> = o
                    .get("attrs")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                if let Some(class) = o.get("class") {
                    attrs.insert("class".into(), class.clone());
                }
                let style: Vec<(String, String)> = o
                    .get("style")
                    .and_then(Value::as_object)
                    .map(|s| {
                        s.iter()
                            .map(|(k, v)| (css_name(k), v.as_str().unwrap().to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                self.element(path, el, o["tag"].as_str().unwrap(), &attrs, &style);
                let children: Vec<(Value, String)> = o
                    .get("children")
                    .and_then(Value::as_array)
                    .map(|c| c.iter().map(|v| (v.clone(), owner.to_string())).collect())
                    .unwrap_or_default();
                self.nodes(&format!("{path}<{}>", el.tag()), el.children(), &children);
                self.handlers(path, got, o.get("on"), owner);
            }
            other => self.fail(path, format!("unexpected golden node {other}")),
        }
    }

    fn element(
        &mut self,
        path: &str,
        el: &excali_ui::dom::Element,
        tag: &str,
        attrs: &Map<String, Value>,
        style: &[(String, String)],
    ) {
        if el.tag() != tag {
            self.fail(path, format!("tag {}, upstream {tag}", el.tag()));
        }
        let got: Map<String, Value> = el
            .attributes()
            .iter()
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect();
        if &got != attrs {
            self.fail(
                path,
                format!("<{tag}> attributes {}, upstream {}", Value::Object(got), Value::Object(attrs.clone())),
            );
        }
        let mut got_style: Vec<(String, String)> = el.style_properties().to_vec();
        let mut want_style = style.to_vec();
        got_style.sort();
        want_style.sort();
        if got_style != want_style {
            self.fail(path, format!("<{tag}> style {got_style:?}, upstream {want_style:?}"));
        }
    }

    /// radix's Popover.Content as the port mounts it: the popper wrapper
    /// and the content with the placement as `data-side`/`data-align`.
    fn popover(&mut self, path: &str, got: &Node, o: &Map<String, Value>, owner: &str) {
        let Some(wrapper) = got.as_element() else {
            return self.fail(path, format!("got {got:?}, want a popover"));
        };
        self.element(
            path,
            wrapper,
            "div",
            json!({ "data-radix-popper-content-wrapper": "" }).as_object().unwrap(),
            &[],
        );
        let [content] = wrapper.children() else {
            return self.fail(path, format!("popover wrapper holds {:?}", wrapper.children()));
        };
        let attrs = o["attrs"].as_object().unwrap();
        assert_eq!(attrs["sideOffset"], json!(ICON_PICKER_SIDE_OFFSET.to_string()));
        assert_eq!(attrs["alignOffset"], json!(ICON_PICKER_ALIGN_OFFSET.to_string()));
        let mut want = Map::new();
        want.insert("class".into(), o["class"].clone());
        for (k, v) in attrs {
            match k.as_str() {
                "side" => {
                    want.insert("data-side".into(), v.clone());
                }
                "align" => {
                    want.insert("data-align".into(), v.clone());
                }
                "sideOffset" | "alignOffset" => {}
                _ => {
                    want.insert(k.clone(), v.clone());
                }
            }
        }
        want.insert("data-state".into(), json!("open"));
        want.insert("tabindex".into(), json!("-1"));
        let style: Vec<(String, String)> = o["style"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (css_name(k), v.as_str().unwrap().to_string()))
            .collect();
        let Some(content_el) = content.as_element() else {
            return self.fail(path, "popover content is text".into());
        };
        self.element(path, content_el, "div", &want, &style);
        let children: Vec<(Value, String)> = o["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v.clone(), owner.to_string()))
            .collect();
        self.nodes(&format!("{path}<popover>"), content_el.children(), &children);
        self.handlers(path, content, o.get("on"), owner);
    }

    fn expected_calls(&self, calls: &Value, owner: &str) -> Value {
        Value::Array(
            calls
                .as_array()
                .unwrap()
                .iter()
                .map(|c| match c.get("update") {
                    // the React event itself: perform ignores it
                    Some(v) if v == &json!("[event]") => json!({ "update": null, "action": owner }),
                    Some(v) => json!({ "update": v, "action": owner }),
                    None => c.clone(),
                })
                .collect(),
        )
    }

    fn fire(&mut self, node: &Node, event: &str, data: &EventData) -> Option<(Value, excali_ui::dom::EventResponse)> {
        self.calls.borrow_mut().clear();
        let response = node.as_element()?.dispatch(event, data)?;
        self.fired += 1;
        Some((Value::Array(self.calls.borrow().clone()), response))
    }

    fn handlers(&mut self, path: &str, got: &Node, on: Option<&Value>, owner: &str) {
        let el = got.as_element().unwrap();
        let want_events: Vec<&str> = match on.and_then(Value::as_object) {
            None => vec![],
            Some(on) => {
                let mut events: Vec<&str> = on
                    .keys()
                    .map(|k| match k.as_str() {
                        "click" | "altClick" => "click",
                        // React's onChange: `input` for a range
                        "change" if is_range(el) => "input",
                        "change" => "change",
                        "keydown" => "keydown",
                        other => panic!("handler {other}"),
                    })
                    .collect();
                events.sort();
                events.dedup();
                events
            }
        };
        let mut got_events: Vec<&str> = el.data_events().collect();
        got_events.sort();
        let mut sorted_want = want_events.clone();
        sorted_want.sort();
        if got_events != sorted_want {
            return self.fail(path, format!("handlers {got_events:?}, upstream {sorted_want:?}"));
        }
        let Some(on) = on else { return };
        if let Some(click) = on.get("click") {
            let (calls, _) = self.fire(got, "click", &EventData::default()).unwrap();
            let want = self.expected_calls(click, owner);
            if calls != want {
                self.fail(path, format!("click calls {calls}, upstream {want}"));
            }
            let alt = on.get("altClick").unwrap_or(click);
            let data = EventData {
                alt_key: true,
                ..EventData::default()
            };
            let (calls, _) = self.fire(got, "click", &data).unwrap();
            let want = self.expected_calls(alt, owner);
            if calls != want {
                self.fail(path, format!("alt-click calls {calls}, upstream {want}"));
            }
        }
        if let Some(change) = on.get("change") {
            let data = EventData {
                value: Some("30".into()),
                ..EventData::default()
            };
            let event = if is_range(el) { "input" } else { "change" };
            let (calls, _) = self.fire(got, event, &data).unwrap();
            let want = self.expected_calls(change, owner);
            if calls != want {
                self.fail(path, format!("change calls {calls}, upstream {want}"));
            }
        }
        if let Some(keys) = on.get("keydown").and_then(Value::as_object) {
            for (spec, outcome) in keys {
                let mut parts: Vec<&str> = spec.split('+').collect();
                let key = parts.pop().unwrap();
                let data = EventData {
                    key: key.to_string(),
                    shift_key: parts.contains(&"Shift"),
                    meta_key: parts.contains(&"Meta"),
                    alt_key: parts.contains(&"Alt"),
                    ctrl_key: parts.contains(&"Ctrl"),
                    ..EventData::default()
                };
                let (calls, response) = self.fire(got, "keydown", &data).unwrap();
                let want = self.expected_calls(&outcome["calls"], owner);
                if calls != want {
                    self.fail(path, format!("{spec}: calls {calls}, upstream {want}"));
                }
                if json!(response.prevent_default) != outcome["prevented"] {
                    self.fail(path, format!("{spec}: prevented {}", response.prevent_default));
                }
                if json!(response.stop_propagation) != outcome["stopped"] {
                    self.fail(path, format!("{spec}: stopped {}", response.stop_propagation));
                }
            }
        }
    }
}

fn is_range(el: &excali_ui::dom::Element) -> bool {
    el.tag() == "input" && el.attribute("type") == Some("range")
}

/// Host-rendered pickers are checked by their own tests below.
const HOST_RENDERED: [&str; 2] = ["changeFontFamily", "changeBucketFillBackgroundColor"];

#[test]
fn fixture_covers_the_panels() {
    let cases = cases();
    assert!(cases.len() >= 300, "{} cases", cases.len());
    assert_eq!(columns().len(), 38);
    let trees = fixture()["trees"].as_array().unwrap();
    assert!(trees.len() >= 100, "{} trees", trees.len());
    // every column renders something somewhere, and nothing somewhere for
    // the ones that may render null
    for (i, column) in columns().iter().enumerate() {
        let rendered = cases
            .iter()
            .flat_map(|c| ["full", "compact", "mobile"].map(|m| tree(&c["panels"][m][i])))
            .filter(|t| !t.as_array().unwrap().is_empty())
            .count();
        assert!(rendered > 0, "{column} never renders");
    }
    let json = serde_json::to_string(fixture()).unwrap();
    for needle in [
        "\"altClick\"",
        "\"FillZigZagIcon\"",
        "\"flip\":true",
        "\"keydown\"",
        "\"iconPicker\"",
        "Break polygon",
        "var(--mobile-action-button-bg)",
        "\"theme\":\"dark\"",
        "\"picker-section-label\"",
    ] {
        assert!(json.contains(needle), "the fixture has no {needle}");
    }
}

#[test]
fn action_panels_match_upstream() {
    let mut failures = Vec::new();
    let mut fired = 0;
    let mut compared = 0;
    for case in cases() {
        let c = Case::new(case);
        for (m, _) in fixture()["modes"].as_object().unwrap() {
            for (i, column) in columns().iter().enumerate() {
                if HOST_RENDERED.contains(column) {
                    continue;
                }
                let (name, cycle) = match column.split_once('+') {
                    Some((name, "cycle")) => (name, true),
                    _ => (*column, false),
                };
                let calls = Rc::new(RefCell::new(Vec::new()));
                let opts = c.options(mode(m), cycle, &calls);
                let Some(got) = render_action_panel(&c.ctx(), action_name(name), &opts) else {
                    failures.push(format!("{} {m} {column}: no panel", c.id));
                    continue;
                };
                let want = expand(tree(&case["panels"][m.as_str()][i]), name, case, m);
                let mut checker = Checker {
                    case: &c,
                    calls,
                    failures: Vec::new(),
                    fired: 0,
                };
                checker.nodes(&format!("{m} {column}"), &got, &want);
                fired += checker.fired;
                compared += 1;
                failures.extend(checker.failures);
            }
        }
    }
    assert!(compared > 30_000, "{compared} panels");
    assert!(fired > 100_000, "{fired} handlers fired");
    let shown: Vec<&String> = failures.iter().take(40).collect();
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        shown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn host_rendered_pickers_match_upstream() {
    let mut failures = Vec::new();
    let columns = columns();
    let font = columns.iter().position(|c| *c == "changeFontFamily").unwrap();
    let bucket = columns
        .iter()
        .position(|c| *c == "changeBucketFillBackgroundColor")
        .unwrap();
    for case in cases() {
        let c = Case::new(case);
        for (m, _) in fixture()["modes"].as_object().unwrap() {
            let ctx = c.ctx();
            // the font family: the legend in the full panel, FontPicker's props
            let p = font_family_panel(&ctx, mode(m));
            let mut got = Vec::new();
            if let Some(legend) = &p.legend {
                got.push(json!({ "tag": "legend", "children": [legend] }));
            }
            let family = |f: Option<FontFamily>| f.map(|f| json!(f.0)).unwrap_or(Value::Null);
            got.push(json!({ "fontPicker": {
                "isOpened": p.is_opened,
                "selectedFontFamily": family(p.state.selected),
                "hoveredFontFamily": family(p.state.hovered),
                "topPicks": p.state.top_picks.as_ref().map(|t| t.iter().map(|f| f.0).collect::<Vec<_>>()),
                "compactMode": p.compact_mode,
            }}));
            let want = tree(&case["panels"][m.as_str()][font]);
            if &Value::Array(got.clone()) != want {
                failures.push(format!("{} {m} font: got {}, upstream {want}", c.id, Value::Array(got)));
            }
            // the bucket fill colour: the heading and ColorPicker's props
            let p = bucket_fill_color_panel(&ctx, mode(m));
            let mut got = Vec::new();
            if let Some(key) = p.heading {
                got.push(json!({ "tag": "h3", "attrs": { "aria-hidden": "true" }, "children": [excali_ui::styles_panel::legend_text(key)] }));
            }
            got.push(json!({ "colorPicker": {
                "type": p.ty.as_str(),
                "label": excali_ui::styles_panel::legend_text(p.label),
                "color": p.color,
                "topPicks": p.top_picks,
                "customizableTopPicks": p.customizable_top_picks.as_str(),
                "excludedColors": p.excluded_colors,
            }}));
            let want = tree(&case["panels"][m.as_str()][bucket]);
            if &Value::Array(got.clone()) != want {
                failures.push(format!("{} {m} bucket: got {}, upstream {want}", c.id, Value::Array(got)));
            }
        }
    }
    assert!(failures.is_empty(), "{} differ:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn actions_without_a_styles_panel_component_render_none() {
    let c = Case::new(&cases()[0]);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let opts = c.options(StylesPanelMode::Full, false, &calls);
    for name in [
        ActionName::ChangeStrokeColor,
        ActionName::ChangeBackgroundColor,
        ActionName::ChangeBucketFillBackgroundColor,
        ActionName::ChangeFontFamily,
        ActionName::ZoomIn,
        ActionName::SelectAll,
    ] {
        assert!(render_action_panel(&c.ctx(), name, &opts).is_none(), "{name:?}");
    }
}
