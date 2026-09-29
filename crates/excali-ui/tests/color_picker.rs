//! The colour picker (ex-523): upstream's `ColorPicker`
//! (`components/ColorPicker/*`): the top-picks strip, the trigger swatch,
//! and the popup's most-used custom colours, 5×3 palette, shades, hex input
//! and eye-dropper trigger; its keyboard map (`keyboardNavHandlers.ts`:
//! q…b, 1–5, Shift+1–5, i, Alt, Escape, Tab, arrows); the palettes of
//! `packages/common/src/colors.ts`; and the eye dropper's cursor and
//! preview placement.
//!
//! Fixture: `tests/fixtures/color-picker.json`, written by
//! `tools/goldens/color-picker.mjs` from upstream at the pinned commit
//! (React 19.0.0, radix-ui 1.4.3, jsdom 22.1.0). `src/color_picker/
//! color_picker.css` is the same generator's ColorPicker.scss and
//! EyeDropper.scss.

use std::collections::BTreeMap;

use excali_core::color::{
    get_all_colors_specific_shade, is_color_dark, normalize_input_color, PaletteColor,
    PaletteEntry, BUCKET_FILL_BACKGROUND_PICKS, COLORS_PER_ROW, COLOR_OUTLINE_CONTRAST_THRESHOLD,
    COLOR_PALETTE, DEFAULT_CANVAS_BACKGROUND_PICKS, DEFAULT_ELEMENT_BACKGROUND_COLOR_INDEX,
    DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE, DEFAULT_ELEMENT_BACKGROUND_PICKS,
    DEFAULT_ELEMENT_STROKE_COLOR_INDEX, DEFAULT_ELEMENT_STROKE_COLOR_PALETTE,
    DEFAULT_ELEMENT_STROKE_PICKS, MAX_CUSTOM_COLORS_USED_IN_CANVAS, STICKY_NOTE_BACKGROUND_PICKS,
    STICKY_NOTE_STROKE_PICKS,
};
use excali_core::constants::{COLOR_TOP_PICKS_SLOTS, DEFAULT_STICKY_NOTE_BG};
use excali_core::element::{Element as SceneElement, ElementBase, ElementKind};
use excali_scene::shape::Theme;
use excali_ui::color_picker::{
    change_hex_input, close_popup, color_picker, color_picker_key_nav_handler,
    color_picker_text, escape, eye_dropper_cursor, get_color_name_and_shade,
    get_most_used_custom_colors, hex_input_value, initial_active_shade, initial_section,
    is_custom_color, picker_custom_colors, position_element_beside_cursor, toggle_eye_dropper,
    toggle_popup, top_pick_follows_focus, ColorPickerProps, ColorPickerType, ContainerRect,
    EscapeOutcome, EyeDropperState, HexInputState, KeyInput, KeyNavEffect, KeyNavState, Section,
    StylesPanelMode, COLOR_PICKER_CSS, COLOR_PICKER_HOTKEY_BINDINGS,
};
use excali_ui::dom::Node;
use serde_json::{json, Map, Value};

fn fixture() -> &'static Value {
    use std::sync::OnceLock;
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| serde_json::from_str(include_str!("fixtures/color-picker.json")).unwrap())
}

fn palette(name: &str) -> &'static [PaletteEntry] {
    static FULL: std::sync::OnceLock<[PaletteEntry; 15]> = std::sync::OnceLock::new();
    match name {
        "COLOR_PALETTE" => FULL.get_or_init(|| COLOR_PALETTE.entries()),
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
        "STICKY_NOTE_BACKGROUND_PICKS" => STICKY_NOTE_BACKGROUND_PICKS,
        "DEFAULT_CANVAS_BACKGROUND_PICKS" => DEFAULT_CANVAS_BACKGROUND_PICKS,
        other => panic!("picks {other}"),
    }
}

fn picker_type(name: &str) -> ColorPickerType {
    [
        ColorPickerType::CanvasBackground,
        ColorPickerType::ElementBackground,
        ColorPickerType::ElementStroke,
    ]
    .into_iter()
    .find(|t| t.as_str() == name)
    .unwrap_or_else(|| panic!("type {name}"))
}

fn section(v: &Value) -> Option<Section> {
    v.as_str().map(|s| {
        [
            Section::Custom,
            Section::BaseColors,
            Section::Shades,
            Section::Hex,
        ]
        .into_iter()
        .find(|x| x.as_str() == s)
        .unwrap_or_else(|| panic!("section {s}"))
    })
}

fn entries_json(p: &[PaletteEntry]) -> Value {
    Value::Array(
        p.iter()
            .map(|(name, value)| {
                let value = match value {
                    PaletteColor::Single(c) => json!(c),
                    PaletteColor::Shades(s) => json!(s),
                };
                json!({"name": name, "value": value})
            })
            .collect(),
    )
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_owned())
        .collect()
}

// -- colors.ts ------------------------------------------------------------------

#[test]
fn the_palettes_and_picks_are_colors_ts() {
    let p = &fixture()["palettes"];
    assert_eq!(entries_json(&COLOR_PALETTE.entries()), p["COLOR_PALETTE"]);
    for name in [
        "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE",
        "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE",
    ] {
        assert_eq!(entries_json(palette(name)), p[name], "{name}");
    }
    for name in [
        "DEFAULT_ELEMENT_STROKE_PICKS",
        "DEFAULT_ELEMENT_BACKGROUND_PICKS",
        "BUCKET_FILL_BACKGROUND_PICKS",
        "STICKY_NOTE_STROKE_PICKS",
        "STICKY_NOTE_BACKGROUND_PICKS",
        "DEFAULT_CANVAS_BACKGROUND_PICKS",
    ] {
        assert_eq!(json!(picks(name)), p[name], "{name}");
    }
    assert_eq!(json!(COLOR_TOP_PICKS_SLOTS), p["COLOR_TOP_PICKS_SLOTS"]);
    assert_eq!(json!(COLORS_PER_ROW), p["COLORS_PER_ROW"]);
    assert_eq!(
        json!(MAX_CUSTOM_COLORS_USED_IN_CANVAS),
        p["MAX_CUSTOM_COLORS_USED_IN_CANVAS"]
    );
    assert_eq!(
        json!(DEFAULT_ELEMENT_STROKE_COLOR_INDEX),
        p["DEFAULT_ELEMENT_STROKE_COLOR_INDEX"]
    );
    assert_eq!(
        json!(DEFAULT_ELEMENT_BACKGROUND_COLOR_INDEX),
        p["DEFAULT_ELEMENT_BACKGROUND_COLOR_INDEX"]
    );
    assert_eq!(
        json!(COLOR_OUTLINE_CONTRAST_THRESHOLD),
        p["COLOR_OUTLINE_CONTRAST_THRESHOLD"]
    );
    assert_eq!(json!(DEFAULT_STICKY_NOTE_BG), p["DEFAULT_STICKY_NOTE_BG"]);
    let shades: Vec<Value> = (0..5)
        .map(|i| json!(get_all_colors_specific_shade(i)))
        .collect();
    assert_eq!(Value::Array(shades), p["allColorsSpecificShade"]);
    assert_eq!(
        json!(COLOR_PICKER_HOTKEY_BINDINGS),
        p["colorPickerHotkeyBindings"]
    );
}

#[test]
fn the_palette_grid_is_5_by_3_with_hotkeys_q_to_b() {
    // research/ui-design-system.md §2 and §3.12: row 1 transparent, white,
    // gray, black, bronze; rows 2-3 cyan … pink / green … red; keys
    // q w e r t / a s d f g / z x c v b (colorPickerUtils.ts:38-42).
    let names: Vec<&str> = DEFAULT_ELEMENT_STROKE_COLOR_PALETTE
        .iter()
        .map(|e| e.0)
        .collect();
    assert_eq!(
        names,
        [
            "transparent",
            "white",
            "gray",
            "black",
            "bronze",
            "cyan",
            "blue",
            "violet",
            "grape",
            "pink",
            "green",
            "teal",
            "yellow",
            "orange",
            "red"
        ]
    );
    assert_eq!(names.len(), COLORS_PER_ROW * 3);
    assert_eq!(
        COLOR_PICKER_HOTKEY_BINDINGS.join(""),
        "qwertasdfgzxcvb"
    );
}

#[test]
fn is_color_dark_is_upstreams() {
    let cases = fixture()["isColorDark"].as_array().unwrap();
    assert!(cases.len() > 100);
    for c in cases {
        let color = c["color"].as_str().unwrap();
        let threshold = c["threshold"].as_f64();
        assert_eq!(
            is_color_dark(color, threshold),
            c["dark"].as_bool().unwrap(),
            "isColorDark({color:?}, {threshold:?})"
        );
    }
}

#[test]
fn normalize_input_color_is_upstreams() {
    for c in fixture()["normalizeInputColor"].as_array().unwrap() {
        let input = c["input"].as_str().unwrap();
        assert_eq!(
            normalize_input_color(input).map(Value::String),
            c["output"].as_str().map(|s| Value::String(s.into())),
            "normalizeInputColor({input:?})"
        );
    }
}

// -- colorPickerUtils.ts --------------------------------------------------------

#[test]
fn color_name_and_shade_is_upstreams() {
    for c in fixture()["colorNameAndShade"].as_array().unwrap() {
        let p = palette(c["palette"].as_str().unwrap());
        let color = c["color"].as_str();
        let actual = get_color_name_and_shade(p, color)
            .map(|o| json!({"colorName": o.color_name, "shade": o.shade}));
        assert_eq!(
            actual.unwrap_or(Value::Null),
            c["result"],
            "{color:?} in {}",
            c["palette"]
        );
        if let Some(color) = color {
            assert_eq!(
                json!(is_custom_color(color, p)),
                c["isCustom"],
                "isCustomColor {color:?}"
            );
        }
    }
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

#[test]
fn most_used_custom_colors_are_upstreams() {
    let cases = fixture()["mostUsedCustomColors"].as_array().unwrap();
    assert!(cases.len() >= 20);
    for c in cases {
        let elements = scene_elements(&c["elements"]);
        let ty = picker_type(c["type"].as_str().unwrap());
        let p = palette(c["palette"].as_str().unwrap());
        assert_eq!(
            json!(get_most_used_custom_colors(&elements, ty, p)),
            c["result"],
            "{} {} {}",
            c["set"],
            c["type"],
            c["palette"]
        );
    }
}

// -- keyboardNavHandlers.ts -----------------------------------------------------

fn key_palette(picker: &str) -> (&'static [PaletteEntry], Vec<&'static str>) {
    match picker {
        "stroke" => (&DEFAULT_ELEMENT_STROKE_COLOR_PALETTE, vec![]),
        "background" => (&DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE, vec![]),
        "sticky" => (&DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE, vec!["transparent"]),
        "full" => (palette("COLOR_PALETTE"), vec![]),
        other => panic!("picker {other}"),
    }
}

fn picker_ty(picker: &str) -> ColorPickerType {
    match picker {
        "stroke" => ColorPickerType::ElementStroke,
        "full" => ColorPickerType::CanvasBackground,
        _ => ColorPickerType::ElementBackground,
    }
}

#[test]
fn the_keyboard_map_is_upstreams() {
    let cases = fixture()["keyNav"].as_array().unwrap();
    assert!(cases.len() > 2000, "{} cases", cases.len());
    let custom: Vec<String> = ["#123456", "#abcdef", "#fedcba"]
        .map(String::from)
        .to_vec();
    for c in cases {
        let picker = c["picker"].as_str().unwrap();
        let (p, excluded) = key_palette(picker);
        let color = c["color"].as_str();
        let customs: &[String] = if c["custom"].as_bool().unwrap() {
            &custom
        } else {
            &[]
        };
        let mods = c
            .get("mods")
            .map(strs)
            .unwrap_or_default();
        let has = |m: &str| mods.iter().any(|x| x == m);
        let event = KeyInput {
            key: c["key"].as_str().unwrap().into(),
            code: c["code"].as_str().unwrap().into(),
            shift: has("shift"),
            // jsdom's navigator.platform is not a Mac's: Ctrl
            ctrl_or_cmd: has("ctrl"),
        };
        let obj = get_color_name_and_shade(p, color);
        let active_shade = initial_active_shade(picker_ty(picker), obj.as_ref());
        assert_eq!(json!(active_shade), c["activeShade"]);
        let state = KeyNavState {
            section: section(&c["section"]),
            palette: p,
            color,
            custom_colors: customs,
            active_shade,
            excluded_colors: &excluded,
        };
        let out = color_picker_key_nav_handler(&event, &state);
        let mut changes = Vec::new();
        let mut sections = Vec::new();
        let mut eye = Vec::new();
        let mut escapes = 0;
        for e in &out.effects {
            match e {
                KeyNavEffect::Change(c) => changes.push(json!(c)),
                KeyNavEffect::SetSection(s) => sections.push(json!(s.as_str())),
                KeyNavEffect::EyeDropperToggle(f) => eye.push(json!(f)),
                KeyNavEffect::Escape => escapes += 1,
            }
        }
        let what = format!("{c}");
        assert_eq!(json!(out.handled), c["handled"], "{what}");
        let arr = |k: &str| c.get(k).cloned().unwrap_or(json!([]));
        assert_eq!(Value::Array(changes), arr("changes"), "{what}");
        assert_eq!(Value::Array(sections), arr("sections"), "{what}");
        assert_eq!(Value::Array(eye), arr("eyeDropper"), "{what}");
        assert_eq!(json!(escapes), c.get("escape").cloned().unwrap_or(json!(0)), "{what}");
        if c.get("prevented").is_some() {
            // the handler's own preventDefault (Tab) is on a handled key,
            // which the Picker prevents anyway (Picker.tsx:156-172)
            assert!(out.handled, "{what}");
        }
    }
}

#[test]
fn the_keyboard_map_covers_the_acceptance_keys() {
    let keys: std::collections::BTreeSet<String> = fixture()["keyNav"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["handled"] == json!(true))
        .map(|c| {
            let shift = c.get("mods").map(strs).unwrap_or_default().contains(&"shift".to_string());
            format!("{}{}", if shift { "Shift+" } else { "" }, c["code"].as_str().unwrap())
        })
        .collect();
    for k in [
        "KeyQ", "KeyT", "KeyA", "KeyG", "KeyZ", "KeyB", "Digit1", "Digit3", "Digit5",
        "Shift+Digit1", "Shift+Digit3", "Shift+Digit5", "KeyI", "AltLeft", "Escape", "Tab",
        "Shift+Tab", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown",
    ] {
        assert!(keys.contains(k), "{k} is never handled");
    }
}

#[test]
fn the_initial_section_follows_the_colour() {
    // Picker.tsx:103-122
    let p = &DEFAULT_ELEMENT_STROKE_COLOR_PALETTE[..];
    let customs = vec!["#123456".to_string()];
    assert_eq!(initial_section(Some("#123456"), p, &customs), Some(Section::Custom));
    assert_eq!(initial_section(Some("#777777"), p, &customs), None);
    assert_eq!(initial_section(Some("#e03131"), p, &customs), Some(Section::Shades));
    assert_eq!(initial_section(Some("#1e1e1e"), p, &customs), Some(Section::BaseColors));
    assert_eq!(initial_section(None, p, &customs), Some(Section::BaseColors));
}

// -- ColorInput.tsx ---------------------------------------------------------------

#[test]
fn the_hex_input_is_upstreams() {
    let cases = fixture()["hexInput"].as_array().unwrap();
    assert!(cases.len() >= 15);
    for c in cases {
        let typed = c["typed"].as_str().unwrap();
        assert_eq!(hex_input_value("#1e1e1e"), c["initial"].as_str().unwrap());
        let out = change_hex_input(typed);
        assert_eq!(json!(out.color.as_slice()), c["changes"], "{typed:?}");
        let after = &c["after"];
        assert_eq!(hex_input_value(&out.state.inner_value), after["value"].as_str().unwrap(), "{typed:?}");
        let invalid = out.state.error.is_some();
        assert_eq!(json!(invalid.to_string()), after["ariaInvalid"], "{typed:?}");
        assert_eq!(json!(invalid), after["hasError"], "{typed:?}");
        assert_eq!(
            json!(out.state.error.map(color_picker_text)),
            after["error"],
            "{typed:?}"
        );
        // a blur shows the colour again, without an error (ColorInput.tsx:94-97)
        let blurred = HexInputState::for_color("#1e1e1e");
        assert_eq!(hex_input_value(&blurred.inner_value), c["blurred"]["value"].as_str().unwrap());
        assert_eq!(blurred.error, None);
    }
}

// -- EyeDropper.tsx -------------------------------------------------------------

#[test]
fn the_eye_dropper_cursor_and_preview_placement_are_upstreams() {
    let e = &fixture()["eyeDropper"];
    assert_eq!(eye_dropper_cursor(), e["cursor"].as_str().unwrap());
    for c in e["positions"].as_array().unwrap() {
        let f = |v: &Value| v.as_f64().unwrap();
        let (left, top) = position_element_beside_cursor(
            (f(&c["cursor"]["x"]), f(&c["cursor"]["y"])),
            (f(&c["element"]["width"]), f(&c["element"]["height"])),
            ContainerRect {
                left: f(&c["container"]["left"]),
                top: f(&c["container"]["top"]),
                width: f(&c["container"]["width"]),
                height: f(&c["container"]["height"]),
            },
            f(&c["gap"]),
        );
        assert_eq!(json!({"left": left, "top": top}), c["result"], "{c}");
    }
}

#[test]
fn the_eye_dropper_toggles_as_the_popup_content_does() {
    // ColorPicker.tsx:181-203
    let ty = ColorPickerType::ElementStroke;
    let on = toggle_eye_dropper(None, None, ty);
    assert_eq!(on, Some(EyeDropperState { keep_open_on_alt: false, picker: ty }));
    assert_eq!(toggle_eye_dropper(on, None, ty), None);
    assert_eq!(toggle_eye_dropper(on, Some(false), ty), None);
    assert_eq!(toggle_eye_dropper(None, Some(false), ty), None);
    // Alt: open (or keep open) and stay open while Alt is held
    let alt = toggle_eye_dropper(None, Some(true), ty);
    assert_eq!(alt, Some(EyeDropperState { keep_open_on_alt: true, picker: ty }));
    assert_eq!(toggle_eye_dropper(on, Some(true), ty), alt);
    // Escape cancels the eye dropper first, then closes (ColorPicker.tsx:204-211)
    assert_eq!(escape(on), EscapeOutcome::CancelEyeDropper);
    assert_eq!(escape(None), EscapeOutcome::ClosePopup);
}

// -- ColorPicker.tsx: popup state ---------------------------------------------------

#[test]
fn the_trigger_toggles_and_switches_popups() {
    // ColorPicker.tsx:462-474
    let stroke = ColorPickerType::ElementStroke;
    assert_eq!(toggle_popup(None, stroke), Some("elementStroke".to_string()));
    assert_eq!(toggle_popup(Some("elementStroke"), stroke), None);
    assert_eq!(toggle_popup(Some("elementBackground"), stroke), Some("elementStroke".to_string()));
    assert_eq!(toggle_popup(Some("fontFamily"), stroke), Some("elementStroke".to_string()));
    // onClose clears only its own popup (ColorPicker.tsx:151-156)
    assert_eq!(close_popup(Some("elementStroke"), stroke), None);
    assert_eq!(close_popup(Some("elementBackground"), stroke), Some("elementBackground".to_string()));
    // a top pick follows the focus to its picker when another colour
    // picker's popup is open (ColorPicker.tsx:431-443)
    assert!(top_pick_follows_focus(Some("elementBackground"), stroke));
    assert!(!top_pick_follows_focus(Some("elementStroke"), stroke));
    assert!(!top_pick_follows_focus(Some("fontFamily"), stroke));
    assert!(!top_pick_follows_focus(None, stroke));
}

// -- DOM parity -------------------------------------------------------------------

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

fn props(case: &Value) -> ColorPickerProps {
    let ty = picker_type(case["type"].as_str().unwrap());
    let p = case["palette"].as_str().map(palette);
    let elements = scene_elements(&case["elements"]);
    let color = case["color"].as_str().map(str::to_owned);
    let label = color_picker_text(match ty {
        ColorPickerType::ElementStroke => "labels.stroke",
        ColorPickerType::ElementBackground => "labels.background",
        ColorPickerType::CanvasBackground => "labels.canvasBackground",
    });
    let custom_colors = p
        .map(|p| picker_custom_colors(ty, &elements, p))
        .unwrap_or_default();
    ColorPickerProps {
        ty,
        color: color.clone(),
        label: label.into(),
        palette: p,
        top_picks: case["topPicks"].as_str().map(picks),
        excluded_colors: strs(&case["excludedColors"]),
        theme: if case["theme"] == "dark" { Theme::Dark } else { Theme::Light },
        open: case["open"].as_bool().unwrap(),
        mode: match case["mode"].as_str().unwrap() {
            "full" => StylesPanelMode::Full,
            "compact" => StylesPanelMode::Compact,
            "mobile" => StylesPanelMode::Mobile,
            other => panic!("mode {other}"),
        },
        phone: case["formFactor"] == "phone",
        is_darwin: false,
        custom_colors,
        section: None,
        eye_dropper_active: false,
        hex: HexInputState::for_color(color.as_deref().unwrap_or("")),
        popup_id: "radix-1".into(),
        on_event: None,
    }
}

#[test]
fn every_case_renders_upstreams_dom() {
    let cases = fixture()["cases"].as_array().unwrap();
    assert!(cases.len() >= 30, "{} cases", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let expected: Vec<Value> = case["dom"].as_array().unwrap().iter().map(expand).collect();
        let actual: Vec<Value> = color_picker(&props(case)).iter().map(tree).collect();
        if actual != expected {
            panic!(
                "case {name}:\nexpected {}\nactual   {}",
                serde_json::to_string(&expected).unwrap(),
                serde_json::to_string(&actual).unwrap()
            );
        }
    }
}

#[test]
fn the_locale_strings_are_upstreams() {
    for (key, value) in fixture()["locale"].as_object().unwrap() {
        assert_eq!(color_picker_text(key), value.as_str().unwrap(), "{key}");
    }
}

fn walk<'a>(n: &'a Node, out: &mut Vec<&'a excali_ui::dom::Element>) {
    if let Node::Element(el) = n {
        out.push(el);
        for c in el.children() {
            walk(c, out);
        }
    }
}

fn open_props() -> ColorPickerProps {
    let case = fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "open-stroke-custom")
        .unwrap();
    props(case)
}

#[test]
fn the_controls_listen_for_their_events() {
    let nodes = color_picker(&open_props());
    let mut els = Vec::new();
    for n in &nodes {
        walk(n, &mut els);
    }
    let listens = |el: &excali_ui::dom::Element, ev: &str| el.listened_events().any(|e| e == ev);
    let buttons: Vec<_> = els.iter().filter(|e| e.tag() == "button").collect();
    // 5 top picks, the trigger, 5 custom colours, 15 palette colours, and
    // the "no shades" placeholder
    assert_eq!(buttons.len(), 27);
    for b in &buttons {
        let class = b.attribute("class").unwrap_or("");
        if class.contains("color-picker__button--no-focus-visible") {
            continue;
        }
        assert!(listens(b, "click"), "{}", b.to_html());
    }
    let content = els
        .iter()
        .find(|e| e.attribute("class") == Some("color-picker-content properties-content"))
        .unwrap();
    assert!(listens(content, "keydown"));
    let input = els.iter().find(|e| e.tag() == "input").unwrap();
    for ev in ["input", "blur", "focus", "keydown"] {
        assert!(listens(input, ev), "input {ev}");
    }
    let dropper = els
        .iter()
        .find(|e| e.attribute("class") == Some("excalidraw-eye-dropper-trigger"))
        .unwrap();
    assert!(listens(dropper, "click"));
}

#[test]
fn the_eye_dropper_trigger_shows_its_state_and_the_hex_input_its_error() {
    let mut p = open_props();
    p.eye_dropper_active = true;
    p.hex = change_hex_input("12345").state;
    let nodes = color_picker(&p);
    let html: String = nodes.iter().map(Node::to_html).collect();
    assert!(html.contains(r#"class="excalidraw-eye-dropper-trigger selected""#), "{html}");
    assert!(html.contains(r#"class="color-picker__input-label has-error""#), "{html}");
    assert!(html.contains(r#"aria-invalid="true""#), "{html}");
    assert!(html.contains(r#"value="12345""#), "{html}");
    assert!(html.contains(
        r#"<div class="color-picker__error-message" role="alert">Hex code must be 3, 4, 6, or 8 characters</div>"#
    ), "{html}");
}

#[test]
fn the_stylesheet_is_upstreams() {
    let css = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/color_picker/color_picker.css"
    ))
    .unwrap();
    assert_eq!(COLOR_PICKER_CSS, css);
    for selector in [
        ".color-picker__top-picks",
        ".color-picker__button--large",
        ".color-picker__button__hotkey-label",
        ".color-picker__input-label",
        ".excalidraw-eye-dropper-trigger",
        ".excalidraw-eye-dropper-preview",
        ".excalidraw-eye-dropper-backdrop",
    ] {
        assert!(css.contains(selector), "{selector}");
    }
}
