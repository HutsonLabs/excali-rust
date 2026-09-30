//! Font picker top-picks customisation (ex-538): upstream's
//! `fontTopPicksDnD.ts` over the shared drag and drop
//! (`TopPicksDnD/topPicksDnD.tsx`), the strip's context menu
//! (`TopPicksContextMenu.tsx`) and the popup's tip (`TopPicksTip.tsx`) in
//! `FontPicker.tsx:115-303` and `FontPickerList.tsx:283-435`.
//!
//! Fixture: `tests/fixtures/font-top-picks-dnd.json`, written by
//! `tools/goldens/font-top-picks-dnd.mjs` from upstream at the pinned
//! commit (React 19.0.0, radix-ui 1.4.3, jsdom 22.1.0, a fixed layout and a
//! fake clock). Each drag scenario is replayed here through
//! [`TopPicksDnd`] with the same pointer events, layout and times, and the
//! strip, the ghosts, the body class, the host's callbacks and its state
//! are compared step by step, as `FontPicker.test.tsx`'s "top picks drag &
//! drop" cases hold upstream's.

use std::collections::BTreeMap;

use excali_core::element::FontFamily;
use excali_scene::shape::Theme;
use excali_ui::color_picker::StylesPanelMode;
use excali_ui::dom::{Element, Node};
use excali_ui::font_picker::{
    font_family_icon, font_family_string, font_ghost, font_ghost_rect, font_picker,
    font_picker_dnd, font_picker_text, ghost_template_index, hover_event, top_picks,
    FontGhostSource, FontGhostStyle, FontListContext, FontPickerEvent, FontPickerProps,
    FontPickerState,
};
use excali_ui::top_picks_dnd::{
    ghost_element, DragOrigin, PointerDown, Rect, StripLayout, TopPicksDnd,
};
use serde_json::{json, Map, Value};

fn fixture() -> &'static Value {
    use std::sync::OnceLock;
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/font-top-picks-dnd.json")).unwrap()
    })
}

// -- helpers ----------------------------------------------------------------------

fn family(v: &Value) -> Option<FontFamily> {
    v.as_u64().map(|n| FontFamily(n as u32))
}

fn families(v: &Value) -> Vec<FontFamily> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| family(x).unwrap())
        .collect()
}

fn rect(v: &Value) -> Rect {
    Rect {
        left: v["left"].as_f64().unwrap(),
        top: v["top"].as_f64().unwrap(),
        width: v["width"].as_f64().unwrap(),
        height: v["height"].as_f64().unwrap(),
    }
}

/// The fixture's layout: each strip pick's rect, the strip's, any other.
fn layout() -> (StripLayout, Rect) {
    let l = &fixture()["layout"];
    let pick = rect(&l["pick"]);
    let step = l["pick"]["step"].as_f64().unwrap();
    let slots = (0..3)
        .map(|i| Rect {
            left: pick.left + i as f64 * step,
            ..pick
        })
        .collect();
    (
        StripLayout {
            strip: rect(&l["strip"]),
            slots,
        },
        rect(&l["other"]),
    )
}

fn state_from(v: &Value) -> FontPickerState {
    FontPickerState {
        open_popup: v["openPopup"].as_str().map(str::to_owned),
        selected: family(&v["selectedFontFamily"]),
        hovered: family(&v["currentHoveredFontFamily"]),
        top_picks: v["fontTopPicks"]
            .as_array()
            .map(|_| families(&v["fontTopPicks"])),
        search: String::new(),
    }
}

fn state_json(s: &FontPickerState) -> Value {
    json!({
        "openPopup": s.open_popup,
        "selectedFontFamily": s.selected.map(|f| f.0),
        "currentHoveredFontFamily": s.hovered.map(|f| f.0),
        "fontTopPicks": s.top_picks.as_ref().map(|p| p.iter().map(|f| f.0).collect::<Vec<_>>()),
    })
}

fn context(case: &Value) -> FontListContext {
    FontListContext {
        scene_families: families(&case["scene"]),
        show_deprecated_fonts: false,
    }
}

fn mode(case: &Value) -> StylesPanelMode {
    match case["mode"].as_str().unwrap() {
        "full" => StylesPanelMode::Full,
        "compact" => StylesPanelMode::Compact,
        other => panic!("mode {other}"),
    }
}

fn props(case: &Value, state: &FontPickerState) -> FontPickerProps {
    FontPickerProps {
        state: state.clone(),
        mode: mode(case),
        list: context(case),
        theme: Theme::Light,
        phone: false,
        is_darwin: false,
        popup_id: "radix-1".into(),
        on_event: None,
        top_picks_menu: None,
        dnd: None,
    }
}

/// A host callback as the fixture records it; `None` for what the port's
/// host keeps itself (the menu's point, a drag's render).
fn call_json(e: &FontPickerEvent) -> Option<Value> {
    Some(match e {
        FontPickerEvent::Select(f) => json!(["select", f.0]),
        FontPickerEvent::Hover(f) => json!(["hover", f.0]),
        FontPickerEvent::Leave => json!(["leave"]),
        FontPickerEvent::PopupChange(open) => json!(["popupChange", open]),
        FontPickerEvent::TopPicksChange(p) => json!([
            "topPicksChange",
            p.as_ref()
                .map(|p| p.iter().map(|f| f.0).collect::<Vec<_>>())
        ]),
        FontPickerEvent::TopPicksMenu(_) | FontPickerEvent::DragChange => return None,
        other => panic!("unexpected {other:?}"),
    })
}

/// Dispatches an event through the host's state, collecting its callbacks.
fn dispatch(
    state: &mut FontPickerState,
    event: FontPickerEvent,
    cx: &FontListContext,
    calls: &mut Vec<Value>,
) {
    for e in state.dispatch(event, cx) {
        calls.extend(call_json(&e));
    }
}

/// React re-renders and radix re-fire the same callback; the host's state
/// sees each change once.
fn dedup(calls: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for c in calls {
        if out.last() != Some(&c) {
            out.push(c);
        }
    }
    out
}

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

/// An `{icon}` of the fixture as the icon's tree; a ghost's cloned icon
/// (`{icon, style}`) with the style the ghost wrote on it.
fn expand(v: &Value) -> Value {
    match v {
        Value::Object(o) if o.contains_key("icon") => {
            let name = o["icon"].as_str().unwrap();
            let icon = excali_ui::icons::icon(name).unwrap_or_else(|| panic!("no icon {name}"));
            let mut el = icon.element(Theme::Light).unwrap();
            if let Some(style) = o.get("style").and_then(Value::as_object) {
                for (k, x) in style {
                    el = el.style(k.as_str(), x.as_str().unwrap());
                }
            }
            tree(&Node::Element(el))
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

fn expand_all(v: &Value) -> Value {
    Value::Array(v.as_array().unwrap().iter().map(expand).collect())
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
        _ => format!("{path}:\nexpected {expected}\nactual   {actual}"),
    }
}

fn assert_same(what: &str, expected: &Value, actual: &Value) {
    if expected != actual {
        panic!(
            "{what}: {}",
            first_difference(expected, actual, String::new())
        );
    }
}

fn walk<'a>(n: &'a Node, out: &mut Vec<&'a Element>) {
    if let Node::Element(el) = n {
        out.push(el);
        for c in el.children() {
            walk(c, out);
        }
    }
}

fn elements(nodes: &[Node]) -> Vec<&Element> {
    let mut out = Vec::new();
    for n in nodes {
        walk(n, &mut out);
    }
    out
}

fn has_class(el: &Element, class: &str) -> bool {
    el.attribute("class")
        .unwrap_or("")
        .split(' ')
        .any(|c| c == class)
}

fn listens(el: &Element, event: &str) -> bool {
    el.listened_events().any(|e| e == event)
}

fn rendered(p: &FontPickerProps) -> Value {
    Value::Array(font_picker(p).iter().map(tree).collect())
}

// -- the strings ----------------------------------------------------------------

#[test]
fn the_locale_strings_are_upstreams() {
    for (key, value) in fixture()["locale"].as_object().unwrap() {
        assert_eq!(font_picker_text(key), value.as_str().unwrap(), "{key}");
    }
}

// -- the strip's context menu and the tip -------------------------------------------

#[test]
fn the_context_menu_resets_the_strip() {
    let menus = fixture()["menus"].as_array().unwrap();
    assert_eq!(menus.len(), 3);
    for case in menus {
        let name = case["name"].as_str().unwrap();
        let cx = context(case);
        let mut state = state_from(&case["initial"]);
        let mut p = props(case, &state);
        let closed = font_picker(&p);
        let wrapper = elements(&closed)
            .into_iter()
            .find(|e| has_class(e, "FontPicker__top-picks"))
            .unwrap();
        assert!(listens(wrapper, "contextmenu"), "{name}");
        // a right click at (60, 36): the host keeps the point, and radix
        // dismisses the popover the menu takes the focus from
        let mut calls = Vec::new();
        if state.is_open() {
            dispatch(
                &mut state,
                FontPickerEvent::PopupChange(false),
                &cx,
                &mut calls,
            );
        }
        assert_eq!(
            json!(calls),
            case["openedCalls"],
            "menu {name} opened calls"
        );
        p = props(case, &state);
        p.top_picks_menu = Some((60, 36));
        assert_same(
            &format!("menu {name} opened"),
            &expand_all(&case["opened"]),
            &rendered(&p),
        );
        // the item: disabled unless customised, else a reset
        let nodes = font_picker(&p);
        let item = elements(&nodes)
            .into_iter()
            .find(|e| has_class(e, "top-picks-dnd__context-menu-item"))
            .unwrap();
        let customized = state.top_picks.as_ref().is_some_and(|t| !t.is_empty());
        assert_eq!(
            item.attribute("data-disabled").is_some(),
            !customized,
            "{name}"
        );
        assert_eq!(listens(item, "click"), customized, "{name}");
        let mut calls = Vec::new();
        if customized {
            dispatch(
                &mut state,
                FontPickerEvent::TopPicksChange(None),
                &cx,
                &mut calls,
            );
            p = props(case, &state);
        }
        assert_eq!(json!(calls), case["calls"], "menu {name} calls");
        assert_eq!(state_json(&state), case["state"], "menu {name} state");
        if customized {
            assert_same(
                &format!("menu {name} after"),
                &expand_all(&case["after"]),
                &rendered(&p),
            );
        }
    }
}

#[test]
fn the_tips_reset_link_resets_the_strip() {
    for case in fixture()["tips"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let cx = context(case);
        let mut state = state_from(&case["initial"]);
        let nodes = font_picker(&props(case, &state));
        let reset = elements(&nodes)
            .into_iter()
            .find(|e| has_class(e, "top-picks-dnd__tip-reset"))
            .unwrap();
        for ev in ["mousedown", "click", "keydown"] {
            assert!(listens(reset, ev), "{name} {ev}");
        }
        let mut calls = Vec::new();
        dispatch(
            &mut state,
            FontPickerEvent::TopPicksChange(None),
            &cx,
            &mut calls,
        );
        assert_eq!(json!(calls), case["calls"], "{name}");
        assert_eq!(state_json(&state), case["state"], "{name}");
        assert_same(
            &format!("tip {name} after"),
            &expand_all(&case["after"]),
            &rendered(&props(case, &state)),
        );
    }
}

#[test]
fn only_the_full_panel_drags() {
    // FontPicker.tsx:234-251: the strip, and so its drag and drop, context
    // menu and tip, only outside compact mode
    let case = json!({"mode": "full", "scene": [5]});
    let state = FontPickerState {
        open_popup: Some("fontFamily".into()),
        selected: Some(FontFamily(5)),
        ..FontPickerState::default()
    };
    let mut p = props(&case, &state);
    p.dnd = Some(font_picker_dnd());
    let nodes = font_picker(&p);
    let draggable: Vec<&Element> = elements(&nodes)
        .into_iter()
        .filter(|e| listens(e, "pointerdown"))
        .collect();
    let picks = draggable
        .iter()
        .filter(|e| e.attribute("data-top-pick-index").is_some())
        .count();
    let rows = draggable
        .iter()
        .filter(|e| has_class(e, "dropdown-menu-item"))
        .count();
    assert_eq!(picks, 3);
    // every row: Excalifont, Nunito, Lilita One, Comic Shanns
    let all_rows = elements(&nodes)
        .into_iter()
        .filter(|e| has_class(e, "dropdown-menu-item"))
        .count();
    assert_eq!((rows, all_rows), (4, 4));
    let compact = json!({"mode": "compact", "scene": [5]});
    let mut p = props(&compact, &state);
    p.dnd = Some(font_picker_dnd());
    let nodes = font_picker(&p);
    // no strip, and the list's rows drag nothing (the trigger's IconButton
    // listens for its own pointerdown)
    let rows: Vec<&Element> = elements(&nodes)
        .into_iter()
        .filter(|e| has_class(e, "dropdown-menu-item"))
        .collect();
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|e| !listens(e, "pointerdown")));
    assert!(elements(&nodes)
        .iter()
        .all(|e| e.attribute("data-top-pick-index").is_none() && !listens(e, "contextmenu")));
    assert!(elements(&nodes)
        .iter()
        .all(|e| !has_class(e, "top-picks-dnd__tip")));
}

#[test]
fn a_dragged_font_does_not_preview_the_rows_it_passes() {
    // FontPickerList.tsx:328-333
    let (f, g) = (FontFamily(7), FontFamily(6));
    assert_eq!(hover_event(false, None, f), Some(FontPickerEvent::Hover(f)));
    assert_eq!(
        hover_event(false, Some(g), f),
        Some(FontPickerEvent::Hover(f))
    );
    assert_eq!(hover_event(false, Some(f), f), None);
    assert_eq!(hover_event(true, None, f), None);
}

// -- drags ----------------------------------------------------------------------------

/// jsdom's computed style of a strip button and a glyph sample, which
/// createFontGhost samples (`fontTopPicksDnD.ts:19-40, 55-63`): the default
/// stylesheet's button colours and border, no radius, no font size.
fn jsdom_ghost_style(icon: Option<(f64, f64)>) -> FontGhostStyle {
    FontGhostStyle {
        background_color: "ButtonFace".into(),
        color: "ButtonText".into(),
        border_width: "2px".into(),
        border_color: "buttonface".into(),
        border_radius: String::new(),
        icon_size: icon,
        sample_font_size: String::new(),
    }
}

/// The strip as upstream's fixture holds it, without the inline
/// `transform: none` that `settleStripInstantly` leaves on the picks React
/// kept (the port renders the strip afresh, where no transform is none).
fn settled(strip: &Value) -> Value {
    let mut strip = expand(strip);
    fn walk(v: &mut Value) {
        if let Some(style) = v.get_mut("style").and_then(Value::as_object_mut) {
            if style.get("transform") == Some(&json!("none")) {
                style.remove("transform");
            }
        }
        if let Some(children) = v.get_mut("children").and_then(Value::as_array_mut) {
            children.iter_mut().for_each(walk);
        }
    }
    walk(&mut strip);
    strip
}

/// A list row or a strip pick of the port's DOM.
fn find_source<'a>(nodes: &'a [Node], on: &Value) -> Option<&'a Element> {
    let all = elements(nodes);
    if let Some(i) = on["pick"].as_u64() {
        let i = i.to_string();
        return all
            .into_iter()
            .find(|e| e.attribute("data-top-pick-index") == Some(i.as_str()));
    }
    let value = on["row"].as_u64().unwrap().to_string();
    all.into_iter()
        .find(|e| has_class(e, "dropdown-menu-item") && e.attribute("value") == Some(&value))
}

#[test]
fn every_drag_replays_upstreams_steps() {
    let drags = fixture()["drags"].as_array().unwrap();
    assert!(drags.len() >= 14, "{} drags", drags.len());
    let (strip_layout, other) = layout();
    for drag in drags {
        let name = drag["name"].as_str().unwrap();
        let cx = context(drag);
        let mut state = state_from(&drag["initial"]);
        let dnd: TopPicksDnd<FontFamily> = font_picker_dnd();
        let render = |state: &FontPickerState| {
            let mut p = props(drag, state);
            p.dnd = Some(dnd.clone());
            p
        };
        let strip_rendered = mode(drag) == StylesPanelMode::Full;
        let measure = || strip_rendered.then(|| strip_layout.clone());
        let mut now = 0.0;
        let mut target: Option<FontFamily> = None;
        let mut contents: BTreeMap<u64, Element> = BTreeMap::new();
        for (i, record) in drag["steps"].as_array().unwrap().iter().enumerate() {
            let step = &record["step"];
            let at = format!("drag {name} step {i} {step}");
            let pointer_id = step["pointerId"].as_i64().unwrap_or(1) as i32;
            let mut calls: Vec<Value> = Vec::new();
            let xy = |v: &Value| (v[0].as_f64().unwrap(), v[1].as_f64().unwrap());
            if let Some(on) = step.get("down") {
                let p = render(&state);
                let nodes = font_picker(&p);
                let el = find_source(&nodes, on).unwrap_or_else(|| panic!("{at}: no source"));
                let picks = top_picks(state.top_picks.as_deref());
                let (value, origin) = match on["pick"].as_u64() {
                    Some(i) => (picks[i as usize], DragOrigin::Pick(i as usize)),
                    None => (family(&on["row"]).unwrap(), DragOrigin::Source),
                };
                let template = strip_layout.slots[ghost_template_index(&picks, state.selected)];
                let home = match origin {
                    DragOrigin::Pick(i) => font_ghost_rect(true, strip_layout.slots[i], other),
                    DragOrigin::Source => font_ghost_rect(false, template, other),
                };
                let session = if listens(el, "pointerdown") {
                    dnd.pointer_down(
                        PointerDown {
                            pointer_id,
                            button: step["button"].as_i64().unwrap_or(0) as i16,
                            x: step["x"].as_f64().unwrap(),
                            y: step["y"].as_f64().unwrap(),
                            now,
                            value: Some(value),
                            origin,
                            home,
                        },
                        strip_rendered,
                    )
                } else {
                    None
                };
                let expected = &record["session"];
                assert_eq!(session.is_some(), !expected.is_null(), "{at}: session");
                if let Some(id) = session {
                    assert_eq!(json!(value.0), expected["value"], "{at}: value");
                    let source = match origin {
                        DragOrigin::Pick(_) => FontGhostSource::Pick(&picks),
                        DragOrigin::Source => FontGhostSource::Row,
                    };
                    let icon = Some((other.width, other.height));
                    contents.insert(id, font_ghost(value, source, &jsdom_ghost_style(icon)));
                }
                target = Some(value);
            } else if let Some(ms) = step["wait"].as_f64() {
                now += ms;
                dnd.advance(now, &measure);
            } else if step.get("move").is_some() {
                let (x, y) = xy(&step["move"]);
                dnd.pointer_move(pointer_id, x, y, now, &measure);
            } else if let Some(f) = family(&step["hover"]) {
                let nodes = font_picker(&render(&state));
                let row = find_source(&nodes, &json!({ "row": f.0 })).unwrap();
                assert!(listens(row, "mousemove"), "{at}");
                if let Some(e) = hover_event(dnd.drag_state().is_some(), state.hovered, f) {
                    dispatch(&mut state, e, &cx, &mut calls);
                }
            } else if step.get("up").is_some() {
                let outcome = dnd.pointer_up(pointer_id, now);
                if let Some(picks) = outcome.picks {
                    dispatch(
                        &mut state,
                        FontPickerEvent::TopPicksChange(Some(picks)),
                        &cx,
                        &mut calls,
                    );
                }
            } else if step.get("frame").is_some() {
                dnd.frame();
            } else if let Some(key) = step["key"].as_str() {
                dnd.key_down(key, now);
            } else if step.get("cancel").is_some() {
                dnd.pointer_cancel(pointer_id, now);
            } else if step.get("click").is_some() {
                let reached = !dnd.take_click_suppression(now);
                assert_eq!(json!(reached), record["clickReached"], "{at}: click");
                if reached {
                    dispatch(
                        &mut state,
                        FontPickerEvent::Select(target.unwrap()),
                        &cx,
                        &mut calls,
                    );
                }
            } else {
                panic!("{at}: unknown step");
            }
            assert_eq!(now, record["now"].as_f64().unwrap(), "{at}: clock");
            assert_eq!(
                dedup(calls),
                dedup(record["calls"].as_array().unwrap().clone()),
                "{at}: calls"
            );
            assert_eq!(state_json(&state), record["state"], "{at}: state");
            let nodes = font_picker(&render(&state));
            let strip = elements(&nodes)
                .into_iter()
                .find(|e| has_class(e, "FontPicker__top-picks"))
                .map(|e| tree(&Node::Element(e.clone())));
            let expected = (!record["strip"].is_null()).then(|| settled(&record["strip"]));
            assert_same(&format!("{at}: strip"), &json!(expected), &json!(strip));
            let ghosts: Vec<Value> = dnd
                .ghosts()
                .iter()
                .map(|g| tree(&Node::Element(ghost_element(g, contents[&g.id].clone()))))
                .collect();
            assert_same(
                &format!("{at}: ghosts"),
                &expand_all(&record["ghosts"]),
                &json!(ghosts),
            );
            assert_eq!(json!(dnd.body_active()), record["bodyActive"], "{at}: body");
        }
    }
}

#[test]
fn a_drop_pins_reorders_and_refuses_duplicates() {
    // the acceptance criteria, as FontPicker.test.tsx states them
    let drag = |name: &str| {
        fixture()["drags"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    let pinned = |name: &str| -> Vec<Value> {
        drag(name)["steps"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|s| s["calls"].as_array().unwrap().clone())
            .filter(|c| c[0] == "topPicksChange")
            .collect()
    };
    // Lilita One from the list onto slot 1 replaces "Normal"
    assert_eq!(
        pinned("pin-list-font"),
        vec![json!(["topPicksChange", [5, 7, 8]])]
    );
    // an already pinned font is refused
    assert!(pinned("pin-duplicate").is_empty());
    // the padded defaults count as pinned, and a drop persists the strip
    assert_eq!(
        pinned("pads-short-list"),
        vec![json!(["topPicksChange", [7, 8, 6]])]
    );
    // a pick moved from 0 to 2
    assert_eq!(
        pinned("reorder-forward"),
        vec![json!(["topPicksChange", [6, 8, 5]])]
    );
    assert!(pinned("escape-cancels").is_empty());
    assert!(pinned("drop-outside").is_empty());
}

#[test]
fn a_ghost_is_the_fonts_tile() {
    // createFontGhost (fontTopPicksDnD.ts:19-82)
    let style = FontGhostStyle {
        background_color: "rgb(236, 236, 244)".into(),
        color: "rgb(27, 27, 31)".into(),
        border_width: "1px".into(),
        border_color: "rgb(236, 236, 244)".into(),
        border_radius: "8px".into(),
        icon_size: Some((16.0, 16.0)),
        sample_font_size: "14px".into(),
    };
    let lilita = FontFamily(7);
    let row = font_ghost(lilita, FontGhostSource::Row, &style);
    let icon = font_family_icon(lilita)
        .element(Theme::Light)
        .unwrap()
        .style("width", "16px")
        .style("height", "16px");
    let tile = Element::new("div")
        .attr("class", "excalidraw-font-dnd-ghost-tile")
        .style("background-color", "rgb(236, 236, 244)")
        .style("color", "rgb(27, 27, 31)")
        .style("border-style", "solid")
        .style("border-width", "1px")
        .style("border-color", "rgb(236, 236, 244)")
        .style("border-radius", "8px");
    assert_eq!(
        tree(&Node::Element(row)),
        tree(&Node::Element(tile.clone().child(icon)))
    );
    // a pick showing a glyph sample carries it
    let (cascadia, code) = (FontFamily(3), FontFamily(8));
    let picks = [cascadia, code, FontFamily(5)];
    let sample = font_ghost(cascadia, FontGhostSource::Pick(&picks), &style);
    let span = Element::new("span")
        .attr("class", "FontPicker__top-pick-sample")
        .style("font-family", font_family_string(cascadia))
        .style("font-size", "14px")
        .style("line-height", "1")
        .child("Aa");
    assert_eq!(
        tree(&Node::Element(sample)),
        tree(&Node::Element(tile.child(span)))
    );
    // a list row's ghost is the template's size, centred on its icon
    let template = Rect {
        left: 20.0,
        top: 20.0,
        width: 32.0,
        height: 30.0,
    };
    let anchor = Rect {
        left: 100.0,
        top: 200.0,
        width: 16.0,
        height: 16.0,
    };
    assert_eq!(
        font_ghost_rect(false, template, anchor),
        Rect {
            left: 92.0,
            top: 193.0,
            width: 32.0,
            height: 30.0
        }
    );
    assert_eq!(font_ghost_rect(true, template, anchor), template);
    // the template is the first pick that is not the active one
    assert_eq!(ghost_template_index(&picks, Some(cascadia)), 1);
    assert_eq!(ghost_template_index(&picks, Some(code)), 0);
    assert_eq!(ghost_template_index(&picks, None), 0);
}
