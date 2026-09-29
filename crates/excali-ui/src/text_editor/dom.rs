//! The text editor's textarea in a document (`web-sys`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use excali_editor::text_editing::CaretRequest;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    AddEventListenerOptions, ClipboardEvent, Document, Element, Event, EventTarget, FocusOptions,
    HtmlElement, HtmlInputElement, HtmlTextAreaElement, KeyboardEvent, MouseEvent, ResizeObserver,
    Window,
};

use super::{
    caret_boundary_offsets, classify_pointer_down, closest_caret_offset, css_property_name,
    is_darwin, rearms_on_pointer_up, refocuses_on_scene_update, PointerDownAction,
    PointerDownTarget, TextareaEvent, TextareaHandler, TextareaKey, TextareaState,
    TEXTAREA_ATTRIBUTES,
};

type Callback = Closure<dyn FnMut(Event)>;

/// An event listener the overlay added, removed on unmount.
struct Listener {
    target: EventTarget,
    kind: &'static str,
    callback: Callback,
    capture: bool,
}

impl Listener {
    fn remove(&self) {
        let _ = if self.capture {
            self.target.remove_event_listener_with_callback_and_bool(
                self.kind,
                self.callback.as_ref().unchecked_ref(),
                true,
            )
        } else {
            self.target.remove_event_listener_with_callback(
                self.kind,
                self.callback.as_ref().unchecked_ref(),
            )
        };
    }
}

/// What the overlay's listeners share.
struct Shared {
    window: Window,
    textarea: HtmlTextAreaElement,
    editor_box: HtmlElement,
    handler: Rc<RefCell<dyn TextareaHandler>>,
    /// `editable.onblur = handleSubmit` is set.
    blur_armed: Cell<bool>,
    /// The editor closed: the listeners ignore their events.
    closed: Cell<bool>,
    /// `KEYS.CTRL_OR_CMD` is `metaKey`.
    darwin: bool,
    /// Listeners added while the blur submit is suspended.
    suspended: RefCell<Vec<Listener>>,
}

impl Shared {
    fn selection(&self) -> (usize, usize) {
        let start = self.textarea.selection_start().ok().flatten().unwrap_or(0);
        let end = self
            .textarea
            .selection_end()
            .ok()
            .flatten()
            .unwrap_or(start);
        (start as usize, end as usize)
    }

    /// Hands `event` to the app and applies its answer; whether to prevent
    /// the browser's default.
    fn dispatch(&self, event: TextareaEvent) -> bool {
        if self.closed.get() {
            return false;
        }
        let handled = self.handler.borrow_mut().on_event(event);
        if let Some(state) = &handled.state {
            self.apply(state);
        }
        handled.prevent_default
    }

    fn apply(&self, state: &TextareaState) {
        if !state.open {
            // cleanup: the listeners ignore what the removal fires
            self.closed.set(true);
            self.textarea.remove();
            let style = self.editor_box.style();
            let _ = style.remove_property("left");
            let _ = style.remove_property("right");
            return;
        }
        if self.textarea.value() != state.value {
            self.textarea.set_value(&state.value);
        }
        if self.selection() != state.selection {
            let _ = self
                .textarea
                .set_selection_range(state.selection.0 as u32, state.selection.1 as u32);
        }
        // in assignment order: `line-height` after the `font` shorthand
        let style = self.textarea.style();
        for (key, value) in &state.style {
            let _ = style.set_property(&css_property_name(key), value);
        }
        let box_style = self.editor_box.style();
        let (left, right) = state.editor_box_insets;
        let _ = box_style.set_property("left", &px(left));
        let _ = box_style.set_property("right", &px(right));
    }

    /// Focus without scrolling the editor's box (`focus({preventScroll})`).
    fn focus(&self) {
        let options = FocusOptions::new();
        options.set_prevent_scroll(true);
        let _ = self.textarea.focus_with_options(&options);
    }

    /// `bindBlurEvent`'s deferred part: a blur submits from now on, the
    /// textarea is focused and a pending caret placed.
    fn arm(self: &Rc<Self>) {
        let shared = Rc::clone(self);
        let armed = Closure::once_into_js(move || {
            if shared.closed.get() {
                return;
            }
            shared.blur_armed.set(true);
            shared.focus();
            shared.dispatch(TextareaEvent::Focused);
        });
        let _ = self.window.set_timeout_with_callback(armed.unchecked_ref());
    }

    /// `temporarilyDisableSubmit()`: no blur submit until the pointer is
    /// released (then re-armed unless released in the panel), and the
    /// window's blur submits (the pointer up may never come).
    fn suspend_submit(self: &Rc<Self>) {
        self.blur_armed.set(false);
        let window: EventTarget = self.window.clone().into();
        let shared = Rc::clone(self);
        let up: Callback = Closure::new(move |event: Event| {
            let listeners = std::mem::take(&mut *shared.suspended.borrow_mut());
            let (ups, others): (Vec<_>, Vec<_>) =
                listeners.into_iter().partition(|l| l.kind == "pointerup");
            *shared.suspended.borrow_mut() = others;
            for l in &ups {
                l.remove();
            }
            if rearms_on_pointer_up(&target_of(&event)) {
                shared.arm();
            }
        });
        let shared = Rc::clone(self);
        let blur: Callback = Closure::new(move |_: Event| {
            shared.dispatch(TextareaEvent::Submit);
        });
        let mut suspended = self.suspended.borrow_mut();
        for (kind, callback) in [("pointerup", up), ("blur", blur)] {
            let _ =
                window.add_event_listener_with_callback(kind, callback.as_ref().unchecked_ref());
            suspended.push(Listener {
                target: window.clone(),
                kind,
                callback,
                capture: false,
            });
        }
    }
}

fn px(x: f64) -> String {
    format!("{}px", excali_core::json::number_to_string(x))
}

/// `event.target` as `onPointerDown` reads it.
fn target_of(event: &Event) -> PointerDownTarget {
    let button = event.dyn_ref::<MouseEvent>().map_or(0, MouseEvent::button);
    match event.target().and_then(|t| t.dyn_into::<Element>().ok()) {
        Some(element) => element_target(&element, button),
        None => PointerDownTarget {
            button,
            ..PointerDownTarget::default()
        },
    }
}

/// `element` as `onPointerDown` and the scene update's popup check read it.
fn element_target(element: &Element, button: i16) -> PointerDownTarget {
    let tag = element.tag_name().to_ascii_lowercase();
    let closest = |selector: &str| element.closest(selector).ok().flatten().is_some();
    PointerDownTarget {
        button,
        textarea: tag == "textarea",
        canvas: tag == "canvas",
        properties_trigger: element.class_list().contains("properties-trigger"),
        in_properties_content: closest(".properties-content"),
        in_actions_menu: closest(".App-menu__left, .zoom-actions")
            || closest(".compact-shape-actions-island"),
        writable: is_writable(element, &tag),
    }
}

/// `isWritableElement(target)` (`packages/common/src/utils.ts:99-121`).
fn is_writable(element: &Element, tag: &str) -> bool {
    let wysiwyg = element
        .dyn_ref::<HtmlElement>()
        .and_then(|e| e.dataset().get("type"))
        .is_some_and(|t| t == "wysiwyg");
    let text_input = element.dyn_ref::<HtmlInputElement>().is_some_and(|i| {
        matches!(
            i.type_().as_str(),
            "text" | "number" | "password" | "search"
        )
    });
    wysiwyg
        || tag == "br"
        || tag == "textarea"
        || text_input
        || element.closest(".cm-editor").ok().flatten().is_some()
}

/// The text editor's textarea mounted in the editor's box, its events
/// going to a [`TextareaHandler`].
pub struct TextEditorOverlay {
    shared: Rc<Shared>,
    listeners: Vec<Listener>,
    resize_observer: Option<(ResizeObserver, Closure<dyn FnMut()>)>,
}

impl TextEditorOverlay {
    /// Creates the textarea, applies `state`, appends it to `editor_box`
    /// (the editor's `.excalidraw-textEditorContainer`) and wires its
    /// events to `handler`. `canvas` is observed for resizes (the window's
    /// `resize` without it).
    pub fn mount(
        editor_box: &HtmlElement,
        canvas: Option<&Element>,
        state: &TextareaState,
        handler: Rc<RefCell<dyn TextareaHandler>>,
    ) -> Result<TextEditorOverlay, JsValue> {
        let document = editor_box
            .owner_document()
            .ok_or_else(|| JsValue::from_str("the editor box is not in a document"))?;
        let window = document
            .default_view()
            .ok_or_else(|| JsValue::from_str("the document has no window"))?;
        let textarea: HtmlTextAreaElement = document.create_element("textarea")?.dyn_into()?;
        textarea.set_dir(TEXTAREA_ATTRIBUTES.dir);
        textarea.set_tab_index(TEXTAREA_ATTRIBUTES.tab_index);
        textarea
            .dataset()
            .set("type", TEXTAREA_ATTRIBUTES.data_type)?;
        // no line wrapping in Safari
        textarea.set_wrap(TEXTAREA_ATTRIBUTES.wrap);
        textarea
            .class_list()
            .add_1(TEXTAREA_ATTRIBUTES.class_name)?;
        let darwin = window.navigator().platform().is_ok_and(|p| is_darwin(&p));
        let shared = Rc::new(Shared {
            window: window.clone(),
            textarea: textarea.clone(),
            editor_box: editor_box.clone(),
            handler,
            blur_armed: Cell::new(false),
            closed: Cell::new(false),
            darwin,
            suspended: RefCell::new(Vec::new()),
        });
        shared.apply(state);

        let mut overlay = TextEditorOverlay {
            shared: Rc::clone(&shared),
            listeners: Vec::new(),
            resize_observer: None,
        };
        let area: EventTarget = textarea.clone().into();
        let win: EventTarget = window.clone().into();
        let boxed: EventTarget = editor_box.clone().into();

        let s = Rc::clone(&shared);
        overlay.listen(&area, "input", false, move |_| {
            let value = s.textarea.value();
            let selection = s.selection();
            s.dispatch(TextareaEvent::Input { value, selection });
        })?;
        let s = Rc::clone(&shared);
        overlay.listen(&area, "selectionchange", false, move |_| {
            let selection = s.selection();
            s.dispatch(TextareaEvent::Select { selection });
        })?;
        let s = Rc::clone(&shared);
        overlay.listen(&area, "keydown", false, move |event| {
            let Some(k) = event.dyn_ref::<KeyboardEvent>() else {
                return;
            };
            let key = TextareaKey {
                key: k.key(),
                code: k.code(),
                shift_key: k.shift_key(),
                alt_key: k.alt_key(),
                ctrl_or_cmd: if s.darwin { k.meta_key() } else { k.ctrl_key() },
                is_composing: k.is_composing(),
                key_code: k.key_code(),
            };
            let selection = s.selection();
            if s.dispatch(TextareaEvent::KeyDown { key, selection }) {
                event.prevent_default();
            }
        })?;
        let s = Rc::clone(&shared);
        overlay.listen(&area, "paste", false, move |event| {
            let Some(data) = event
                .dyn_ref::<ClipboardEvent>()
                .and_then(ClipboardEvent::clipboard_data)
            else {
                return;
            };
            let items = data.items();
            let mut types: Vec<String> = Vec::new();
            for i in 0..items.length() {
                if let Some(item) = items.get(i) {
                    let kind = item.type_();
                    if !types.contains(&kind) {
                        types.push(kind);
                    }
                }
            }
            let text = types
                .iter()
                .any(|t| t == "text/plain")
                .then(|| data.get_data("text/plain").unwrap_or_default());
            let selection = s.selection();
            if s.dispatch(TextareaEvent::Paste {
                types,
                text,
                selection,
            }) {
                event.prevent_default();
            }
        })?;
        let s = Rc::clone(&shared);
        overlay.listen(&area, "blur", false, move |_| {
            if s.blur_armed.get() {
                s.dispatch(TextareaEvent::Submit);
            }
        })?;
        overlay.listen(&area, "pointerdown", false, |event| {
            event.stop_propagation();
        })?;
        let s = Rc::clone(&shared);
        overlay.listen(&boxed, "scroll", false, move |_| {
            let (left, top) = (
                f64::from(s.editor_box.scroll_left()),
                f64::from(s.editor_box.scroll_top()),
            );
            if left == 0.0 && top == 0.0 {
                return;
            }
            s.editor_box.set_scroll_left(0);
            s.editor_box.set_scroll_top(0);
            s.dispatch(TextareaEvent::EditorBoxScrolled { left, top });
        })?;
        let s = Rc::clone(&shared);
        overlay.listen(&win, "beforeunload", false, move |_| {
            s.dispatch(TextareaEvent::Submit);
        })?;

        match canvas {
            Some(canvas) => {
                let s = Rc::clone(&shared);
                let callback: Closure<dyn FnMut()> = Closure::new(move || {
                    s.dispatch(TextareaEvent::Resized);
                });
                let observer = ResizeObserver::new(callback.as_ref().unchecked_ref())?;
                observer.observe(canvas);
                overlay.resize_observer = Some((observer, callback));
            }
            None => {
                let s = Rc::clone(&shared);
                overlay.listen(&win, "resize", false, move |_| {
                    s.dispatch(TextareaEvent::Resized);
                })?;
            }
        }

        // the window's pointer downs, from the next frame so the one that
        // opened the editor is not caught
        let s = Rc::clone(&shared);
        let pointer_down: Callback = Closure::new(move |event: Event| {
            if s.closed.get() {
                return;
            }
            let target = target_of(&event);
            match classify_pointer_down(&target) {
                PointerDownAction::Pan { on_textarea } => {
                    if on_textarea {
                        event.prevent_default();
                        let (client_x, client_y) =
                            event.dyn_ref::<MouseEvent>().map_or((0.0, 0.0), |m| {
                                (f64::from(m.client_x()), f64::from(m.client_y()))
                            });
                        s.dispatch(TextareaEvent::PanStart { client_x, client_y });
                    }
                    s.suspend_submit();
                }
                PointerDownAction::SuspendSubmit => s.suspend_submit(),
                PointerDownAction::SubmitNextFrame => {
                    let next = Rc::clone(&s);
                    let submit = Closure::once_into_js(move || {
                        next.dispatch(TextareaEvent::Submit);
                    });
                    let _ = s.window.request_animation_frame(submit.unchecked_ref());
                }
                PointerDownAction::Nothing => {}
            }
        });
        let options = AddEventListenerOptions::new();
        options.set_capture(true);
        let add_pointer_down = {
            let win = win.clone();
            let callback = pointer_down.as_ref().clone();
            Closure::once_into_js(move || {
                let _ = win.add_event_listener_with_callback_and_add_event_listener_options(
                    "pointerdown",
                    callback.unchecked_ref(),
                    &options,
                );
            })
        };
        let _ = window.request_animation_frame(add_pointer_down.unchecked_ref());
        overlay.listeners.push(Listener {
            target: win,
            kind: "pointerdown",
            callback: pointer_down,
            capture: true,
        });

        editor_box.append_child(&textarea)?;
        shared.arm();
        Ok(overlay)
    }

    fn listen(
        &mut self,
        target: &EventTarget,
        kind: &'static str,
        capture: bool,
        f: impl FnMut(Event) + 'static,
    ) -> Result<(), JsValue> {
        let callback: Callback = Closure::new(f);
        target.add_event_listener_with_callback_and_bool(
            kind,
            callback.as_ref().unchecked_ref(),
            capture,
        )?;
        self.listeners.push(Listener {
            target: target.clone(),
            kind,
            callback,
            capture,
        });
        Ok(())
    }

    /// The textarea.
    pub fn textarea(&self) -> &HtmlTextAreaElement {
        &self.shared.textarea
    }

    /// Applies the editor's state (after the app changed it outside the
    /// textarea's events: a scroll or app state change, a submit). A scene
    /// change goes through [`TextEditorOverlay::scene_updated`], which also
    /// takes the focus back.
    pub fn apply(&self, state: &TextareaState) {
        if !self.shared.closed.get() {
            self.shared.apply(state);
        }
    }

    /// The scene changed (`app.scene.onUpdate`, `textWysiwyg.tsx:1053-1061`):
    /// applies the editor's state after its `updateWysiwygStyle()`
    /// (`TextEditor::relayout`), then focuses the textarea without
    /// scrolling, unless the focused element is inside a properties popover
    /// (`.properties-content`, [`refocuses_on_scene_update`]). The app calls
    /// it for every scene change while the editor is open: a style change
    /// from the panel, a collaborator's edit, the container moved.
    pub fn scene_updated(&self, state: &TextareaState) {
        if self.shared.closed.get() {
            return;
        }
        self.shared.apply(state);
        if self.shared.closed.get() {
            return;
        }
        let active = self
            .shared
            .textarea
            .owner_document()
            .and_then(|d| d.active_element())
            .map_or_else(PointerDownTarget::default, |e| element_target(&e, 0));
        if refocuses_on_scene_update(&active) {
            self.shared.focus();
        }
    }

    /// Whether the textarea is gone (the editor closed).
    pub fn is_closed(&self) -> bool {
        self.shared.closed.get()
    }

    /// Removes the textarea and every listener.
    pub fn unmount(self) {
        self.shared.closed.set(true);
        self.shared.textarea.remove();
        for listener in &self.listeners {
            listener.remove();
        }
        for listener in self.shared.suspended.borrow_mut().drain(..) {
            listener.remove();
        }
        if let Some((observer, _)) = &self.resize_observer {
            observer.disconnect();
        }
    }
}

/// `getLineCaretOffsetFromNativeLayout({...})` (`textWysiwyg.tsx:131-205`):
/// the offset in `request`'s line whose caret is closest to the point, as
/// the page lays the line out (a hidden mirror in the line's font, line
/// height and direction, measured with a collapsed `Range` at every code
/// point boundary). `None` for an empty line or when the page cannot
/// measure.
pub fn measure_caret_offset(document: &Document, request: &CaretRequest) -> Option<usize> {
    let text = &request.line_text;
    if text.is_empty() {
        return None;
    }
    let body = document.body()?;
    let offsets = caret_boundary_offsets(text);
    let mirror: HtmlElement = document.create_element("div").ok()?.dyn_into().ok()?;
    let node = document.create_text_node(text);
    let range = document.create_range().ok()?;
    mirror.set_dir(request.direction);
    let style = mirror.style();
    for (key, value) in [
        ("position", "fixed".to_owned()),
        ("top", "0".to_owned()),
        ("left", "0".to_owned()),
        ("margin", "0".to_owned()),
        ("padding", "0".to_owned()),
        ("border", "0".to_owned()),
        ("opacity", "0".to_owned()),
        ("pointer-events", "none".to_owned()),
        ("white-space", "pre".to_owned()),
        ("font", request.font.clone()),
        ("line-height", px(request.line_height_px)),
    ] {
        let _ = style.set_property(key, &value);
    }
    mirror.append_child(&node).ok()?;
    body.append_child(&mirror).ok()?;
    let mut positions = Vec::with_capacity(offsets.len());
    let mut measured = true;
    for offset in &offsets {
        let offset = *offset as u32;
        if range.set_start(&node, offset).is_err() || range.set_end(&node, offset).is_err() {
            measured = false;
            break;
        }
        let left = range.get_bounding_client_rect().left();
        if !left.is_finite() {
            measured = false;
            break;
        }
        positions.push(left);
    }
    mirror.remove();
    measured.then(|| closest_caret_offset(&offsets, &positions, request.target_x))
}
