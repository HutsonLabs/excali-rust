//! Colour picker top-picks customisation (ex-536): upstream's
//! `customizableTopPicks` (`ColorPicker.tsx:353-500`,
//! `colorTopPicksDnD.ts`), the shared drag and drop
//! (`TopPicksDnD/topPicksDnD.tsx`), the strip's context menu
//! (`TopPicksContextMenu.tsx`) and the popup's tip (`TopPicksTip.tsx`).
//!
//! Fixture: `tests/fixtures/color-top-picks-dnd.json`, written by
//! `tools/goldens/color-top-picks-dnd.mjs` from upstream at the pinned
//! commit (React 19.0.0, radix-ui 1.4.3, jsdom 22.1.0, a fixed layout and a
//! fake clock). Each drag scenario is replayed here through
//! [`TopPicksDnd`] with the same pointer events, layout and times, and the
//! strip, the ghosts, the body class and the host's calls are compared
//! step by step. `src/top_picks_dnd/top_picks_dnd.css` is the same
//! generator's TopPicksDnD.scss.

use std::collections::BTreeMap;

use excali_core::color::{
    PaletteColor, PaletteEntry, BUCKET_FILL_BACKGROUND_PICKS,
    DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE, DEFAULT_ELEMENT_BACKGROUND_PICKS,
    DEFAULT_ELEMENT_STROKE_COLOR_PALETTE, DEFAULT_ELEMENT_STROKE_PICKS, STICKY_NOTE_STROKE_PICKS,
};
use excali_core::element::{Element as SceneElement, ElementBase, ElementKind};
use excali_scene::shape::Theme;
use excali_ui::color_picker::{
    color_ghost, color_picker, color_picker_text, color_top_picks_update, effective_top_picks,
    get_color_name_and_shade, initial_active_shade, is_same_color, picker_custom_colors,
    ColorPickerEvent, ColorPickerProps, ColorPickerType, ColorTopPicksSlot, HexInputState,
    StylesPanelMode,
};
use excali_ui::dom::{Element, Node};
use excali_ui::top_picks_dnd::{
    get_top_pick_reorder_offset, ghost_element, DragOrigin, PointerDown, Rect, StripLayout,
    TopPicksDnd, TopPicksDragState, BODY_CLASS, DRAG_THRESHOLD, DRAG_TIME_THRESHOLD_MS,
    GHOST_CLASS, TOP_PICKS_DND_CSS,
};
use serde_json::{json, Map, Value};

fn fixture() -> &'static Value {
    use std::sync::OnceLock;
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/color-top-picks-dnd.json")).unwrap()
    })
}

// -- helpers ----------------------------------------------------------------------

fn palette(name: &str) -> &'static [PaletteEntry] {
    match name {
        "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE" => &DEFAULT_ELEMENT_STROKE_COLOR_PALETTE,
        "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE" => &DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE,
        other => panic!("palette {other}"),
    }
}

fn picks(name: &str) -> [&'static str; 5] {
    match name {
        "DEFAULT_ELEMENT_STROKE_PICKS" => DEFAULT_ELEMENT_STROKE_PICKS,
        "DEFAULT_ELEMENT_BACKGROUND_PICKS" => DEFAULT_ELEMENT_BACKGROUND_PICKS,
        "BUCKET_FILL_BACKGROUND_PICKS" => BUCKET_FILL_BACKGROUND_PICKS,
        "STICKY_NOTE_STROKE_PICKS" => STICKY_NOTE_STROKE_PICKS,
        other => panic!("picks {other}"),
    }
}

fn slot(name: &str) -> ColorTopPicksSlot {
    ColorTopPicksSlot::ALL
        .into_iter()
        .find(|s| s.as_str() == name)
        .unwrap_or_else(|| panic!("slot {name}"))
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn scene_elements(v: &Value) -> Vec<SceneElement> {
    v.as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let mut base = ElementBase::new(format!("e{i}"), 0.0, 0.0, 1.0, 0.0);
            base.stroke_color = e["strokeColor"].as_str().unwrap().into();
            base.background_color = e["backgroundColor"].as_str().unwrap().into();
            base.is_deleted = e["isDeleted"].as_bool().unwrap();
            SceneElement::new(base, ElementKind::Rectangle)
        })
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
    let slots = (0..5)
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

/// The picker's props for a fixture case (its `customizableTopPicks` slot
/// and app state).
fn props(case: &Value) -> ColorPickerProps {
    let ty = match case["type"].as_str().unwrap() {
        "elementStroke" => ColorPickerType::ElementStroke,
        "elementBackground" => ColorPickerType::ElementBackground,
        other => panic!("type {other}"),
    };
    let p = palette(case["palette"].as_str().unwrap());
    let color = case["color"].as_str().map(str::to_owned);
    let elements = scene_elements(&case["elements"]);
    let customizable = case["customizableTopPicks"].as_str().map(slot);
    let color_top_picks = customizable
        .map(|s| strs(&case["colorTopPicks"][s.as_str()]))
        .unwrap_or_default();
    ColorPickerProps {
        ty,
        color: color.clone(),
        label: color_picker_text(match ty {
            ColorPickerType::ElementStroke => "labels.stroke",
            _ => "labels.background",
        })
        .into(),
        palette: Some(p),
        top_picks: Some(picks(case["topPicks"].as_str().unwrap())),
        excluded_colors: strs(&case["excludedColors"]),
        theme: if case["theme"] == "dark" {
            Theme::Dark
        } else {
            Theme::Light
        },
        open: case["open"].as_bool().unwrap(),
        mode: match case["mode"].as_str().unwrap() {
            "full" => StylesPanelMode::Full,
            "compact" => StylesPanelMode::Compact,
            other => panic!("mode {other}"),
        },
        phone: false,
        is_darwin: false,
        custom_colors: picker_custom_colors(ty, &elements, p),
        section: None,
        eye_dropper_active: false,
        hex: HexInputState::for_color(color.as_deref().unwrap_or("")),
        popup_id: "radix-1".into(),
        on_event: None,
        customizable_top_picks: customizable,
        color_top_picks,
        top_picks_menu: None,
        dnd: None,
    }
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

fn class_of(el: &Element) -> &str {
    el.attribute("class").unwrap_or("")
}

fn has_class(el: &Element, class: &str) -> bool {
    class_of(el).split(' ').any(|c| c == class)
}

fn listens(el: &Element, event: &str) -> bool {
    el.listened_events().any(|e| e == event)
}

// -- helpers of colorTopPicksDnD.ts and topPicksDnD.tsx -------------------------

#[test]
fn is_same_color_is_upstreams() {
    for case in fixture()["isSameColor"].as_array().unwrap() {
        let (a, b) = (case["a"].as_str().unwrap(), case["b"].as_str().unwrap());
        assert_eq!(
            is_same_color(a, b),
            case["same"].as_bool().unwrap(),
            "{a} {b}"
        );
    }
}

#[test]
fn the_reorder_preview_offsets_are_upstreams() {
    for case in fixture()["reorderOffset"].as_array().unwrap() {
        let s = &case["state"];
        let state = (!s.is_null()).then(|| TopPicksDragState {
            value: "#000000".to_string(),
            origin: match s["origin"]["kind"].as_str().unwrap() {
                "source" => DragOrigin::Source,
                _ => DragOrigin::Pick(s["origin"]["index"].as_u64().unwrap() as usize),
            },
            over_index: s["overIndex"].as_u64().map(|i| i as usize),
            duplicate_index: None,
            slot_span: s["slotSpan"].as_f64().unwrap(),
        });
        let offsets: Vec<f64> = (0..5)
            .map(|i| get_top_pick_reorder_offset(state.as_ref(), i))
            .collect();
        let expected: Vec<f64> = case["offsets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        assert_eq!(offsets, expected, "{s}");
    }
}

#[test]
fn the_constants_are_upstreams() {
    // topPicksDnD.tsx:17-22
    assert_eq!(DRAG_THRESHOLD, 10.0);
    assert_eq!(DRAG_TIME_THRESHOLD_MS, 100.0);
    assert_eq!(GHOST_CLASS, "excalidraw-top-picks-dnd-ghost");
    assert_eq!(BODY_CLASS, "excalidraw-top-picks-dnd-active");
}

#[test]
fn the_locale_strings_are_upstreams() {
    for (key, value) in fixture()["locale"].as_object().unwrap() {
        assert_eq!(color_picker_text(key), value.as_str().unwrap(), "{key}");
    }
}

#[test]
fn the_stylesheet_is_upstreams() {
    let css = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/top_picks_dnd/top_picks_dnd.css"
    ))
    .unwrap();
    assert_eq!(TOP_PICKS_DND_CSS, css);
    for selector in [
        ".top-picks-dnd__outline",
        ".top-picks-dnd__pick.is-dnd-source",
        ".top-picks-dnd__tip-reset",
        ".top-picks-dnd__context-menu-item",
        ".excalidraw-top-picks-dnd-ghost--dropping",
        "body.excalidraw-top-picks-dnd-active",
    ] {
        assert!(css.contains(selector), "{selector}");
    }
}

// -- DOM ----------------------------------------------------------------------------

#[test]
fn every_case_renders_upstreams_dom() {
    let cases = fixture()["cases"].as_array().unwrap();
    assert!(cases.len() >= 12, "{} cases", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let expected: Vec<Value> = case["dom"].as_array().unwrap().iter().map(expand).collect();
        let actual: Vec<Value> = color_picker(&props(case)).iter().map(tree).collect();
        assert_same(&format!("case {name}"), &json!(expected), &json!(actual));
    }
}

#[test]
fn the_effective_top_picks_are_the_custom_ones_when_any() {
    // ColorPicker.tsx:370-385
    for case in fixture()["cases"].as_array().unwrap() {
        let p = props(case);
        let customized = case["customizableTopPicks"]
            .as_str()
            .map(|s| strs(&case["colorTopPicks"][s]))
            .filter(|v| !v.is_empty() && case["mode"] == "full");
        let expected = customized.unwrap_or_else(|| {
            picks(case["topPicks"].as_str().unwrap())
                .iter()
                .map(|s| s.to_string())
                .collect()
        });
        assert_eq!(effective_top_picks(&p), expected, "{}", case["name"]);
    }
}

#[test]
fn the_swatches_start_drags_only_while_customizable() {
    let case = |name: &str| {
        fixture()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap()
    };
    let draggable = |nodes: &[Node]| -> Vec<String> {
        elements(nodes)
            .into_iter()
            .filter(|e| listens(e, "pointerdown"))
            .map(|e| class_of(e).to_owned())
            .collect()
    };
    // the strip's picks, the trigger, the custom colours, the palette and
    // the shades (TopPicks.tsx:115-119, ColorPicker.tsx:333-337,
    // CustomColorList.tsx:62-64, PickerColorList.tsx:110-112,
    // ShadeList.tsx:91-93)
    let open = color_picker(&props(case("open-stroke-customized")));
    let classes = draggable(&open);
    let count = |c: &str| classes.iter().filter(|x| x.contains(c)).count();
    assert_eq!(count("top-picks-dnd__pick"), 5, "{classes:?}");
    assert_eq!(count("active-color"), 1, "{classes:?}");
    assert_eq!(count("color-picker__button--large"), 2 + 15, "{classes:?}");
    // the strip opens its context menu
    let strip = elements(&open)
        .into_iter()
        .find(|e| has_class(e, "top-picks-dnd"))
        .unwrap();
    assert!(listens(strip, "contextmenu"));
    // the tip's reset link keeps the focus off itself and Enter to itself
    let reset = elements(&open)
        .into_iter()
        .find(|e| has_class(e, "top-picks-dnd__tip-reset"))
        .unwrap();
    for ev in ["mousedown", "click", "keydown"] {
        assert!(listens(reset, ev), "{ev}");
    }
    for name in ["open-compact-stroke", "open-stroke-not-customizable"] {
        let nodes = color_picker(&props(case(name)));
        assert_eq!(draggable(&nodes), Vec::<String>::new(), "{name}");
        assert!(
            elements(&nodes).iter().all(|e| !listens(e, "contextmenu")),
            "{name}"
        );
    }
}

/// The host applies an event the way upstream's `updateData` does: the
/// `colorTopPicks` patch it records, and the props it renders next.
fn apply(
    p: &mut ColorPickerProps,
    top_picks: &mut Map<String, Value>,
    event: &ColorPickerEvent,
) -> Option<Value> {
    let slot = p.customizable_top_picks?;
    let picks = match event {
        ColorPickerEvent::TopPicksChange(picks) => Some(picks.as_slice()),
        ColorPickerEvent::ResetTopPicks => None,
        _ => return None,
    };
    let patch = color_top_picks_update(top_picks, slot, picks);
    *top_picks = patch["colorTopPicks"].as_object().unwrap().clone();
    p.color_top_picks = strs(&top_picks[slot.as_str()]);
    Some(json!({ "updateData": patch }))
}

#[test]
fn the_context_menu_resets_the_strip() {
    for case in fixture()["menus"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut p = props(case);
        let mut top_picks = case["colorTopPicks"].as_object().unwrap().clone();
        let closed: Vec<Node> = color_picker(&p);
        let strip = elements(&closed)
            .into_iter()
            .find(|e| has_class(e, "top-picks-dnd"))
            .unwrap();
        assert!(listens(strip, "contextmenu"), "{name}");
        // right click at (60, 32): the host opens the menu there
        p.top_picks_menu = Some((60, 32));
        let opened: Vec<Value> = color_picker(&p).iter().map(tree).collect();
        let expected: Vec<Value> = case["opened"]
            .as_array()
            .unwrap()
            .iter()
            .map(expand)
            .collect();
        assert_same(
            &format!("menu {name} opened"),
            &json!(expected),
            &json!(opened),
        );
        // the item: disabled unless customized, else a reset
        let nodes = color_picker(&p);
        let item = elements(&nodes)
            .into_iter()
            .find(|e| has_class(e, "top-picks-dnd__context-menu-item"))
            .unwrap();
        let customized = !p.color_top_picks.is_empty();
        assert_eq!(
            item.attribute("data-disabled").is_some(),
            !customized,
            "{name}"
        );
        assert_eq!(listens(item, "click"), customized, "{name}");
        let mut calls = Vec::new();
        if customized {
            calls.extend(apply(
                &mut p,
                &mut top_picks,
                &ColorPickerEvent::ResetTopPicks,
            ));
            p.top_picks_menu = None;
        }
        assert_eq!(json!(calls), case["calls"], "menu {name} calls");
        if customized {
            let after: Vec<Value> = color_picker(&p).iter().map(tree).collect();
            let expected: Vec<Value> = case["after"]
                .as_array()
                .unwrap()
                .iter()
                .map(expand)
                .collect();
            assert_same(
                &format!("menu {name} after"),
                &json!(expected),
                &json!(after),
            );
        }
    }
}

#[test]
fn the_tips_reset_link_resets_the_strip() {
    for case in fixture()["tips"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut p = props(case);
        let mut top_picks = case["colorTopPicks"].as_object().unwrap().clone();
        let calls: Vec<Value> = apply(&mut p, &mut top_picks, &ColorPickerEvent::ResetTopPicks)
            .into_iter()
            .collect();
        assert_eq!(json!(calls), case["calls"], "{name}");
        let after: Vec<Value> = color_picker(&p).iter().map(tree).collect();
        let expected: Vec<Value> = case["after"]
            .as_array()
            .unwrap()
            .iter()
            .map(expand)
            .collect();
        assert_same(
            &format!("tip {name} after"),
            &json!(expected),
            &json!(after),
        );
    }
}

// -- drags ----------------------------------------------------------------------------

/// The colour a swatch of the fixture's selectors drags, from the picker's
/// own state: a palette entry at the active shade, the first most-used
/// custom colour, a shade of the picked hue, the active colour.
fn source_value(p: &ColorPickerProps, selector: &str) -> Option<String> {
    let palette = p.palette.unwrap();
    if let Some(key) = selector
        .strip_prefix("[data-testid=\"color-")
        .and_then(|s| s.strip_suffix("\"]"))
    {
        let obj = get_color_name_and_shade(palette, p.color.as_deref());
        let shade = initial_active_shade(p.ty, obj.as_ref());
        let entry = palette.iter().find(|e| e.0 == key).unwrap();
        return Some(match entry.1 {
            PaletteColor::Shades(s) => s[shade].to_owned(),
            PaletteColor::Single(c) => c.to_owned(),
        });
    }
    match selector {
        ".active-color" => p.color.clone(),
        ".color-picker-content--default > button" => Some(p.custom_colors[0].clone()),
        ".color-picker-content--default.shades > button:nth-child(4)" => {
            let obj = get_color_name_and_shade(palette, p.color.as_deref()).unwrap();
            let entry = palette.iter().find(|e| e.0 == obj.color_name).unwrap();
            match entry.1 {
                PaletteColor::Shades(s) => Some(s[3].to_owned()),
                PaletteColor::Single(_) => unreachable!(),
            }
        }
        other => panic!("selector {other}"),
    }
}

/// The element of the port's DOM a fixture selector names.
fn find_source<'a>(nodes: &'a [Node], selector: &str) -> Option<&'a Element> {
    let all = elements(nodes);
    if let Some(i) = selector
        .strip_prefix("[data-top-pick-index=\"")
        .and_then(|s| s.strip_suffix("\"]"))
    {
        return all
            .into_iter()
            .find(|e| e.attribute("data-top-pick-index") == Some(i));
    }
    if let Some(id) = selector
        .strip_prefix("[data-testid=\"")
        .and_then(|s| s.strip_suffix("\"]"))
    {
        return all
            .into_iter()
            .find(|e| e.attribute("data-testid") == Some(id));
    }
    let children_of = |pred: &dyn Fn(&Element) -> bool| -> Vec<&'a Element> {
        elements(nodes)
            .into_iter()
            .filter(|e| pred(e))
            .flat_map(|e| e.children().iter().filter_map(|c| c.as_element()))
            .collect()
    };
    match selector {
        ".active-color" => all.into_iter().find(|e| has_class(e, "active-color")),
        ".color-picker-content--default > button" => {
            children_of(&|e| has_class(e, "color-picker-content--default"))
                .into_iter()
                .find(|e| e.tag() == "button")
        }
        ".color-picker-content--default.shades > button:nth-child(4)" => children_of(&|e| {
            has_class(e, "color-picker-content--default") && has_class(e, "shades")
        })
        .into_iter()
        .nth(3),
        other => panic!("selector {other}"),
    }
}

/// The strip as upstream's fixture holds it, without the inline
/// `transform: none` that `settleStripInstantly` leaves on the picks React
/// kept (the port renders the strip afresh, where no transform is none).
fn settled(strip: &Value) -> Value {
    let mut strip = strip.clone();
    if let Some(children) = strip["children"].as_array_mut() {
        for c in children {
            if let Some(style) = c["style"].as_object_mut() {
                if style.get("transform") == Some(&json!("none")) {
                    style.remove("transform");
                }
            }
        }
    }
    strip
}

/// jsdom's computed `background-color` of every swatch button, which
/// createColorGhost samples (`colorTopPicksDnD.ts:32-37`).
const JSDOM_BUTTON_BACKGROUND: &str = "ButtonFace";

#[test]
fn every_drag_replays_upstreams_steps() {
    let drags = fixture()["drags"].as_array().unwrap();
    assert!(drags.len() >= 25, "{} drags", drags.len());
    let (strip_layout, other) = layout();
    for drag in drags {
        let name = drag["name"].as_str().unwrap();
        let mut p = props(drag);
        let mut top_picks = drag["colorTopPicks"].as_object().unwrap().clone();
        let dnd: TopPicksDnd<String> = TopPicksDnd::new(is_same_color);
        let customizable = p.customizable_top_picks.is_some() && p.mode == StylesPanelMode::Full;
        dnd.set_enabled(customizable);
        dnd.set_picks(effective_top_picks(&p));
        p.dnd = Some(dnd.clone());
        let strip_rendered = p.mode == StylesPanelMode::Full;
        let measure = || strip_rendered.then(|| strip_layout.clone());
        let mut now = 0.0;
        let mut target: Option<(String, Option<String>)> = None;
        let mut contents: BTreeMap<u64, Element> = BTreeMap::new();
        for (i, record) in drag["steps"].as_array().unwrap().iter().enumerate() {
            let step = &record["step"];
            let at = format!("drag {name} step {i} {step}");
            let pointer_id = step["pointerId"].as_i64().unwrap_or(1) as i32;
            let mut calls: Vec<Value> = Vec::new();
            let xy = |v: &Value| (v[0].as_f64().unwrap(), v[1].as_f64().unwrap());
            if let Some(on) = step.get("down") {
                let (selector, origin) = match on.get("pick") {
                    Some(i) => {
                        let i = i.as_u64().unwrap() as usize;
                        (
                            format!("[data-top-pick-index=\"{i}\"]"),
                            DragOrigin::Pick(i),
                        )
                    }
                    None => (
                        on["source"].as_str().unwrap().to_owned(),
                        DragOrigin::Source,
                    ),
                };
                let value = match origin {
                    DragOrigin::Pick(i) => Some(effective_top_picks(&p)[i].clone()),
                    DragOrigin::Source => source_value(&p, &selector),
                };
                let nodes = color_picker(&p);
                let el =
                    find_source(&nodes, &selector).unwrap_or_else(|| panic!("{at}: no {selector}"));
                let home = match origin {
                    DragOrigin::Pick(i) => strip_layout.slots[i],
                    DragOrigin::Source => other,
                };
                let session = if listens(el, "pointerdown") {
                    dnd.pointer_down(
                        PointerDown {
                            pointer_id,
                            button: step["button"].as_i64().unwrap_or(0) as i16,
                            x: step["x"].as_f64().unwrap(),
                            y: step["y"].as_f64().unwrap(),
                            now,
                            value: value.clone(),
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
                    let v = value.clone().unwrap();
                    assert_eq!(json!(v), expected["value"], "{at}: value");
                    contents.insert(id, color_ghost(&v, Some(JSDOM_BUTTON_BACKGROUND)));
                }
                if pointer_id == 1 {
                    target = Some((selector, value));
                }
            } else if let Some(ms) = step["wait"].as_f64() {
                now += ms;
                dnd.advance(now, &measure);
            } else if step.get("move").is_some() {
                let (x, y) = xy(&step["move"]);
                dnd.pointer_move(pointer_id, x, y, now, &measure);
            } else if step.get("up").is_some() {
                let outcome = dnd.pointer_up(pointer_id, now);
                if let Some(picks) = outcome.picks {
                    let event = ColorPickerEvent::TopPicksChange(picks);
                    calls.extend(apply(&mut p, &mut top_picks, &event));
                    dnd.set_picks(effective_top_picks(&p));
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
                let (selector, value) = target.clone().unwrap();
                if reached && selector != ".active-color" {
                    calls.push(json!({ "onChange": value }));
                }
            } else {
                panic!("{at}: unknown step");
            }
            assert_eq!(now, record["now"].as_f64().unwrap(), "{at}: clock");
            assert_eq!(json!(calls), record["calls"], "{at}: calls");
            let nodes = color_picker(&p);
            let strip = elements(&nodes)
                .into_iter()
                .find(|e| has_class(e, "color-picker__top-picks"))
                .map(|e| tree(&Node::Element(e.clone())));
            let expected = (!record["strip"].is_null()).then(|| settled(&record["strip"]));
            assert_same(&format!("{at}: strip"), &json!(expected), &json!(strip));
            let ghosts: Vec<Value> = dnd
                .ghosts()
                .iter()
                .map(|g| tree(&Node::Element(ghost_element(g, contents[&g.id].clone()))))
                .collect();
            assert_same(&format!("{at}: ghosts"), &record["ghosts"], &json!(ghosts));
            assert_eq!(json!(dnd.body_active()), record["bodyActive"], "{at}: body");
        }
    }
}

#[test]
fn a_drop_pins_reorders_and_refuses_duplicates() {
    // the acceptance criteria, stated on the recorded drags
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
            .filter(|c| c.get("updateData").is_some())
            .collect()
    };
    // a palette colour onto slot 1 replaces it
    assert_eq!(
        pinned("pin-palette-colour"),
        vec![
            json!({"updateData": {"colorTopPicks": {"elementStroke": ["#1e1e1e", "#6741d9", "#2f9e44", "#1971c2", "#f08c00"]}}})
        ]
    );
    // the active colour onto slot 4
    assert_eq!(
        pinned("pin-active-colour"),
        vec![
            json!({"updateData": {"colorTopPicks": {"elementStroke": ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#123456"]}}})
        ]
    );
    // a pick moved from 0 to 3
    assert_eq!(
        pinned("reorder-forward"),
        vec![
            json!({"updateData": {"colorTopPicks": {"elementStroke": ["#e03131", "#2f9e44", "#1971c2", "#1e1e1e", "#f08c00"]}}})
        ]
    );
    // an already pinned colour, in any notation, is refused
    assert!(pinned("pin-duplicate").is_empty());
    assert!(pinned("pin-duplicate-other-notation").is_empty());
    assert!(pinned("escape-cancels").is_empty());
}

#[test]
fn a_ghost_is_the_colour_it_carries() {
    // createColorGhost (colorTopPicksDnD.ts:23-40)
    let html = |e: Element| Node::Element(e).to_html();
    assert_eq!(
        html(color_ghost("#e03131", Some("rgb(224, 49, 49)"))),
        r#"<div class="excalidraw-color-dnd-ghost-swatch" style="background-color: rgb(224, 49, 49);"></div>"#
    );
    assert_eq!(
        html(color_ghost("#e03131", Some("rgba(0, 0, 0, 0)"))),
        r#"<div class="excalidraw-color-dnd-ghost-swatch" style="background-color: #e03131;"></div>"#
    );
    assert_eq!(
        html(color_ghost("#e03131", None)),
        r#"<div class="excalidraw-color-dnd-ghost-swatch" style="background-color: #e03131;"></div>"#
    );
    assert_eq!(
        html(color_ghost("transparent", Some("red"))),
        r#"<div class="excalidraw-color-dnd-ghost-swatch is-transparent"></div>"#
    );
}
