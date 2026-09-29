//! ex-516 harness: excali-ui's UI primitives in the browser.
//!
//! [`mount_gallery`] installs the primitives' stylesheet and mounts one of
//! each in an `.excalidraw` element of the page; [`open_dialog`] and
//! [`open_popover`] open the modal ones. Every callback appends a line to a
//! log the Playwright suite `tests/web/primitives` reads with
//! [`take_log`]. `scripts/web/ui-primitives.sh` builds it.

use std::cell::RefCell;
use std::rc::Rc;

use excali_scene::shape::Theme;
use excali_ui::dom::{mount, Element, Mounted, Node};
use excali_ui::primitives::icons::{close_icon, eye_icon};
use excali_ui::primitives::{
    button, dialog, icon_button, install_stylesheet, island, open_modal, popover, radio_group,
    range, stack_col, stack_row, text_field, tooltip, Align, ButtonProps, DialogProps, DialogSize,
    IconButtonKind, IconButtonProps, IslandProps, OpenModal, PopoverProps, RadioGroupChoice,
    RadioGroupProps, RangeProps, StackProps, TextFieldProps, TextFieldValue, TooltipProps,
};
use excali_ui::theme::{apply_container_tokens, apply_theme};
use wasm_bindgen::prelude::*;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static MOUNTED: RefCell<Vec<Mounted>> = const { RefCell::new(Vec::new()) };
    static DIALOG: RefCell<Option<OpenModal>> = const { RefCell::new(None) };
}

fn log(line: impl Into<String>) {
    LOG.with(|l| l.borrow_mut().push(line.into()));
}

/// The callbacks' lines since the last call.
#[wasm_bindgen(js_name = takeLog)]
pub fn take_log() -> Vec<String> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

fn document() -> Result<web_sys::Document, JsError> {
    web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsError::new("no document"))
}

fn js(e: JsValue) -> JsError {
    JsError::new(&format!("{e:?}"))
}

fn keep(mounted: Mounted) {
    MOUNTED.with(|m| m.borrow_mut().push(mounted));
}

fn tool(name: &'static str, checked: bool, key: &str) -> Node {
    icon_button(IconButtonProps {
        kind: IconButtonKind::Toggle { checked },
        icon: Some(eye_icon().into()),
        aria_label: name.into(),
        title: Some(name.into()),
        key_binding_label: Some(key.into()),
        test_id: Some(format!("toolbar-{name}")),
        on_select: Some(Rc::new(move |pointer: Option<String>| {
            log(format!(
                "select:{name}:{}",
                pointer.as_deref().unwrap_or("null")
            ))
        })),
        ..IconButtonProps::default()
    })
    .into()
}

/// Switches `container` (the `.excalidraw` element) to the dark theme or
/// back (ex-532).
#[wasm_bindgen(js_name = setTheme)]
pub fn set_theme(container: &web_sys::Element, dark: bool) -> Result<(), JsError> {
    let theme = if dark { Theme::Dark } else { Theme::Light };
    apply_theme(container, theme).map_err(js)
}

/// Mounts one of each primitive in `parent` (an `.excalidraw` element) and
/// gives it the container's inline tokens.
#[wasm_bindgen(js_name = mountGallery)]
pub fn mount_gallery(parent: &web_sys::HtmlElement) -> Result<(), JsError> {
    let document = document()?;
    install_stylesheet(&document).map_err(js)?;
    apply_container_tokens(parent).map_err(js)?;
    let toolbar = island(
        IslandProps {
            padding: Some(2.0),
            class_name: Some("gallery-toolbar".into()),
            ..IslandProps::default()
        },
        vec![stack_row(
            StackProps {
                gap: Some(1.0),
                align: Some(Align::Center),
                class_name: Some("gallery-row".into()),
                ..StackProps::default()
            },
            vec![
                tool("rectangle", true, "2"),
                tool("ellipse", false, "4"),
                icon_button(IconButtonProps {
                    icon: Some(close_icon().into()),
                    aria_label: "Close".into(),
                    test_id: Some("plain-button".into()),
                    on_click: Some(Rc::new(|_| log("click:plain"))),
                    ..IconButtonProps::default()
                })
                .into(),
            ],
        )
        .into()],
    );
    let controls = stack_col(
        StackProps {
            gap: Some(2.0),
            class_name: Some("gallery-controls".into()),
            ..StackProps::default()
        },
        vec![
            button(
                ButtonProps {
                    class_name: "gallery-button".into(),
                    on_select: Some(Rc::new(|| log("button"))),
                    ..ButtonProps::default()
                },
                vec![Node::text("Save")],
            )
            .into(),
            radio_group(RadioGroupProps {
                name: "gallery-radio".into(),
                value: "light".to_owned(),
                choices: ["light", "dark", "system"]
                    .into_iter()
                    .map(|v| RadioGroupChoice {
                        value: v.to_owned(),
                        label: Node::text(v),
                        aria_label: Some(v.to_owned()),
                    })
                    .collect(),
                on_change: Rc::new(|v: String| log(format!("radio:{v}"))),
            })
            .into(),
            range(RangeProps {
                label: Node::text("Opacity"),
                value: 50.0,
                test_id: Some("gallery-range".into()),
                on_change: Rc::new(|v| log(format!("range:{v}"))),
                ..RangeProps::default()
            })
            .into(),
            text_field(TextFieldProps {
                value: TextFieldValue::Controlled("secret".into()),
                label: Some("Key".into()),
                is_redacted: true,
                class_name: Some("gallery-text".into()),
                on_change: Some(Rc::new(|v: String| log(format!("text:{v}")))),
                ..TextFieldProps::default()
            })
            .into(),
            tooltip(
                TooltipProps {
                    label: "A tooltip".into(),
                    class_name: Some("gallery-tooltip".into()),
                    ..TooltipProps::default()
                },
                vec![Element::new("button")
                    .attr("class", "gallery-tooltip-target")
                    .child("hover me")
                    .into()],
            )
            .map_or(Node::text(""), Node::from),
        ],
    );
    for node in [toolbar, controls] {
        keep(mount(&node.into(), &document, parent).map_err(js)?);
    }
    Ok(())
}

fn buttons(n: usize, class: &str) -> Vec<Node> {
    (0..n)
        .map(|i| {
            Element::new("button")
                .attr("class", class)
                .attr("data-i", i.to_string())
                .child(format!("b{i}"))
                .into()
        })
        .collect()
}

/// Opens a dialog of `size` ("small", "regular", "wide") holding three
/// buttons; closing it (Escape, the backdrop, the close button) logs
/// `dialog-close` and removes it.
#[wasm_bindgen(js_name = openDialog)]
pub fn open_dialog(title: &str, size: &str, phone: bool) -> Result<(), JsError> {
    let document = document()?;
    let el = dialog(
        DialogProps {
            title: Some(Node::text(title)),
            size: match size {
                "small" => DialogSize::Small,
                "wide" => DialogSize::Wide,
                _ => DialogSize::Regular,
            },
            container_id: "gallery".into(),
            theme: Theme::Light,
            phone,
            on_close_request: Some(Rc::new(|| {
                log("dialog-close");
                if let Some(open) = DIALOG.with(|d| d.borrow_mut().take()) {
                    open.close();
                }
            })),
            ..DialogProps::default()
        },
        buttons(3, "dialog-button"),
    );
    let open = open_modal(&document, el).map_err(js)?;
    DIALOG.with(|d| *d.borrow_mut() = Some(open));
    Ok(())
}

/// Mounts a popover at (`top`, `left`) in `parent` whose content is
/// `width` × `height` px, with three buttons; a press outside it logs
/// `popover-close`.
#[wasm_bindgen(js_name = openPopover)]
pub fn open_popover(
    parent: &web_sys::Element,
    top: f64,
    left: f64,
    width: f64,
    height: f64,
    fit: bool,
) -> Result<(), JsError> {
    let document = document()?;
    let content = Element::new("div")
        .attr("class", "popover-content")
        .style("width", format!("{width}px"))
        .style("height", format!("{height}px"))
        .children_from(buttons(3, "popover-button"));
    let el = popover(
        PopoverProps {
            top: Some(top),
            left: Some(left),
            fit_in_viewport: fit,
            class_name: Some("gallery-popover".into()),
            on_close_request: Some(Rc::new(|_| log("popover-close"))),
            ..PopoverProps::default()
        },
        vec![content.into()],
    )
    .style("position", "fixed")
    .style("top", format!("{top}px"))
    .style("left", format!("{left}px"));
    keep(mount(&el.into(), &document, parent).map_err(js)?);
    Ok(())
}
