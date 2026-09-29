//! The icons the primitives use, as `createIcon` builds them
//! (`components/icons.tsx:27-51`): an `svg` with `aria-hidden`,
//! `focusable="false"`, `role="img"`, a `0 0 width height` view box and
//! the icon's presentation attributes.

use crate::dom::{Element, Node};

/// An SVG icon's options (`Opts`, `icons.tsx:22-26`): its view box size,
/// whether it mirrors in right-to-left languages, and the presentation
/// attributes put on the `svg`.
#[derive(Clone, Debug)]
pub struct IconOptions {
    pub width: f64,
    pub height: f64,
    pub mirror: bool,
    pub attributes: Vec<(&'static str, &'static str)>,
}

impl IconOptions {
    /// A square `size` × `size` icon.
    pub fn square(size: f64) -> IconOptions {
        IconOptions {
            width: size,
            height: size,
            mirror: false,
            attributes: Vec::new(),
        }
    }
}

/// `tablerIconProps` (`icons.tsx:53-61`).
pub fn tabler_icon_options() -> IconOptions {
    IconOptions {
        width: 24.0,
        height: 24.0,
        mirror: false,
        attributes: vec![
            ("fill", "none"),
            ("stroke-width", "2"),
            ("stroke", "currentColor"),
            ("stroke-linecap", "round"),
            ("stroke-linejoin", "round"),
        ],
    }
}

/// `modifiedTablerIconProps` (`icons.tsx:63-70`).
pub fn modified_tabler_icon_options() -> IconOptions {
    IconOptions {
        width: 20.0,
        height: 20.0,
        mirror: false,
        attributes: vec![
            ("fill", "none"),
            ("stroke", "currentColor"),
            ("stroke-linecap", "round"),
            ("stroke-linejoin", "round"),
        ],
    }
}

/// `createIcon(children, opts)`.
pub fn create_icon(children: Vec<Node>, options: IconOptions) -> Element {
    let view_box = format!(
        "0 0 {} {}",
        super::number(options.width),
        super::number(options.height)
    );
    let svg = Element::svg("svg")
        .attr("aria-hidden", "true")
        .attr("focusable", "false")
        .attr("role", "img")
        .attr("viewBox", view_box)
        .attr("class", if options.mirror { "rtl-mirror" } else { "" });
    options
        .attributes
        .iter()
        .fold(svg, |svg, (k, v)| svg.attr(*k, *v))
        .children_from(children)
}

fn path(d: &str) -> Element {
    Element::svg("path").attr("d", d)
}

/// `CloseIcon` (`icons.tsx:958-976`).
pub fn close_icon() -> Element {
    create_icon(
        vec![
            Element::svg("g")
                .attr("clip-path", "url(#a)")
                .attr("stroke", "currentColor")
                .attr("stroke-width", "1.25")
                .attr("stroke-linecap", "round")
                .attr("stroke-linejoin", "round")
                .child(path("M15 5 5 15M5 5l10 10"))
                .into(),
            Element::svg("defs")
                .child(
                    Element::svg("clipPath")
                        .attr("id", "a")
                        .child(path("M0 0h20v20H0z").attr("fill", "#fff")),
                )
                .into(),
        ],
        modified_tabler_icon_options(),
    )
}

/// The frame every tabler icon starts with.
fn tabler_frame() -> Element {
    path("M0 0h24v24H0z")
        .attr("stroke", "none")
        .attr("fill", "none")
}

/// `eyeIcon` (`icons.tsx:2112-2119`): shows a redacted value.
pub fn eye_icon() -> Element {
    create_icon(
        vec![Element::svg("g")
            .attr("stroke", "currentColor")
            .attr("fill", "none")
            .attr("stroke-width", "1.5")
            .child(tabler_frame())
            .child(path("M10 12a2 2 0 1 0 4 0a2 2 0 0 0 -4 0"))
            .child(path(
                "M21 12c-2.4 4 -5.4 6 -9 6c-3.6 0 -6.6 -2 -9 -6c2.4 -4 5.4 -6 9 -6c3.6 0 6.6 2 9 6",
            ))
            .into()],
        tabler_icon_options(),
    )
}

/// `eyeClosedIcon` (`icons.tsx:2121-2129`): redacts it again.
pub fn eye_closed_icon() -> Element {
    create_icon(
        vec![Element::svg("g")
            .attr("stroke", "currentColor")
            .attr("fill", "none")
            .child(tabler_frame())
            .child(path("M10.585 10.587a2 2 0 0 0 2.829 2.828"))
            .child(path(
                "M16.681 16.673a8.717 8.717 0 0 1 -4.681 1.327c-3.6 0 -6.6 -2 -9 -6c1.272 -2.12 2.712 -3.678 4.32 -4.674m2.86 -1.146a9.055 9.055 0 0 1 1.82 -.18c3.6 0 6.6 2 9 6c-.666 1.11 -1.379 2.067 -2.138 2.87",
            ))
            .child(path("M3 3l18 18"))
            .into()],
        tabler_icon_options(),
    )
}
