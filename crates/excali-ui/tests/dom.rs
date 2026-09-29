//! The DOM builder (ex-516): attributes and style replace on a second
//! set, `clsx`'s joining, and the HTML a tree serializes to.

use excali_ui::dom::{class_names, Element, Node};

#[test]
fn class_names_is_clsx_over_pairs() {
    assert_eq!(
        class_names([("a", true), ("", true), ("b", false), ("c", true)]),
        "a c"
    );
    assert_eq!(class_names([("a", false)]), "");
    assert_eq!(
        class_names([("Stack Stack_vertical", true), ("x", true)]),
        "Stack Stack_vertical x"
    );
}

#[test]
fn attributes_and_style_replace_in_place() {
    let el = Element::new("div")
        .attr("class", "a")
        .attr("title", "t")
        .attr("class", "b")
        .attr_opt("hidden", None::<String>)
        .flag("disabled", false)
        .flag("readonly", true)
        .style("--gap", "1")
        .style("left", "2px")
        .style("--gap", "3")
        .style_opt("top", None::<String>);
    assert_eq!(
        el.attributes(),
        &[
            ("class".to_owned(), "b".to_owned()),
            ("title".to_owned(), "t".to_owned()),
            ("readonly".to_owned(), String::new()),
        ]
    );
    assert_eq!(el.attribute("class"), Some("b"));
    assert_eq!(el.attribute("hidden"), None);
    assert_eq!(
        el.style_properties(),
        &[
            ("--gap".to_owned(), "3".to_owned()),
            ("left".to_owned(), "2px".to_owned()),
        ]
    );
}

#[test]
fn html_serialization() {
    let el = Element::new("div")
        .attr("title", "a \"quoted\" & <b>")
        .style("--padding", "2")
        .style("left", "1px")
        .child("x < y & z")
        .child(Element::new("input").attr("value", "v"))
        .child(
            Element::svg("svg")
                .attr("viewBox", "0 0 1 1")
                .child(Element::svg("path")),
        )
        .on("click", |_| {});
    assert_eq!(
        Node::from(el.clone()).to_html(),
        "<div title=\"a &quot;quoted&quot; &amp; <b>\" style=\"--padding: 2; left: 1px;\">\
         x &lt; y &amp; z<input value=\"v\"><svg viewBox=\"0 0 1 1\"><path></path></svg></div>"
    );
    assert_eq!(el.listened_events().collect::<Vec<_>>(), ["click"]);
    assert!(el.children()[2].as_element().unwrap().is_svg());
}
