//! Icon markup into the DOM builder's tree.

use crate::dom::Element;

/// Parses the markup React's `renderToStaticMarkup` writes for an icon:
/// elements with double-quoted attributes and explicit end tags, no text.
pub(super) fn parse(markup: &str) -> Element {
    let mut stack: Vec<Element> = Vec::new();
    let mut rest = markup;
    loop {
        rest = rest
            .strip_prefix('<')
            .unwrap_or_else(|| panic!("icon markup: expected a tag at {rest:?}"));
        if let Some(after) = rest.strip_prefix('/') {
            let end = after.find('>').expect("icon markup: unterminated end tag");
            rest = &after[end + 1..];
            let done = stack.pop().expect("icon markup: unbalanced end tag");
            match stack.last_mut() {
                Some(parent) => *parent = std::mem::replace(parent, Element::new("")).child(done),
                None => {
                    assert!(rest.is_empty(), "icon markup: trailing {rest:?}");
                    return done;
                }
            }
            continue;
        }
        let name_end = rest
            .find([' ', '>'])
            .expect("icon markup: unterminated tag");
        let tag = &rest[..name_end];
        let mut el = if tag == "div" {
            Element::new(tag)
        } else {
            Element::svg(tag)
        };
        rest = &rest[name_end..];
        while let Some(attr) = rest.strip_prefix(' ') {
            let eq = attr
                .find("=\"")
                .expect("icon markup: attribute without value");
            let value_end = attr[eq + 2..]
                .find('"')
                .expect("icon markup: unterminated attribute");
            el = el.attr(&attr[..eq], unescape(&attr[eq + 2..eq + 2 + value_end]));
            rest = &attr[eq + 3 + value_end..];
        }
        rest = rest
            .strip_prefix('>')
            .expect("icon markup: unterminated start tag");
        stack.push(el);
    }
}

fn unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}
