//! `Tooltip` (`components/Tooltip.tsx`, `Tooltip.scss`): one shared
//! `div.excalidraw-tooltip` in the body, shown under (or over) the hovered
//! item, centred on it and kept 5px inside the viewport.

use std::cell::RefCell;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{Document, MutationObserver, MutationObserverInit};

use crate::dom::{class_names, Element, Node};

use super::px;

/// ms before a delayed tooltip shows (`TOOLTIP_DELAY`).
pub const TOOLTIP_DELAY: i32 = 500;
/// ms after a tooltip hides during which a delayed one shows at once, as
/// when moving across adjacent buttons (`TOOLTIP_WARM_WINDOW`).
pub const TOOLTIP_WARM_WINDOW: f64 = 300.0;
/// The tooltip's distance from its item and the viewport's edges.
pub const TOOLTIP_MARGIN: f64 = 5.0;

/// A client rect.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

/// Which side of the item the tooltip prefers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipPlacement {
    #[default]
    Bottom,
    Top,
}

/// `updateTooltipPosition` (`Tooltip.tsx:20-60`): the tooltip's `(top,
/// left)` for an item, the tooltip's size and the viewport's: centred on
/// the item, 5px from the viewport's left or right edge when it would
/// cross it, and on the other side of the item when the preferred one has
/// no room.
pub fn tooltip_position(
    item: Rect,
    tooltip: (f64, f64),
    viewport: (f64, f64),
    placement: TooltipPlacement,
) -> (f64, f64) {
    let margin = TOOLTIP_MARGIN;
    let mut left = item.left + item.width / 2.0 - tooltip.0 / 2.0;
    if left < 0.0 {
        left = margin;
    } else if left + tooltip.0 >= viewport.0 {
        left = viewport.0 - tooltip.0 - margin;
    }
    let top = match placement {
        TooltipPlacement::Bottom => {
            let top = item.top + item.height + margin;
            if top + tooltip.1 >= viewport.1 {
                item.top - tooltip.1 - margin
            } else {
                top
            }
        }
        TooltipPlacement::Top => {
            let top = item.top - tooltip.1 - margin;
            if top < 0.0 {
                item.top + item.height + margin
            } else {
                top
            }
        }
    };
    (top, left)
}

/// The tooltip's `(min-width, max-width)`: 50ch both when long, else 10ch
/// to 15ch (`Tooltip.tsx:94-95`).
pub fn tooltip_width_limits(long: bool) -> (&'static str, &'static str) {
    if long {
        ("50ch", "50ch")
    } else {
        ("10ch", "15ch")
    }
}

/// How [`show_tooltip`] shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TooltipOptions {
    pub long: bool,
    /// After [`TOOLTIP_DELAY`], unless a tooltip hid within
    /// [`TOOLTIP_WARM_WINDOW`].
    pub delay: bool,
    pub placement: TooltipPlacement,
}

#[derive(Default)]
struct TooltipState {
    timer: i32,
    hidden_at: f64,
    observer: Option<(MutationObserver, Closure<dyn FnMut()>)>,
}

thread_local! {
    static STATE: RefCell<TooltipState> = RefCell::default();
}

const TOOLTIP_CLASS: &str = "excalidraw-tooltip";
const VISIBLE_CLASS: &str = "excalidraw-tooltip--visible";

/// `getTooltipDiv`: the document's tooltip div, created in the body on
/// first use.
fn tooltip_div(document: &Document) -> Option<web_sys::HtmlElement> {
    if let Ok(Some(existing)) = document.query_selector(&format!(".{TOOLTIP_CLASS}")) {
        return existing.dyn_into().ok();
    }
    let div: web_sys::HtmlElement = document.create_element("div").ok()?.dyn_into().ok()?;
    document.body()?.append_child(&div).ok()?;
    let _ = div.class_list().add_1(TOOLTIP_CLASS);
    Some(div)
}

fn clear_timer() {
    let timer = STATE.with(|s| s.borrow().timer);
    if let Some(window) = web_sys::window() {
        window.clear_timeout_with_handle(timer);
    }
}

fn disconnect_observer() {
    // taken out first: disconnecting from the observer's own callback
    // must not drop the closure while it runs
    let observer = STATE.with(|s| s.borrow_mut().observer.take());
    if let Some((observer, closure)) = observer {
        observer.disconnect();
        // keep the closure alive until the current task ends
        closure.forget();
    }
}

/// `hideTooltip`: hides the tooltip and cancels a pending one.
pub fn hide_tooltip(document: &Document) {
    clear_timer();
    disconnect_observer();
    let Some(tooltip) = tooltip_div(document) else {
        return;
    };
    if tooltip.class_list().contains(VISIBLE_CLASS) {
        let _ = tooltip.class_list().remove_1(VISIBLE_CLASS);
        STATE.with(|s| s.borrow_mut().hidden_at = js_sys::Date::now());
    }
}

fn viewport() -> (f64, f64) {
    let Some(window) = web_sys::window() else {
        return (0.0, 0.0);
    };
    let size = |v: Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>| {
        v.ok().and_then(|v| v.as_f64()).unwrap_or(0.0)
    };
    (size(window.inner_width()), size(window.inner_height()))
}

fn update_tooltip(item: &web_sys::Element, label: &str, options: TooltipOptions) {
    let Some(document) = item.owner_document() else {
        return;
    };
    let Some(tooltip) = tooltip_div(&document) else {
        return;
    };
    let _ = tooltip.class_list().add_1(VISIBLE_CLASS);
    let (min, max) = tooltip_width_limits(options.long);
    let style = tooltip.style();
    let _ = style.set_property("min-width", min);
    let _ = style.set_property("max-width", max);
    tooltip.set_text_content(Some(label));
    let r = item.get_bounding_client_rect();
    let t = tooltip.get_bounding_client_rect();
    let (top, left) = tooltip_position(
        Rect {
            left: r.left(),
            top: r.top(),
            width: r.width(),
            height: r.height(),
        },
        (t.width(), t.height()),
        viewport(),
        options.placement,
    );
    let _ = style.set_property("top", &px(top));
    let _ = style.set_property("left", &px(left));
    // hide once the item leaves the DOM (unmounted while hovered, which
    // fires no pointerleave)
    disconnect_observer();
    let watched = item.clone();
    let closure = Closure::<dyn FnMut()>::new(move || {
        if !watched.is_connected() {
            if let Some(document) = watched.owner_document() {
                hide_tooltip(&document);
            }
        }
    });
    let Ok(observer) = MutationObserver::new(closure.as_ref().unchecked_ref()) else {
        return;
    };
    if let Some(body) = document.body() {
        let init = MutationObserverInit::new();
        init.set_child_list(true);
        init.set_subtree(true);
        let _ = observer.observe_with_options(&body, &init);
    }
    STATE.with(|s| s.borrow_mut().observer = Some((observer, closure)));
}

/// `showTooltip`: shows `label` for `item`, for elements that cannot be
/// wrapped in a [`tooltip`]; pair with [`hide_tooltip`].
pub fn show_tooltip(item: &web_sys::Element, label: &str, options: TooltipOptions) {
    clear_timer();
    let hidden_at = STATE.with(|s| s.borrow().hidden_at);
    let show = {
        let item = item.clone();
        let label = label.to_owned();
        move || {
            // the item may be gone by the time a delayed tooltip shows
            if item.is_connected() {
                update_tooltip(&item, &label, options);
            }
        }
    };
    if options.delay && js_sys::Date::now() - hidden_at > TOOLTIP_WARM_WINDOW {
        let callback = Closure::once_into_js(show);
        if let Some(window) = web_sys::window() {
            let timer = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.unchecked_ref(),
                    TOOLTIP_DELAY,
                )
                .unwrap_or(0);
            STATE.with(|s| s.borrow_mut().timer = timer);
        }
    } else {
        show();
    }
}

/// Tooltip's props.
#[derive(Clone, Debug, Default)]
pub struct TooltipProps {
    pub label: String,
    pub long: bool,
    pub style: Vec<(String, String)>,
    pub class_name: Option<String>,
    /// Renders nothing, children included.
    pub disabled: bool,
    pub delay: bool,
}

/// `div.excalidraw-tooltip-wrapper` around `children`, showing the tooltip
/// while hovered (`Tooltip.tsx:161-187`); `None` when disabled.
pub fn tooltip(props: TooltipProps, children: Vec<Node>) -> Option<Element> {
    if props.disabled {
        return None;
    }
    let options = TooltipOptions {
        long: props.long,
        delay: props.delay,
        placement: TooltipPlacement::Bottom,
    };
    let label = props.label;
    Some(
        Element::new("div")
            .attr(
                "class",
                class_names([
                    ("excalidraw-tooltip-wrapper", true),
                    (props.class_name.as_deref().unwrap_or(""), true),
                ]),
            )
            .on("pointerenter", move |e| {
                if let Some(item) = e
                    .current_target()
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                {
                    show_tooltip(&item, &label, options);
                }
            })
            .on("pointerleave", |e| {
                if let Some(document) = e
                    .current_target()
                    .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
                    .and_then(|n| n.owner_document())
                {
                    hide_tooltip(&document);
                }
            })
            .styles(props.style)
            .children_from(children),
    )
}
