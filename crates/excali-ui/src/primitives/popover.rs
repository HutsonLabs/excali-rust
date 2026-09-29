//! `Popover` (`components/Popover.tsx`, `Popover.scss`): a focusable
//! floating container that traps Tab, can keep itself inside the
//! viewport, and asks to close on a press outside it.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{Event, KeyboardEvent};

use crate::dom::{class_names, Element, Handler, Node};

use super::{focusable_elements, px};

/// `POPOVER_CONTAINER_GAP` (`Popover.tsx:12`): the popover's margin from
/// the viewport's edges.
pub const POPOVER_CONTAINER_GAP: f64 = 10.0;

/// Popover's props.
#[derive(Clone, Default)]
pub struct PopoverProps {
    pub top: Option<f64>,
    pub left: Option<f64>,
    /// Called with a `pointerdown` outside the popover.
    pub on_close_request: Option<Handler>,
    /// Clamps the popover inside the viewport once mounted (needs `top`
    /// and `left`).
    pub fit_in_viewport: bool,
    /// The viewport; the window's inner size when `None`.
    pub viewport_width: Option<f64>,
    pub viewport_height: Option<f64>,
    pub class_name: Option<String>,
}

/// Where focus is when Tab is pressed in a trap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapFocus {
    /// On the popover itself.
    Container,
    /// On its `i`th focusable element.
    Index(usize),
    /// Elsewhere.
    Other,
}

/// The Tab trap of `Popover.tsx:52-83`: the index of the focusable element
/// Tab (`shift` for Shift+Tab) moves to, the key's default prevented;
/// `None` leaves Tab to the browser.
pub fn popover_tab_target(focus: TrapFocus, count: usize, shift: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let last = count - 1;
    match focus {
        TrapFocus::Container => Some(if shift { last } else { 0 }),
        TrapFocus::Index(0) if shift => Some(last),
        TrapFocus::Index(i) if i == last && !shift => Some(0),
        _ => None,
    }
}

/// `Math.min(Math.max(value, min), max)` (`@excalidraw/math` `clamp`).
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    let lower = if value.is_nan() || min.is_nan() {
        f64::NAN
    } else {
        value.max(min)
    };
    if lower.is_nan() || max.is_nan() {
        f64::NAN
    } else {
        lower.min(max)
    }
}

/// The inline style `fitInViewport` gives a popover at `top`, `left` whose
/// content measures `size` in a `viewport` (`Popover.tsx:91-130`): each
/// axis clamped 10px inside the viewport, or, when the content does not
/// fit, filling it with a scrollbar.
pub fn popover_fit_style(
    top: f64,
    left: f64,
    size: (f64, f64),
    viewport: (f64, f64),
) -> Vec<(String, String)> {
    let gap = POPOVER_CONTAINER_GAP;
    let mut style = Vec::new();
    let max_width = (viewport.0 - gap * 2.0).max(0.0);
    if size.0 >= max_width {
        style.push(("width".into(), px(max_width)));
        style.push(("left".into(), px(gap)));
        style.push(("overflow-x".into(), "scroll".into()));
    } else {
        style.push((
            "left".into(),
            px(clamp(left, gap, viewport.0 - gap - size.0)),
        ));
    }
    let max_height = (viewport.1 - gap * 2.0).max(0.0);
    if size.1 >= max_height {
        style.push(("height".into(), px(max_height)));
        style.push(("top".into(), px(gap)));
        style.push(("overflow-y".into(), "scroll".into()));
    } else {
        style.push(("top".into(), px(clamp(top, gap, viewport.1 - gap - size.1))));
    }
    style
}

/// `div.popover` with `tabindex="-1"` (`Popover.tsx:150-154`). Once
/// mounted it takes focus unless something inside has it, traps Tab, fits
/// in the viewport when asked, and calls `on_close_request` on a
/// `pointerdown` outside it while it is in the document.
pub fn popover(props: PopoverProps, children: Vec<Node>) -> Element {
    let fit = match (props.fit_in_viewport, props.top, props.left) {
        (true, Some(top), Some(left)) => Some((top, left)),
        _ => None,
    };
    let (viewport_width, viewport_height) = (props.viewport_width, props.viewport_height);
    let on_close_request = props.on_close_request;
    Element::new("div")
        .attr(
            "class",
            class_names([
                ("popover", true),
                (props.class_name.as_deref().unwrap_or(""), true),
            ]),
        )
        .attr("tabindex", "-1")
        .children_from(children)
        .on("keydown", |e| {
            let Some(container) = e
                .current_target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            else {
                return;
            };
            trap_tab(e, &container);
        })
        .on_mount(move |el| {
            if let Some((top, left)) = fit {
                fit_in_viewport(el, top, left, viewport_width, viewport_height);
            }
            focus_unless_inside(el);
            if let Some(on_close_request) = &on_close_request {
                close_on_outside_press(el, on_close_request.clone());
            }
        })
}

fn trap_tab(e: &Event, container: &web_sys::Element) {
    let Some(key) = e.dyn_ref::<KeyboardEvent>() else {
        return;
    };
    if key.key() != "Tab" {
        return;
    }
    let focusables = focusable_elements(container);
    let active = container.owner_document().and_then(|d| d.active_element());
    let focus = match &active {
        Some(a) if a == container => TrapFocus::Container,
        Some(a) => focusables
            .iter()
            .position(|f| f.unchecked_ref::<web_sys::Element>() == a)
            .map_or(TrapFocus::Other, TrapFocus::Index),
        None => TrapFocus::Other,
    };
    if let Some(i) = popover_tab_target(focus, focusables.len(), key.shift_key()) {
        let _ = focusables[i].focus();
        e.prevent_default();
        e.stop_immediate_propagation();
    }
}

fn focus_unless_inside(el: &web_sys::Element) {
    let active = el.owner_document().and_then(|d| d.active_element());
    let inside = active.is_some_and(|a| el.contains(Some(&a)));
    if !inside {
        if let Some(html) = el.dyn_ref::<web_sys::HtmlElement>() {
            let _ = html.focus();
        }
    }
}

fn fit_in_viewport(
    el: &web_sys::Element,
    top: f64,
    left: f64,
    viewport_width: Option<f64>,
    viewport_height: Option<f64>,
) {
    let window = web_sys::window();
    let inner =
        |f: fn(&web_sys::Window) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>| {
            window
                .as_ref()
                .and_then(|w| f(w).ok())
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
        };
    let viewport = (
        viewport_width.unwrap_or_else(|| inner(web_sys::Window::inner_width)),
        viewport_height.unwrap_or_else(|| inner(web_sys::Window::inner_height)),
    );
    let rect = el.get_bounding_client_rect();
    let Some(html) = el.dyn_ref::<web_sys::HtmlElement>() else {
        return;
    };
    let style = html.style();
    for (property, value) in popover_fit_style(top, left, (rect.width(), rect.height()), viewport) {
        let _ = style.set_property(&property, &value);
    }
}

fn close_on_outside_press(el: &web_sys::Element, on_close_request: Handler) {
    let Some(document) = el.owner_document() else {
        return;
    };
    // the listener's own function, to remove it once the popover is gone
    let slot: Rc<RefCell<Option<js_sys::Function>>> = Rc::default();
    let own = slot.clone();
    let popover = el.clone();
    let target = document.clone();
    let closure = Closure::<dyn FnMut(Event)>::new(move |e: Event| {
        if !popover.is_connected() {
            if let Some(f) = own.borrow_mut().take() {
                let _ = target.remove_event_listener_with_callback("pointerdown", &f);
            }
            return;
        }
        let inside = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
            .is_some_and(|n| popover.contains(Some(&n)));
        if !inside {
            on_close_request(&e);
        }
    });
    let f: js_sys::Function = closure.into_js_value().unchecked_into();
    let _ = document.add_event_listener_with_callback("pointerdown", &f);
    *slot.borrow_mut() = Some(f);
}
