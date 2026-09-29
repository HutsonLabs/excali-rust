//! Browser key, mouse and clipboard events as `excali_editor::keyboard`
//! reads them: `event.key`, `event.code`, the modifiers and what the event
//! went to (`isWritableElement`, `isInputLike`, the focused container),
//! and the outcome applied back to the event (`preventDefault`,
//! `stopPropagation`).
//!
//! Upstream: `packages/common/src/utils.ts:69-121` (`isInputLike`,
//! `isWritableElement`), `components/App.tsx:5640-5645` (the container or
//! the convert popup has focus) and `common/src/constants.ts:116`
//! (`CLASSES.CONVERT_ELEMENT_TYPE_POPUP`).

use excali_editor::keyboard::{ClipboardTarget, KeyOutcome, KeyTarget, Keystroke, Modifiers};
use wasm_bindgen::JsCast;
use web_sys::{
    Document, Element, Event, EventTarget, HtmlBrElement, HtmlElement, HtmlInputElement,
    HtmlSelectElement, HtmlTextAreaElement, KeyboardEvent, MouseEvent,
};

/// `CLASSES.CONVERT_ELEMENT_TYPE_POPUP`.
pub const CONVERT_ELEMENT_TYPE_POPUP: &str = "ConvertElementTypePopup";

fn is_wysiwyg(target: &EventTarget) -> bool {
    target
        .dyn_ref::<HtmlElement>()
        .and_then(|e| e.get_attribute("data-type"))
        .as_deref()
        == Some("wysiwyg")
}

/// `isInputLike(target)` (`common/src/utils.ts:69-87`): the text editor, a
/// `<br>` in it, or any input, textarea or select.
pub fn is_input_like(target: Option<&EventTarget>) -> bool {
    let Some(t) = target else {
        return false;
    };
    is_wysiwyg(t)
        || t.is_instance_of::<HtmlBrElement>()
        || t.is_instance_of::<HtmlInputElement>()
        || t.is_instance_of::<HtmlTextAreaElement>()
        || t.is_instance_of::<HtmlSelectElement>()
}

/// `isWritableElement(target)` (`common/src/utils.ts:99-121`): the text
/// editor, a `<br>` in it, a textarea, a text, number, password or search
/// input, or anything inside a CodeMirror editor.
pub fn is_writable_element(target: Option<&EventTarget>) -> bool {
    let Some(t) = target else {
        return false;
    };
    is_wysiwyg(t)
        || t.is_instance_of::<HtmlBrElement>()
        || t.is_instance_of::<HtmlTextAreaElement>()
        || t.dyn_ref::<HtmlInputElement>().is_some_and(|i| {
            matches!(
                i.type_().as_str(),
                "text" | "number" | "password" | "search"
            )
        })
        || t.dyn_ref::<HtmlElement>()
            .is_some_and(|e| e.closest(".cm-editor").ok().flatten().is_some())
}

/// Whether `document.activeElement` is `container` or the convert element
/// type popup (`App.tsx:5640-5645`).
pub fn editor_focused(document: &Document, container: Option<&Element>) -> bool {
    let Some(active) = document.active_element() else {
        return false;
    };
    container.is_some_and(|c| c == &active)
        || active.class_list().contains(CONVERT_ELEMENT_TYPE_POPUP)
}

/// The modifiers of a mouse or pointer event.
pub fn mouse_modifiers(event: &MouseEvent) -> Modifiers {
    Modifiers {
        shift_key: event.shift_key(),
        alt_key: event.alt_key(),
        ctrl_key: event.ctrl_key(),
        meta_key: event.meta_key(),
    }
}

/// A `keydown` or `keyup` as [`excali_editor::keyboard`] reads it.
/// `container` is the editor's focusable container.
pub fn keystroke(event: &KeyboardEvent, container: Option<&Element>) -> Keystroke {
    let target = event.target();
    let focused = web_sys::window()
        .and_then(|w| w.document())
        .is_some_and(|d| editor_focused(&d, container));
    Keystroke {
        key: event.key(),
        code: event.code(),
        modifiers: Modifiers {
            shift_key: event.shift_key(),
            alt_key: event.alt_key(),
            ctrl_key: event.ctrl_key(),
            meta_key: event.meta_key(),
        },
        repeat: event.repeat(),
        target: KeyTarget {
            writable: is_writable_element(target.as_ref()),
            input_like: is_input_like(target.as_ref()),
            editor_focused: focused,
        },
    }
}

/// Where a `copy`, `cut` or `paste` event happened: the container holds
/// the focus, the focused element (paste) or target (copy, cut) is
/// writable, and whether a canvas is under `pointer` (paste).
pub fn clipboard_target(
    event: &Event,
    container: Option<&Element>,
    pointer: (f64, f64),
    paste: bool,
) -> ClipboardTarget {
    let document = web_sys::window().and_then(|w| w.document());
    let active = document.as_ref().and_then(Document::active_element);
    let editor_active = match (container, active.as_ref()) {
        (Some(c), Some(a)) => c.contains(Some(a)),
        _ => false,
    };
    let writable = if paste {
        is_writable_element(active.as_ref().map(|a| a.unchecked_ref::<EventTarget>()))
    } else {
        is_writable_element(event.target().as_ref())
    };
    let canvas_under_pointer = document
        .as_ref()
        .and_then(|d| d.element_from_point(pointer.0 as f32, pointer.1 as f32))
        .is_some_and(|e| e.is_instance_of::<web_sys::HtmlCanvasElement>());
    ClipboardTarget {
        editor_active,
        writable,
        canvas_under_pointer,
    }
}

/// Applies a key outcome to its event.
pub fn apply_outcome(event: &Event, outcome: &KeyOutcome) {
    if outcome.prevent_default {
        event.prevent_default();
    }
    if outcome.stop_propagation {
        event.stop_propagation();
    }
}
