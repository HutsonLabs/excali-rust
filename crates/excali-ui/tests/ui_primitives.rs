//! The UI primitives (ex-516): Island, Stack, Button, the ToolIcon button
//! (`IconButton`), RadioGroup, Range, TextField, Popover, Modal, Dialog and
//! Tooltip, built with `excali_ui::dom` and held to the DOM upstream's
//! components leave (`packages/excalidraw/components/*.tsx`), their layout
//! rules and the design system's sizes.
//!
//! Fixture: `tests/fixtures/ui-primitives.json`, upstream's components at
//! the pinned commit rendered by React 19.0.0 into jsdom
//! (`tools/goldens/ui-primitives.mjs`): per case the props and the element
//! tree (attributes and inline style as maps, input `value`/`checked`),
//! updateTooltipPosition and showTooltip on rects in a viewport, Popover's
//! fit-in-viewport style, the Popover and Dialog Tab traps, and the sizing
//! tokens theme.scss declares. `src/primitives/primitives.css` is the same
//! generator's compilation of upstream's SCSS.

use std::collections::BTreeMap;
use std::rc::Rc;

use excali_scene::shape::Theme;
use excali_ui::dom::{Element, Node};
use excali_ui::primitives::{
    button, dialog, dialog_size, dialog_tab_target, icon_button, island, modal, popover,
    popover_fit_style, popover_tab_target, radio_group, range, stack_col, stack_row, text_field,
    tooltip, tooltip_position, tooltip_width_limits, Align, ButtonProps, ButtonType, DialogProps,
    DialogSize, IconButtonKind, IconButtonProps, IconButtonSize, IslandProps, JustifyContent,
    ModalProps, PopoverProps, RadioGroupChoice, RadioGroupProps, RangeProps, Rect, StackProps,
    TextFieldProps, TextFieldType, TextFieldValue, TooltipPlacement, TooltipProps, TrapFocus,
    BORDER_RADIUS_LG, BORDER_RADIUS_MD, DEFAULT_BUTTON_SIZE, DEFAULT_ICON_SIZE,
    LARGE_SCREEN_TOKENS, LG_BUTTON_SIZE, LG_ICON_SIZE, PRIMITIVES_CSS, SIZE_TOKENS, SPACE_FACTOR,
};
use serde_json::{json, Map, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/ui-primitives.json")).unwrap()
}

fn cases(key: &str) -> Vec<Value> {
    fixture()[key].as_array().unwrap().clone()
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn string(v: &Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

// -- the tree the fixture records ---------------------------------------------

/// A node as the fixture records it: attributes and style sorted, adjacent
/// text merged (React may split a JSX text run into several text nodes;
/// the DOM's text is the same), inputs with their `value` and `checked`.
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
            if el.tag() == "input" {
                // a fresh input's properties follow its attributes
                let kind = el.attribute("type").unwrap_or("text");
                let default = if kind == "radio" || kind == "checkbox" {
                    "on"
                } else {
                    ""
                };
                out.insert(
                    "props".into(),
                    json!({
                        "value": el.attribute("value").unwrap_or(default),
                        "checked": el.attribute("checked").is_some(),
                    }),
                );
            }
            out.insert(
                "children".into(),
                Value::Array(merge_text(el.children().iter().map(tree).collect())),
            );
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

fn normalize(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut out = Map::new();
            for (k, x) in o {
                if k == "children" {
                    let kids = x.as_array().unwrap().iter().map(normalize).collect();
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

// -- props from the fixture ---------------------------------------------------

/// A child as the generator gives it to React: text, or an element
/// `{tag, className, text}`.
fn child(spec: &Value) -> Node {
    match spec {
        Value::String(s) => Node::text(s),
        Value::Object(o) => {
            let tag = o["tag"].as_str().unwrap();
            let mut el = if tag == "svg" {
                Element::svg(tag)
            } else {
                Element::new(tag)
            };
            if let Some(class) = o.get("className").and_then(Value::as_str) {
                el = el.attr("class", class);
            }
            if let Some(text) = o.get("text").and_then(Value::as_str) {
                if !text.is_empty() {
                    el = el.child(Node::text(text));
                }
            }
            el.into()
        }
        other => panic!("child {other}"),
    }
}

fn children(props: &Value) -> Vec<Node> {
    props["children"]
        .as_array()
        .map(|a| a.iter().map(child).collect())
        .unwrap_or_default()
}

/// React's `style` prop as the DOM serializes it: camelCase to
/// kebab-case, numbers in px (unitless for custom properties), `0` as
/// `0px`.
fn style(props: &Value) -> Vec<(String, String)> {
    let Some(o) = props["style"].as_object() else {
        return Vec::new();
    };
    o.iter()
        .map(|(k, v)| {
            let name = if k.starts_with("--") {
                k.clone()
            } else {
                k.chars()
                    .flat_map(|c| {
                        if c.is_ascii_uppercase() {
                            vec!['-', c.to_ascii_lowercase()]
                        } else {
                            vec![c]
                        }
                    })
                    .collect()
            };
            let value = match v {
                Value::Number(n) if k.starts_with("--") => {
                    excali_core::json::number_to_string(n.as_f64().unwrap())
                }
                Value::Number(n) => format!(
                    "{}px",
                    excali_core::json::number_to_string(n.as_f64().unwrap())
                ),
                Value::String(s) => s.clone(),
                other => panic!("style {other}"),
            };
            (name, value)
        })
        .collect()
}

/// `className` as clsx takes it: a string, or `false` for none.
fn class_name(props: &Value) -> Option<String> {
    string(&props["className"])
}

fn render(case: &Value) -> Vec<Node> {
    let p = &case["props"];
    let env = &case["env"];
    let theme = match env["theme"].as_str() {
        Some("dark") => Theme::Dark,
        _ => Theme::Light,
    };
    let phone = env["formFactor"].as_str() == Some("phone");
    let bool_prop = |k: &str| p[k].as_bool().unwrap_or(false);
    let el: Option<Element> = match case["component"].as_str().unwrap() {
        "Island" => Some(island(
            IslandProps {
                padding: p["padding"].as_f64(),
                class_name: class_name(p),
                style: style(p),
                viewport_ui: string(&p["data-viewport-ui"]),
                viewport_ui_name: string(&p["data-viewport-ui-name"]),
            },
            children(p),
        )),
        name @ ("Stack.Row" | "Stack.Col") => {
            let props = StackProps {
                gap: p["gap"].as_f64(),
                align: p["align"].as_str().map(|a| match a {
                    "start" => Align::Start,
                    "center" => Align::Center,
                    "end" => Align::End,
                    "baseline" => Align::Baseline,
                    other => panic!("{other}"),
                }),
                justify_content: p["justifyContent"].as_str().map(|j| match j {
                    "center" => JustifyContent::Center,
                    "space-around" => JustifyContent::SpaceAround,
                    "space-between" => JustifyContent::SpaceBetween,
                    other => panic!("{other}"),
                }),
                class_name: class_name(p),
                style: style(p),
            };
            Some(if name == "Stack.Row" {
                stack_row(props, children(p))
            } else {
                stack_col(props, children(p))
            })
        }
        "Button" => Some(button(
            ButtonProps {
                kind: match p["type"].as_str() {
                    Some("submit") => ButtonType::Submit,
                    Some("reset") => ButtonType::Reset,
                    _ => ButtonType::Button,
                },
                selected: bool_prop("selected"),
                class_name: class_name(p).unwrap_or_default(),
                title: string(&p["title"]),
                style: style(p),
                on_select: Some(Rc::new(|| {})),
            },
            children(p),
        )),
        "IconButton" => {
            let kind = match p["type"].as_str().unwrap() {
                "button" => IconButtonKind::Button,
                "icon" => IconButtonKind::Icon,
                "toggle" => IconButtonKind::Toggle {
                    checked: bool_prop("checked"),
                },
                other => panic!("{other}"),
            };
            Some(icon_button(IconButtonProps {
                kind,
                icon: (!p["icon"].is_null()).then(|| child(&p["icon"])),
                label: string(&p["label"]),
                aria_label: string(&p["aria-label"]).unwrap(),
                aria_keyshortcuts: string(&p["aria-keyshortcuts"]),
                test_id: string(&p["data-testid"]),
                title: string(&p["title"]),
                size: match p["size"].as_str() {
                    Some("small") => IconButtonSize::Small,
                    _ => IconButtonSize::Medium,
                },
                key_binding_label: string(&p["keyBindingLabel"]),
                show_aria_label: bool_prop("showAriaLabel"),
                hidden: bool_prop("hidden"),
                visible: p["visible"].as_bool().unwrap_or(true),
                disabled: bool_prop("disabled"),
                class_name: class_name(p).unwrap_or_default(),
                style: style(p),
                children: children(p),
                ..IconButtonProps::default()
            }))
        }
        "RadioGroup" => Some(radio_group(RadioGroupProps {
            name: string(&p["name"]).unwrap(),
            value: string(&p["value"]).unwrap(),
            choices: p["choices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| RadioGroupChoice {
                    value: string(&c["value"]).unwrap(),
                    label: child(&c["label"]),
                    aria_label: string(&c["ariaLabel"]),
                })
                .collect(),
            on_change: Rc::new(|_: String| {}),
        })),
        "Range" => {
            let min = p["min"].as_f64().unwrap_or(0.0);
            Some(range(RangeProps {
                label: child(&p["label"]),
                value: num(&p["value"]),
                min,
                max: p["max"].as_f64().unwrap_or(100.0),
                step: p["step"].as_f64().unwrap_or(10.0),
                min_label: (!p["minLabel"].is_null()).then(|| child(&p["minLabel"])),
                has_common_value: p["hasCommonValue"].as_bool().unwrap_or(true),
                test_id: string(&p["testId"]),
                on_change: Rc::new(|_| {}),
            }))
        }
        "TextField" => Some(text_field(TextFieldProps {
            value: match (string(&p["value"]), string(&p["defaultValue"])) {
                (Some(v), _) => TextFieldValue::Controlled(v),
                (None, Some(v)) => TextFieldValue::Default(v),
                _ => panic!("no value"),
            },
            label: string(&p["label"]),
            full_width: bool_prop("fullWidth"),
            placeholder: string(&p["placeholder"]),
            readonly: bool_prop("readonly"),
            is_redacted: bool_prop("isRedacted"),
            icon: (!p["icon"].is_null()).then(|| child(&p["icon"])),
            class_name: class_name(p),
            kind: p["type"].as_str().map(|t| match t {
                "text" => TextFieldType::Text,
                "search" => TextFieldType::Search,
                other => panic!("{other}"),
            }),
            ..TextFieldProps::default()
        })),
        "Popover" => Some(popover(
            PopoverProps {
                class_name: class_name(p),
                top: p["top"].as_f64(),
                left: p["left"].as_f64(),
                ..PopoverProps::default()
            },
            children(p),
        )),
        "Modal" => Some(modal(
            ModalProps {
                class_name: class_name(p),
                max_width: p["maxWidth"].as_f64(),
                labelled_by: string(&p["labelledBy"]).unwrap(),
                theme,
                phone,
                ..ModalProps::default()
            },
            children(p),
        )),
        "Dialog" => Some(dialog(
            DialogProps {
                class_name: class_name(p),
                size: match &p["size"] {
                    Value::Null => DialogSize::Default,
                    Value::Number(n) => DialogSize::Px(n.as_f64().unwrap()),
                    Value::String(s) => match s.as_str() {
                        "small" => DialogSize::Small,
                        "regular" => DialogSize::Regular,
                        "wide" => DialogSize::Wide,
                        other => panic!("{other}"),
                    },
                    other => panic!("size {other}"),
                },
                title: string(&p["title"]).map(Node::text),
                container_id: "excalidraw-id".into(),
                theme,
                phone,
                ..DialogProps::default()
            },
            children(p),
        )),
        "Tooltip" => tooltip(
            TooltipProps {
                label: string(&p["label"]).unwrap(),
                long: bool_prop("long"),
                class_name: class_name(p),
                style: style(p),
                disabled: bool_prop("disabled"),
                ..TooltipProps::default()
            },
            children(p),
        ),
        other => panic!("component {other}"),
    };
    el.into_iter().map(Node::from).collect()
}

#[test]
fn every_primitive_renders_upstreams_dom() {
    let cases = cases("render");
    assert!(cases.len() >= 50);
    let mut failures = Vec::new();
    for case in &cases {
        let got: Vec<Value> = render(case).iter().map(tree).collect();
        let want: Vec<Value> = case["dom"].as_array().unwrap().iter().map(normalize).collect();
        if got != want {
            failures.push(format!(
                "{}:\n  want {}\n  got  {}",
                case["name"],
                Value::Array(want),
                Value::Array(got)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn class_names_are_upstreams() {
    // the root class of each primitive, as its stylesheet selects it
    let classes: Vec<String> = cases("render")
        .iter()
        .flat_map(|c| render(c).into_iter())
        .filter_map(|n| match n {
            Node::Element(el) => el.attribute("class").map(str::to_owned),
            Node::Text(_) => None,
        })
        .collect();
    for want in [
        "Island",
        "Stack Stack_horizontal",
        "Stack Stack_vertical",
        "excalidraw-button",
        "ToolIcon_type_button",
        "ToolIcon ToolIcon_type_toggle",
        "RadioGroup",
        "control-label",
        "ExcTextField",
        "popover",
        "excalidraw excalidraw-modal-container",
        "excalidraw-tooltip-wrapper",
    ] {
        assert!(
            classes.iter().any(|c| c.starts_with(want)),
            "no root class {want}"
        );
        assert!(
            PRIMITIVES_CSS.contains(&format!(".{}", want.split(' ').next_back().unwrap())),
            "{want} not styled"
        );
    }
}

// -- sizes --------------------------------------------------------------------

#[test]
fn sizing_tokens_are_theme_scss() {
    let f = fixture();
    let tokens = f["tokens"].as_object().unwrap();
    assert_eq!(tokens.len(), SIZE_TOKENS.len());
    for (name, value) in SIZE_TOKENS {
        assert_eq!(tokens[*name], json!(value), "{name}");
    }
    let large = f["largeScreenTokens"].as_object().unwrap();
    assert_eq!(large.len(), LARGE_SCREEN_TOKENS.len());
    for (name, value) in LARGE_SCREEN_TOKENS {
        assert_eq!(large[*name], json!(value), "{name}");
    }
    // the acceptance criteria's numbers
    assert_eq!(DEFAULT_BUTTON_SIZE, "2rem");
    assert_eq!(DEFAULT_ICON_SIZE, "1rem");
    assert_eq!(LG_BUTTON_SIZE, "2.25rem");
    assert_eq!(LG_ICON_SIZE, "1rem");
    assert_eq!(SPACE_FACTOR, "0.25rem");
    assert_eq!(BORDER_RADIUS_MD, "0.375rem");
    assert_eq!(BORDER_RADIUS_LG, "0.5rem");
}

#[test]
fn the_stylesheet_declares_the_sizes_and_uses_them() {
    for (name, value) in SIZE_TOKENS {
        assert!(
            PRIMITIVES_CSS.contains(&format!("  {name}: {value};")),
            "{name}"
        );
    }
    for rule in [
        // Island.scss: padding × space factor, the large radius
        "padding: calc(var(--padding) * var(--space-factor));",
        // Stack.scss: gap × space factor
        "gap: calc(var(--space-factor) * var(--gap));",
        "border-radius: var(--border-radius-lg);",
        // ToolIcon.scss: the button and its icon
        "width: var(--default-button-size);",
        "height: var(--default-button-size);",
        "width: var(--default-icon-size);",
        "height: var(--default-icon-size);",
    ] {
        assert!(PRIMITIVES_CSS.contains(rule), "{rule}");
    }
    assert!(PRIMITIVES_CSS.contains("@media screen and (min-device-width: 1921px)"));
    assert!(PRIMITIVES_CSS.contains(".excalidraw.theme--dark"));
}

// -- layout rules -------------------------------------------------------------

#[test]
fn dialog_sizes() {
    assert_eq!(dialog_size(DialogSize::Small), 550.0);
    assert_eq!(dialog_size(DialogSize::Regular), 800.0);
    assert_eq!(dialog_size(DialogSize::Default), 800.0);
    assert_eq!(dialog_size(DialogSize::Wide), 1024.0);
    assert_eq!(dialog_size(DialogSize::Px(640.0)), 640.0);
    // `size && typeof size === "number"`: 0 is not a size
    assert_eq!(dialog_size(DialogSize::Px(0.0)), 800.0);
}

fn rect(v: &Value) -> Rect {
    Rect {
        left: num(&v["left"]),
        top: num(&v["top"]),
        width: num(&v["width"]),
        height: num(&v["height"]),
    }
}

fn px(n: f64) -> String {
    format!("{}px", excali_core::json::number_to_string(n))
}

fn placement(v: &Value) -> TooltipPlacement {
    match v.as_str() {
        Some("top") => TooltipPlacement::Top,
        _ => TooltipPlacement::Bottom,
    }
}

#[test]
fn tooltip_positions() {
    let cases = cases("tooltipPosition");
    assert!(cases.len() >= 10);
    for c in &cases {
        let (top, left) = tooltip_position(
            rect(&c["item"]),
            (num(&c["tooltip"]["width"]), num(&c["tooltip"]["height"])),
            (num(&c["viewport"]["width"]), num(&c["viewport"]["height"])),
            placement(&c["position"]),
        );
        assert_eq!(px(top), c["top"].as_str().unwrap(), "{c}");
        assert_eq!(px(left), c["left"].as_str().unwrap(), "{c}");
    }
}

#[test]
fn shown_tooltips() {
    for c in &cases("showTooltip") {
        let long = c["options"]["long"].as_bool().unwrap_or(false);
        let (top, left) = tooltip_position(
            rect(&c["item"]),
            (
                num(&c["tooltipSize"]["width"]),
                num(&c["tooltipSize"]["height"]),
            ),
            (1024.0, 768.0),
            placement(&c["options"]["position"]),
        );
        let (min, max) = tooltip_width_limits(long);
        let shown = &c["shown"];
        assert_eq!(
            shown["attrs"]["class"],
            "excalidraw-tooltip excalidraw-tooltip--visible"
        );
        assert_eq!(
            shown["style"],
            json!({"left": px(left), "max-width": max, "min-width": min, "top": px(top)}),
            "{c}"
        );
        assert_eq!(shown["children"], json!([c["label"]]));
        assert_eq!(c["hidden"]["attrs"]["class"], "excalidraw-tooltip");
    }
}

#[test]
fn popover_fits_in_the_viewport() {
    let cases = cases("popoverFit");
    assert!(cases.len() >= 8);
    for c in &cases {
        let got: BTreeMap<String, String> = popover_fit_style(
            num(&c["top"]),
            num(&c["left"]),
            (num(&c["content"]["width"]), num(&c["content"]["height"])),
            (num(&c["viewport"]["width"]), num(&c["viewport"]["height"])),
        )
        .into_iter()
        .collect();
        let want: BTreeMap<String, String> = c["style"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect();
        assert_eq!(got, want, "{c}");
    }
}

#[test]
fn tab_traps() {
    let cases = cases("focusTrap");
    assert!(cases.len() >= 18);
    for c in &cases {
        let count = c["count"].as_u64().unwrap() as usize;
        let shift = c["shift"].as_bool().unwrap();
        let focused = c["focused"].as_i64().unwrap();
        let target = match c["component"].as_str().unwrap() {
            "Popover" => popover_tab_target(
                if focused < 0 {
                    TrapFocus::Container
                } else {
                    TrapFocus::Index(focused as usize)
                },
                count,
                shift,
            ),
            "Dialog" => dialog_tab_target(
                (focused >= 0).then_some(focused as usize),
                count,
                shift,
            ),
            other => panic!("{other}"),
        };
        // a target: focus moves there and Tab's default is prevented
        assert_eq!(target.is_some(), c["prevented"].as_bool().unwrap(), "{c}");
        if let Some(i) = target {
            assert_eq!(i as i64, c["result"].as_i64().unwrap(), "{c}");
        }
    }
}
