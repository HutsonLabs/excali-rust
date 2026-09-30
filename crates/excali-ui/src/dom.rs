//! A small DOM builder: element trees (tag, attributes, inline style,
//! children, event listeners) built as plain values, so the chrome's
//! markup is testable natively, and mounted into a document with
//! `web-sys`.
//!
//! The primitives ([`crate::primitives`]) are functions from props to an
//! [`Element`], as upstream's components are functions from props to JSX.
//! Attributes keep the order they are set in; setting one twice replaces
//! it, as `setAttribute` does. [`class_names`] is `clsx` for the
//! `(name, condition)` pairs upstream passes it.

use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Document, Event, EventTarget};

/// `http://www.w3.org/2000/svg`, the namespace of SVG elements.
pub const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";

/// An event listener: called with the event, `currentTarget` being the
/// element it is on.
pub type Handler = Rc<dyn Fn(&Event)>;

/// Called once the element is in the document (a layout effect).
pub type MountHook = Rc<dyn Fn(&web_sys::Element)>;

/// What a data listener ([`Element::on_data`]) reads from its event: the
/// key and modifiers, and the target's value. Decoded from the DOM event
/// when mounted; built directly to [`Element::dispatch`] natively.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventData {
    /// `KeyboardEvent.key` (empty for other events).
    pub key: String,
    pub alt_key: bool,
    pub ctrl_key: bool,
    pub meta_key: bool,
    pub shift_key: bool,
    /// `event.target.value` for an input.
    pub value: Option<String>,
}

impl EventData {
    /// The event's data (modifiers from a keyboard or mouse event).
    pub fn from_event(e: &Event) -> EventData {
        let mut data = EventData::default();
        if let Some(k) = e.dyn_ref::<web_sys::KeyboardEvent>() {
            data.key = k.key();
            data.alt_key = k.alt_key();
            data.ctrl_key = k.ctrl_key();
            data.meta_key = k.meta_key();
            data.shift_key = k.shift_key();
        } else if let Some(m) = e.dyn_ref::<web_sys::MouseEvent>() {
            data.alt_key = m.alt_key();
            data.ctrl_key = m.ctrl_key();
            data.meta_key = m.meta_key();
            data.shift_key = m.shift_key();
        }
        data.value = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            .map(|i| i.value());
        data
    }
}

/// What a data listener asks of its event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventResponse {
    /// `event.preventDefault()`.
    pub prevent_default: bool,
    /// `event.stopPropagation()` and `stopImmediatePropagation()`.
    pub stop_propagation: bool,
}

/// A listener on the event's [`EventData`].
pub type DataHandler = Rc<dyn Fn(&EventData) -> EventResponse>;

/// A node of a tree: an element or text.
#[derive(Clone)]
pub enum Node {
    Element(Element),
    Text(String),
}

impl Node {
    /// A text node.
    pub fn text(text: impl Into<String>) -> Node {
        Node::Text(text.into())
    }

    /// The element, if this is one.
    pub fn as_element(&self) -> Option<&Element> {
        match self {
            Node::Element(el) => Some(el),
            Node::Text(_) => None,
        }
    }

    /// The node as HTML (text and attribute values escaped), for
    /// diagnostics and server rendering. Inline style is written as the
    /// `style` attribute, `name: value;` pairs joined by a space.
    pub fn to_html(&self) -> String {
        let mut out = String::new();
        self.write_html(&mut out);
        out
    }

    fn write_html(&self, out: &mut String) {
        match self {
            Node::Text(text) => escape(text, false, out),
            Node::Element(el) => el.write_html(out),
        }
    }
}

impl From<Element> for Node {
    fn from(el: Element) -> Node {
        Node::Element(el)
    }
}

impl From<&str> for Node {
    fn from(text: &str) -> Node {
        Node::text(text)
    }
}

impl From<String> for Node {
    fn from(text: String) -> Node {
        Node::Text(text)
    }
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_html())
    }
}

/// An element: tag, namespace, attributes, inline style, children and
/// listeners.
#[derive(Clone)]
pub struct Element {
    tag: String,
    svg: bool,
    attributes: Vec<(String, String)>,
    style: Vec<(String, String)>,
    children: Vec<Node>,
    listeners: Vec<(String, Handler)>,
    data_listeners: Vec<(String, DataHandler)>,
    on_mount: Vec<MountHook>,
}

impl Element {
    /// An HTML element.
    pub fn new(tag: impl Into<String>) -> Element {
        Element {
            tag: tag.into(),
            svg: false,
            attributes: Vec::new(),
            style: Vec::new(),
            children: Vec::new(),
            listeners: Vec::new(),
            data_listeners: Vec::new(),
            on_mount: Vec::new(),
        }
    }

    /// An SVG element (created in [`SVG_NAMESPACE`]).
    pub fn svg(tag: impl Into<String>) -> Element {
        Element {
            svg: true,
            ..Element::new(tag)
        }
    }

    /// Sets an attribute, replacing a previous value.
    pub fn attr(mut self, name: impl Into<String>, value: impl Into<String>) -> Element {
        let (name, value) = (name.into(), value.into());
        match self.attributes.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = value,
            None => self.attributes.push((name, value)),
        }
        self
    }

    /// Sets an attribute when `value` is `Some`, as React skips an
    /// `undefined` prop.
    pub fn attr_opt(self, name: impl Into<String>, value: Option<impl Into<String>>) -> Element {
        match value {
            Some(value) => self.attr(name, value),
            None => self,
        }
    }

    /// A boolean attribute (`hidden`, `disabled`, ...): present and empty
    /// when `on`.
    pub fn flag(self, name: impl Into<String>, on: bool) -> Element {
        if on {
            self.attr(name, "")
        } else {
            self
        }
    }

    /// Sets an inline style property (`--custom` or a CSS property name),
    /// replacing a previous value.
    pub fn style(mut self, property: impl Into<String>, value: impl Into<String>) -> Element {
        let (property, value) = (property.into(), value.into());
        match self.style.iter_mut().find(|(p, _)| *p == property) {
            Some(slot) => slot.1 = value,
            None => self.style.push((property, value)),
        }
        self
    }

    /// Sets an inline style property when `value` is `Some`.
    pub fn style_opt(
        self,
        property: impl Into<String>,
        value: Option<impl Into<String>>,
    ) -> Element {
        match value {
            Some(value) => self.style(property, value),
            None => self,
        }
    }

    /// Sets each `(property, value)` in order, as a spread `style` prop.
    pub fn styles(self, style: impl IntoIterator<Item = (String, String)>) -> Element {
        style.into_iter().fold(self, |el, (p, v)| el.style(p, v))
    }

    /// Appends a child.
    pub fn child(mut self, child: impl Into<Node>) -> Element {
        self.children.push(child.into());
        self
    }

    /// Appends a child when there is one.
    pub fn child_opt(self, child: Option<impl Into<Node>>) -> Element {
        match child {
            Some(child) => self.child(child),
            None => self,
        }
    }

    /// Appends children in order.
    pub fn children_from(mut self, children: impl IntoIterator<Item = Node>) -> Element {
        self.children.extend(children);
        self
    }

    /// Adds a listener for `event` (`click`, `keydown`, ...).
    pub fn on(mut self, event: impl Into<String>, handler: impl Fn(&Event) + 'static) -> Element {
        self.listeners.push((event.into(), Rc::new(handler)));
        self
    }

    /// Adds a listener for `event` on its [`EventData`]; mounted, it
    /// applies the returned [`EventResponse`] to the DOM event. Natively,
    /// [`Element::dispatch`] runs it.
    pub fn on_data(
        mut self,
        event: impl Into<String>,
        handler: impl Fn(&EventData) -> EventResponse + 'static,
    ) -> Element {
        let event = event.into();
        let handler: DataHandler = Rc::new(handler);
        self.data_listeners.push((event.clone(), handler.clone()));
        self.on(event, move |e| {
            let response = handler(&EventData::from_event(e));
            if response.prevent_default {
                e.prevent_default();
            }
            if response.stop_propagation {
                e.stop_immediate_propagation();
                e.stop_propagation();
            }
        })
    }

    /// Runs the element's data listeners for `event` with `data`, in
    /// order, merging their responses; `None` when it has none.
    pub fn dispatch(&self, event: &str, data: &EventData) -> Option<EventResponse> {
        let mut out: Option<EventResponse> = None;
        for (e, handler) in &self.data_listeners {
            if e == event {
                let r = handler(data);
                let acc = out.get_or_insert_with(EventResponse::default);
                acc.prevent_default |= r.prevent_default;
                acc.stop_propagation |= r.stop_propagation;
            }
        }
        out
    }

    /// The events data listeners are on, in order.
    pub fn data_events(&self) -> impl Iterator<Item = &str> {
        self.data_listeners.iter().map(|(e, _)| e.as_str())
    }

    /// Runs `hook` with the element once the tree is mounted, children
    /// first (as React runs layout effects).
    pub fn on_mount(mut self, hook: impl Fn(&web_sys::Element) + 'static) -> Element {
        self.on_mount.push(Rc::new(hook));
        self
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }

    pub fn is_svg(&self) -> bool {
        self.svg
    }

    /// The attributes in the order they were set.
    pub fn attributes(&self) -> &[(String, String)] {
        &self.attributes
    }

    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The inline style in the order it was set.
    pub fn style_properties(&self) -> &[(String, String)] {
        &self.style
    }

    pub fn children(&self) -> &[Node] {
        &self.children
    }

    /// Whether a hook runs once the element is mounted
    /// ([`Element::on_mount`]).
    pub fn has_mount_hook(&self) -> bool {
        !self.on_mount.is_empty()
    }

    /// The events listened for, in order.
    pub fn listened_events(&self) -> impl Iterator<Item = &str> {
        self.listeners.iter().map(|(e, _)| e.as_str())
    }

    /// See [`Node::to_html`].
    pub fn to_html(&self) -> String {
        let mut out = String::new();
        self.write_html(&mut out);
        out
    }

    fn write_html(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.tag);
        for (name, value) in &self.attributes {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            escape(value, true, out);
            out.push('"');
        }
        if !self.style.is_empty() {
            out.push_str(" style=\"");
            escape(&self.style_text(), true, out);
            out.push('"');
        }
        out.push('>');
        if is_void(&self.tag) && !self.svg {
            return;
        }
        for child in &self.children {
            child.write_html(out);
        }
        out.push_str("</");
        out.push_str(&self.tag);
        out.push('>');
    }

    fn style_text(&self) -> String {
        self.style
            .iter()
            .map(|(p, v)| format!("{p}: {v};"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl std::fmt::Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_html())
    }
}

fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "source"
            | "track"
            | "wbr"
    )
}

/// HTML serialization's escaping (`&`, nbsp, and `"` in attributes or
/// `<` `>` in text).
fn escape(text: &str, attribute: bool, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            '"' if attribute => out.push_str("&quot;"),
            '<' if !attribute => out.push_str("&lt;"),
            '>' if !attribute => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

/// `clsx(...)` over `(class, condition)` pairs: the non-empty classes whose
/// condition holds, space separated.
pub fn class_names<'a>(parts: impl IntoIterator<Item = (&'a str, bool)>) -> String {
    let mut out = String::new();
    for (class, on) in parts {
        if on && !class.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(class);
        }
    }
    out
}

/// A mounted tree: its root node, and the listeners it holds. Dropping it
/// removes the listeners (the nodes stay where they are; see
/// [`Mounted::remove`]).
pub struct Mounted {
    root: web_sys::Node,
    listeners: Listeners,
}

impl Mounted {
    pub fn root(&self) -> &web_sys::Node {
        &self.root
    }

    /// The root as an element (`None` for a text root).
    pub fn element(&self) -> Option<web_sys::Element> {
        self.root.dyn_ref::<web_sys::Element>().cloned()
    }

    /// Detaches the root from its parent and drops the listeners.
    pub fn remove(self) {
        if let Some(parent) = self.root.parent_node() {
            let _ = parent.remove_child(&self.root);
        }
    }
}

impl Drop for Mounted {
    fn drop(&mut self) {
        for (target, event, closure) in &self.listeners {
            let _ =
                target.remove_event_listener_with_callback(event, closure.as_ref().unchecked_ref());
        }
    }
}

/// Creates `node`'s DOM in `document` and appends it to `parent`, then
/// runs the tree's mount hooks (children before parents, in order).
pub fn mount(node: &Node, document: &Document, parent: &web_sys::Node) -> Result<Mounted, JsValue> {
    let mut listeners = Vec::new();
    let mut hooks = Vec::new();
    let root = create(node, document, &mut listeners, &mut hooks)?;
    parent.append_child(&root)?;
    for (el, hook) in hooks {
        hook(&el);
    }
    Ok(Mounted { root, listeners })
}

/// The mount hooks of a tree mounted with [`mount_deferred`], to run once
/// it is in the document.
#[must_use]
pub struct PendingHooks(Hooks);

impl PendingHooks {
    /// Runs the hooks (children before parents, in order).
    pub fn run(self) {
        for (el, hook) in self.0 {
            hook(&el);
        }
    }
}

/// [`mount`] into a `parent` that is not in the document yet (a fragment):
/// the tree's mount hooks are returned, to run once it is.
pub fn mount_deferred(
    node: &Node,
    document: &Document,
    parent: &web_sys::Node,
) -> Result<(Mounted, PendingHooks), JsValue> {
    let mut listeners = Vec::new();
    let mut hooks = Vec::new();
    let root = create(node, document, &mut listeners, &mut hooks)?;
    parent.append_child(&root)?;
    Ok((Mounted { root, listeners }, PendingHooks(hooks)))
}

/// Creates `node`'s DOM without attaching it; its listeners stay attached
/// for as long as the page lives (for a subtree replacing part of a
/// mounted one, such as a toggled icon).
pub fn create_detached(node: &Node, document: &Document) -> Result<web_sys::Node, JsValue> {
    let mut listeners = Vec::new();
    let mut hooks = Vec::new();
    let root = create(node, document, &mut listeners, &mut hooks)?;
    for (_, _, closure) in listeners {
        closure.forget();
    }
    Ok(root)
}

type Listeners = Vec<(EventTarget, String, Closure<dyn FnMut(Event)>)>;
type Hooks = Vec<(web_sys::Element, MountHook)>;

fn create(
    node: &Node,
    document: &Document,
    listeners: &mut Listeners,
    hooks: &mut Hooks,
) -> Result<web_sys::Node, JsValue> {
    let el = match node {
        Node::Text(text) => return Ok(document.create_text_node(text).into()),
        Node::Element(el) => el,
    };
    let dom = if el.svg {
        document.create_element_ns(Some(SVG_NAMESPACE), &el.tag)?
    } else {
        document.create_element(&el.tag)?
    };
    for (name, value) in &el.attributes {
        dom.set_attribute(name, value)?;
    }
    if !el.style.is_empty() {
        let style = dom
            .dyn_ref::<web_sys::HtmlElement>()
            .map(|h| h.style())
            .or_else(|| dom.dyn_ref::<web_sys::SvgElement>().map(|s| s.style()));
        match style {
            Some(style) => {
                for (property, value) in &el.style {
                    style.set_property(property, value)?;
                }
            }
            None => dom.set_attribute("style", &el.style_text())?,
        }
    }
    for child in &el.children {
        dom.append_child(&create(child, document, listeners, hooks)?)?;
    }
    for (event, handler) in &el.listeners {
        let handler = handler.clone();
        let closure = Closure::<dyn FnMut(Event)>::new(move |e: Event| handler(&e));
        dom.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref())?;
        listeners.push((dom.clone().into(), event.clone(), closure));
    }
    for hook in &el.on_mount {
        hooks.push((dom.clone(), hook.clone()));
    }
    Ok(dom.into())
}
