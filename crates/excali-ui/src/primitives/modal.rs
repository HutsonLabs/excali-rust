//! `Modal` and `Dialog` (`components/Modal.tsx`, `Dialog.tsx`,
//! `Modal.scss`, `Dialog.scss`): a modal layer portalled to the body, in
//! its own `excalidraw excalidraw-modal-container` div
//! (`hooks/useCreatePortalContainer.ts`), and the titled dialog island in
//! it.
//!
//! The app-state side of a dialog's close (`setAppState({openMenu:
//! null})` and closing the library menu, `Dialog.tsx:94-99`) is the
//! caller's `on_close_request`.

use std::cell::RefCell;
use std::rc::Rc;

use excali_scene::shape::Theme;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Document, DragEvent, Event, HtmlElement, KeyboardEvent};

use crate::dom::{class_names, mount, Element, Mounted, Node};

use super::icons::close_icon;
use super::island::{island, IslandProps};
use super::{focusable_elements, px};

/// `t("buttons.close")` (`locales/en.json`), the fullscreen dialog's close
/// button's title and label.
pub const CLOSE_LABEL: &str = "Close";

/// Modal's props.
#[derive(Clone)]
pub struct ModalProps {
    pub class_name: Option<String>,
    /// `--max-width` of the content, in px.
    pub max_width: Option<f64>,
    /// `aria-labelledby`.
    pub labelled_by: String,
    /// The container's `theme--dark`.
    pub theme: Theme,
    /// The phone form factor: the container's `excalidraw--mobile`.
    pub phone: bool,
    /// The body has `excalidraw-animations-disabled`.
    pub animations_disabled: bool,
    /// A click on the backdrop requests closing (upstream's default).
    pub close_on_click_outside: bool,
    /// Called on Escape and, with `close_on_click_outside`, a backdrop
    /// click.
    pub on_close_request: Option<Rc<dyn Fn()>>,
}

impl Default for ModalProps {
    fn default() -> ModalProps {
        ModalProps {
            class_name: None,
            max_width: None,
            labelled_by: String::new(),
            theme: Theme::Light,
            phone: false,
            animations_disabled: false,
            close_on_click_outside: true,
            on_close_request: None,
        }
    }
}

/// Upstream's file-drop guard on a portal container
/// (`useCreatePortalContainer.ts:45-55`): a file dragged over a modal
/// must not make the browser open it.
fn refuse_file_drop(e: &Event) {
    let Some(drag) = e.dyn_ref::<DragEvent>() else {
        return;
    };
    let Some(transfer) = drag.data_transfer() else {
        return;
    };
    if transfer.types().includes(&JsValue::from_str("Files"), 0) {
        e.prevent_default();
        transfer.set_drop_effect("none");
    }
}

/// The portal container holding the modal (`Modal.tsx:40-69`): the
/// element [`open_modal`] appends to the body.
pub fn modal(props: ModalProps, children: Vec<Node>) -> Element {
    let on_escape = props.on_close_request.clone();
    let on_backdrop = props
        .on_close_request
        .clone()
        .filter(|_| props.close_on_click_outside);
    let mut background = Element::new("div").attr("class", "Modal__background");
    if let Some(close) = on_backdrop {
        background = background.on("click", move |_| close());
    }
    // `${props.maxWidth}px`: an absent width writes `undefinedpx`
    let max_width = props.max_width.map_or_else(|| "undefinedpx".to_owned(), px);
    Element::new("div")
        .attr(
            "class",
            class_names([
                ("excalidraw", true),
                ("excalidraw-modal-container", true),
                ("excalidraw--mobile", props.phone),
                ("theme--dark", props.theme == Theme::Dark),
            ]),
        )
        .on("dragover", refuse_file_drop)
        .on("drop", refuse_file_drop)
        .child(
            Element::new("div")
                .attr(
                    "class",
                    class_names([
                        ("Modal", true),
                        (props.class_name.as_deref().unwrap_or(""), true),
                        ("animations-disabled", props.animations_disabled),
                    ]),
                )
                .attr("role", "dialog")
                .attr("aria-modal", "true")
                .attr("aria-labelledby", props.labelled_by)
                .on("keydown", move |e| {
                    let Some(key) = e.dyn_ref::<KeyboardEvent>() else {
                        return;
                    };
                    if key.key() == "Escape" {
                        e.prevent_default();
                        e.stop_immediate_propagation();
                        e.stop_propagation();
                        if let Some(close) = &on_escape {
                            close();
                        }
                    }
                })
                .child(background)
                .child(
                    Element::new("div")
                        .attr("class", "Modal__content")
                        .style("--max-width", max_width)
                        .attr("tabindex", "0")
                        .children_from(children),
                ),
        )
}

/// A modal in the document.
pub struct OpenModal {
    mounted: Mounted,
}

impl OpenModal {
    /// The portal container.
    pub fn container(&self) -> Option<web_sys::Element> {
        self.mounted.element()
    }

    /// Removes the modal from the document.
    pub fn close(self) {
        self.mounted.remove();
    }
}

/// Appends a [`modal`] or [`dialog`] to `document`'s body, where upstream
/// portals it.
pub fn open_modal(document: &Document, modal: Element) -> Result<OpenModal, JsValue> {
    let body = document
        .body()
        .ok_or_else(|| JsValue::from_str("the document has no body"))?;
    let mounted = mount(&modal.into(), document, &body)?;
    // Modal.tsx:26-33: no animations when the body opts out
    if body.class_list().contains("excalidraw-animations-disabled") {
        if let Some(modal) = mounted.element().and_then(|c| c.first_element_child()) {
            modal.class_list().add_1("animations-disabled")?;
        }
    }
    Ok(OpenModal { mounted })
}

/// `size`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum DialogSize {
    Small,
    Regular,
    Wide,
    /// A width in px.
    Px(f64),
    /// No size: regular.
    #[default]
    Default,
}

/// `getDialogSize` (`Dialog.tsx:34-47`): small 550, regular 800, wide
/// 1024 px, a number as given (unless 0 or NaN, which are falsy).
pub fn dialog_size(size: DialogSize) -> f64 {
    match size {
        DialogSize::Px(n) if n != 0.0 && !n.is_nan() => n,
        DialogSize::Small => 550.0,
        DialogSize::Wide => 1024.0,
        _ => 800.0,
    }
}

/// The Tab trap of `Dialog.tsx:70-89`: from the first focusable element
/// Shift+Tab moves to the last, from the last Tab moves to the first (the
/// index returned, the key's default prevented); `None` leaves Tab to the
/// browser. `focused` is the index of the focused element among the
/// island's focusable ones.
pub fn dialog_tab_target(focused: Option<usize>, count: usize, shift: bool) -> Option<usize> {
    let i = focused?;
    if count == 0 {
        return None;
    }
    if i == 0 && shift {
        Some(count - 1)
    } else if i == count - 1 && !shift {
        Some(0)
    } else {
        None
    }
}

/// Dialog's props.
#[derive(Clone)]
pub struct DialogProps {
    pub class_name: Option<String>,
    pub size: DialogSize,
    /// The `h2` title; none when `None`.
    pub title: Option<Node>,
    /// Focuses the first focusable element after the close button once
    /// open (upstream's default).
    pub autofocus: bool,
    pub close_on_click_outside: bool,
    /// The editor container's id: the title's id is `{id}-dialog-title`.
    pub container_id: String,
    pub theme: Theme,
    /// The phone form factor: full screen, with a close button.
    pub phone: bool,
    /// Called after focus returns to where it was before the dialog.
    pub on_close_request: Option<Rc<dyn Fn()>>,
    /// The close button's title and label.
    pub close_label: String,
}

impl Default for DialogProps {
    fn default() -> DialogProps {
        DialogProps {
            class_name: None,
            size: DialogSize::Default,
            title: None,
            autofocus: true,
            close_on_click_outside: true,
            container_id: String::new(),
            theme: Theme::Light,
            phone: false,
            on_close_request: None,
            close_label: CLOSE_LABEL.to_owned(),
        }
    }
}

/// `Dialog` (`Dialog.tsx:49-136`): a [`modal`] (`Modal Dialog`, labelled by
/// `dialog-title`, [`dialog_size`] wide) holding an island with the title,
/// on phones a close button, and `div.Dialog__content`. Once open it
/// focuses its first field (after the close button), traps Tab, and on
/// close returns focus to the element that had it.
pub fn dialog(props: DialogProps, children: Vec<Node>) -> Element {
    let last_active: Rc<RefCell<Option<HtmlElement>>> = Rc::default();
    let on_close: Rc<dyn Fn()> = {
        let last_active = last_active.clone();
        let request = props.on_close_request.clone();
        Rc::new(move || {
            if let Some(el) = last_active.borrow().as_ref() {
                let _ = el.focus();
            }
            if let Some(request) = &request {
                request();
            }
        })
    };
    let close_button = props.phone.then(|| {
        let on_close = on_close.clone();
        Element::new("button")
            .attr("class", "Dialog__close")
            .attr("title", props.close_label.clone())
            .attr("aria-label", props.close_label.clone())
            .attr("type", "button")
            .on("click", move |_| on_close())
            .child(close_icon())
    });
    let title = props.title.map(|title| {
        Element::new("h2")
            .attr("id", format!("{}-dialog-title", props.container_id))
            .attr("class", "Dialog__title")
            .child(
                Element::new("span")
                    .attr("class", "Dialog__titleContent")
                    .child(title),
            )
    });
    let autofocus = props.autofocus;
    let content = island(
        IslandProps::default(),
        [
            title.map(Node::from),
            close_button.map(Node::from),
            Some(
                Element::new("div")
                    .attr("class", "Dialog__content")
                    .children_from(children)
                    .into(),
            ),
        ]
        .into_iter()
        .flatten()
        .collect(),
    )
    .on("keydown", |e| {
        let Some(key) = e.dyn_ref::<KeyboardEvent>() else {
            return;
        };
        let Some(island) = e
            .current_target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        else {
            return;
        };
        if key.key() != "Tab" {
            return;
        }
        let focusables = focusable_elements(&island);
        let active = island.owner_document().and_then(|d| d.active_element());
        let focused = active.and_then(|a| {
            focusables
                .iter()
                .position(|f| f.unchecked_ref::<web_sys::Element>() == &a)
        });
        if let Some(i) = dialog_tab_target(focused, focusables.len(), key.shift_key()) {
            let _ = focusables[i].focus();
            e.prevent_default();
        }
    })
    .on_mount(move |island| {
        *last_active.borrow_mut() = island
            .owner_document()
            .and_then(|d| d.active_element())
            .and_then(|a| a.dyn_into::<HtmlElement>().ok());
        let focusables = focusable_elements(island);
        if focusables.is_empty() || !autofocus {
            return;
        }
        // after the current task, as setTimeout(…) does
        let target = focusables.get(1).unwrap_or(&focusables[0]).clone();
        let focus = Closure::once_into_js(move || {
            let _ = target.focus();
        });
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback(focus.unchecked_ref());
        }
    });
    modal(
        ModalProps {
            class_name: Some(class_names([
                ("Dialog", true),
                (props.class_name.as_deref().unwrap_or(""), true),
                ("Dialog--fullscreen", props.phone),
            ])),
            max_width: Some(dialog_size(props.size)),
            labelled_by: "dialog-title".into(),
            theme: props.theme,
            phone: props.phone,
            animations_disabled: false,
            close_on_click_outside: props.close_on_click_outside,
            on_close_request: Some(on_close),
        },
        vec![content.into()],
    )
}
