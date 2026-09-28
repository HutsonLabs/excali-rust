//! The few DOM nodes an SVG export is made of, and their `outerHTML`.
//!
//! Upstream builds the document with `document.createElementNS(SVG_NS, …)`,
//! `createComment` and `createTextNode` and saves `svgRoot.outerHTML`
//! (`packages/excalidraw/data/index.ts:139-146`). The page is an HTML
//! document, so the markup is the HTML fragment serialization algorithm's
//! (HTML Standard, "Serializing HTML fragments"), as jsdom 22.1.0 (parse5)
//! writes it in upstream's own tests:
//!
//! - every tag has an end tag (SVG elements are not void elements);
//! - attributes in the order they were first set, the value with `&`,
//!   U+00A0 and `"` escaped (`&amp;`, `&nbsp;`, `&quot;`);
//! - text with `&`, U+00A0, `<` and `>` escaped (an SVG `<style>` is not
//!   an HTML raw-text element);
//! - a comment as `<!--data-->`, unescaped.

/// A node: a tag, a text node or a comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Tag(Tag),
    /// A text node's data.
    Text(String),
    /// A comment's data (upstream's `createHTMLComment` pads it with a
    /// space on each side).
    Comment(String),
}

impl From<Tag> for Node {
    fn from(tag: Tag) -> Node {
        Node::Tag(tag)
    }
}

/// A tag: its name, its attributes in the order they were first set, and
/// its children.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    attributes: Vec<(String, String)>,
    children: Vec<Node>,
}

impl Tag {
    /// `createElementNS(SVG_NS, name)`.
    pub fn new(name: impl Into<String>) -> Tag {
        Tag {
            name: name.into(),
            attributes: Vec::new(),
            children: Vec::new(),
        }
    }

    /// `setAttribute(name, value)`: a new attribute goes last, an existing
    /// one keeps its place.
    pub fn set_attribute(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        match self.attributes.iter_mut().find(|(n, _)| *n == name) {
            Some((_, v)) => *v = value,
            None => self.attributes.push((name, value)),
        }
    }

    /// `getAttribute(name)`.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The attributes, in order.
    pub fn attributes(&self) -> &[(String, String)] {
        &self.attributes
    }

    /// `classList.add(class)`: appended to `class` unless already there.
    pub fn add_class(&mut self, class: &str) {
        let classes: Vec<&str> = self
            .attribute("class")
            .map(|c| c.split_ascii_whitespace().collect())
            .unwrap_or_default();
        if classes.contains(&class) {
            return;
        }
        let mut next: Vec<&str> = classes;
        next.push(class);
        let value = next.join(" ");
        self.set_attribute("class", value);
    }

    /// `appendChild(node)`.
    pub fn append(&mut self, node: impl Into<Node>) {
        self.children.push(node.into());
    }

    /// The child nodes, in order.
    pub fn children(&self) -> &[Node] {
        &self.children
    }

    /// The child nodes, mutably, for a writer adding to an element it
    /// already appended.
    pub fn children_mut(&mut self) -> &mut Vec<Node> {
        &mut self.children
    }

    /// `outerHTML`.
    pub fn outer_html(&self) -> String {
        let mut out = String::new();
        write_tag(self, &mut out);
        out
    }

    /// `innerHTML`.
    pub fn inner_html(&self) -> String {
        let mut out = String::new();
        for child in &self.children {
            write_node(child, &mut out);
        }
        out
    }
}

fn write_node(node: &Node, out: &mut String) {
    match node {
        Node::Tag(tag) => write_tag(tag, out),
        Node::Text(text) => escape(text, false, out),
        Node::Comment(data) => {
            out.push_str("<!--");
            out.push_str(data);
            out.push_str("-->");
        }
    }
}

fn write_tag(tag: &Tag, out: &mut String) {
    out.push('<');
    out.push_str(&tag.name);
    for (name, value) in &tag.attributes {
        out.push(' ');
        out.push_str(name);
        out.push_str("=\"");
        escape(value, true, out);
        out.push('"');
    }
    out.push('>');
    for child in &tag.children {
        write_node(child, out);
    }
    out.push_str("</");
    out.push_str(&tag.name);
    out.push('>');
}

/// "Escaping a string": `&`, U+00A0, and `"` in attribute mode or `<` and
/// `>` otherwise.
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
