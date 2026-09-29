//! The full styles panel: `SelectedShapeActions`
//! (`components/Actions.tsx:63-217`) inside LayerUI's section and island
//! (`components/LayerUI.tsx:249-297`), as a tree of [`PanelNode`]s and
//! mounted with `web-sys`.
//!
//! Which controls show is `getShapeActionPredicates`
//! ([`get_shape_action_predicates`]); each control is its action's
//! `PanelComponent`, which the caller renders where the tree holds
//! [`PanelNode::Action`] (upstream's `renderAction`). Whether the panel
//! shows at all is [`excali_editor::actions::show_selected_shape_actions`].
//! See `site/content/research/ui-design-system.md` sections 3.2 and 8.

use excali_core::json::number_to_string;
use excali_editor::actions::{get_shape_action_predicates, ActionContext, ActionName};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Document, HtmlElement, Node};

/// `CLASSES.SHAPE_ACTIONS_MENU` (`common/src/constants.ts:113`).
pub const SHAPE_ACTIONS_MENU: &str = "App-menu__left";

/// What LayerUI subtracts from the app height for the island's max height:
/// the approximate height of the hamburger menu and the footer
/// (`LayerUI.tsx:282-284`).
pub const SHAPE_ACTIONS_HEIGHT_OFFSET: f64 = 166.0;

/// A node of the panel's tree.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelNode {
    Element(PanelElement),
    /// Text: a locale key, in English through [`legend_text`].
    Text(&'static str),
    /// `renderAction(name)`: the action's panel component.
    Action(ActionName),
    /// Where the panel goes inside [`shape_actions_section`].
    Panel,
}

/// A DOM element: tag, class, attributes, inline style (CSS property
/// names) and children.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PanelElement {
    pub tag: &'static str,
    pub class: Option<String>,
    pub attrs: Vec<(String, String)>,
    pub style: Vec<(String, String)>,
    pub children: Vec<PanelNode>,
}

fn el(tag: &'static str, class: Option<&str>, children: Vec<PanelNode>) -> PanelNode {
    PanelNode::Element(PanelElement {
        tag,
        class: class.map(str::to_string),
        children,
        ..PanelElement::default()
    })
}

/// The English strings of the panel's text (`locales/en.json`); the key
/// itself for any other.
pub fn legend_text(key: &str) -> &str {
    match key {
        "labels.layers" => "Layers",
        "labels.align" => "Align",
        "labels.actions" => "Actions",
        "headings.selectedShapeActions" => "Selected shape actions",
        other => other,
    }
}

/// `<fieldset><legend>{t(legend)}</legend>{children}</fieldset>`.
fn fieldset(legend: &'static str, children: Vec<PanelNode>) -> PanelNode {
    let mut all = vec![el("legend", None, vec![PanelNode::Text(legend)])];
    all.extend(children);
    el("fieldset", None, all)
}

/// `SelectedShapeActions` (`Actions.tsx:129-217`) for the context's
/// active tool and targets; `rtl` is the document's direction
/// (`document.documentElement.getAttribute("dir") === "rtl"`), which
/// mirrors the horizontal align row (:93-107).
pub fn selected_shape_actions(ctx: &ActionContext<'_>, rtl: bool) -> PanelNode {
    use ActionName as N;
    let p = get_shape_action_predicates(ctx);
    let a = PanelNode::Action;
    let root = |children| el("div", Some("selected-shape-actions"), children);
    let tool = ctx
        .app_state
        .get("activeTool")
        .and_then(|t| t.get("type"))
        .and_then(|t| t.as_str());
    // the bucket fill tool configures only the fill it creates (:150-158)
    if tool == Some("bucketfill") {
        return root(vec![
            el("div", None, vec![a(N::ChangeBucketFillBackgroundColor)]),
            a(N::ChangeFillStyle),
            a(N::ChangeOpacity),
        ]);
    }
    let mut out = Vec::new();
    let stroke = if p.stroke_color {
        vec![a(N::ChangeStrokeColor)]
    } else {
        vec![]
    };
    out.push(el("div", None, stroke));
    if p.background_color {
        out.push(el("div", None, vec![a(N::ChangeBackgroundColor)]));
    }
    let gated = [
        (p.fill, N::ChangeFillStyle),
        (p.stroke_width, N::ChangeStrokeWidth),
        (p.stroke_style, N::ChangeStrokeStyle),
        (p.freedraw_mode, N::ChangeFreedrawMode),
        (p.sloppiness, N::ChangeSloppiness),
        (p.roundness, N::ChangeRoundness),
        (p.arrow_type, N::ChangeArrowType),
    ];
    out.extend(gated.iter().filter(|(on, _)| *on).map(|(_, n)| a(*n)));
    if p.text {
        out.push(el("fieldset", None, vec![a(N::ChangeFontFamily)]));
        out.push(a(N::ChangeFontSize));
        if p.text_align {
            out.push(a(N::ChangeTextAlign));
        }
    }
    let gated = [
        (p.vertical_align, N::ChangeVerticalAlign),
        (p.arrowheads, N::ChangeArrowhead),
        (p.opacity, N::ChangeOpacity),
    ];
    out.extend(gated.iter().filter(|(on, _)| *on).map(|(_, n)| a(*n)));
    if p.layers {
        // LayersFieldset (:63-79)
        out.push(fieldset(
            "labels.layers",
            vec![el(
                "div",
                Some("buttonList"),
                vec![
                    a(N::SendToBack),
                    a(N::SendBackward),
                    a(N::BringForward),
                    a(N::BringToFront),
                ],
            )],
        ));
    }
    if p.align {
        // AlignFieldset (:81-123)
        let (first, last) = if rtl {
            (N::AlignRight, N::AlignLeft)
        } else {
            (N::AlignLeft, N::AlignRight)
        };
        let mut horizontal = vec![a(first), a(N::AlignHorizontallyCentered), a(last)];
        let mut vertical = vec![
            a(N::AlignTop),
            a(N::AlignVerticallyCentered),
            a(N::AlignBottom),
        ];
        if p.distribute {
            horizontal.push(a(N::DistributeHorizontally));
            vertical.push(a(N::DistributeVertically));
        }
        out.push(fieldset(
            "labels.align",
            vec![el(
                "div",
                Some("buttonList align-buttons"),
                vec![
                    el("div", Some("align-buttons__row"), horizontal),
                    el("div", Some("align-buttons__row"), vertical),
                ],
            )],
        ));
    }
    if p.show_extra_actions {
        let mut buttons = vec![
            a(N::DuplicateSelection),
            a(N::DeleteSelectedElements),
            a(N::Group),
            a(N::Ungroup),
        ];
        let gated = [
            (p.link, N::Hyperlink),
            (p.crop_editor, N::CropEditor),
            (p.line_editor, N::ToggleLinearEditor),
        ];
        buttons.extend(gated.iter().filter(|(on, _)| *on).map(|(_, n)| a(*n)));
        out.push(fieldset(
            "labels.actions",
            vec![el("div", Some("buttonList"), buttons)],
        ));
    }
    root(out)
}

/// LayerUI's `renderSelectedShapeActions` in full mode
/// (`LayerUI.tsx:249-297`): the `selectedShapeActions` Section
/// (`components/Section.tsx`) of the container `container_id`, and the
/// Island (`components/Island.tsx`, padding 2) whose max height is the app
/// height less [`SHAPE_ACTIONS_HEIGHT_OFFSET`], holding `panel`.
pub fn shape_actions_section(
    app_height: f64,
    zen_mode_enabled: bool,
    container_id: &str,
    panel: PanelNode,
) -> PanelNode {
    let title_id = format!("{container_id}-selectedShapeActions-title");
    let class = if zen_mode_enabled {
        "selected-shape-actions zen-mode-transition transition-left"
    } else {
        "selected-shape-actions zen-mode-transition"
    };
    let heading = PanelNode::Element(PanelElement {
        tag: "h2",
        class: Some("visually-hidden".into()),
        attrs: vec![("id".into(), title_id.clone())],
        style: vec![],
        children: vec![PanelNode::Text("headings.selectedShapeActions")],
    });
    let max_height = format!(
        "{}px",
        number_to_string(app_height - SHAPE_ACTIONS_HEIGHT_OFFSET)
    );
    let island = PanelNode::Element(PanelElement {
        tag: "div",
        class: Some(format!("Island {SHAPE_ACTIONS_MENU}")),
        attrs: vec![
            ("data-viewport-ui".into(), "side".into()),
            ("data-viewport-ui-name".into(), "stylesPanel".into()),
        ],
        style: vec![
            ("--padding".into(), "2".into()),
            ("max-height".into(), max_height),
        ],
        children: vec![panel],
    });
    PanelNode::Element(PanelElement {
        tag: "section",
        class: Some(class.into()),
        attrs: vec![("aria-labelledby".into(), title_id)],
        style: vec![],
        children: vec![heading, island],
    })
}

/// Builds `node` in `document`: each [`PanelNode::Action`] is what
/// `render_action` returns for it (nothing for `None`, as `renderAction`
/// renders nothing for an action without a panel component), and
/// [`PanelNode::Panel`] is `panel`, when given.
pub fn mount(
    document: &Document,
    node: &PanelNode,
    render_action: &mut dyn FnMut(ActionName) -> Result<Option<Node>, JsValue>,
    panel: Option<&Node>,
) -> Result<Option<Node>, JsValue> {
    match node {
        PanelNode::Element(e) => {
            let element: HtmlElement = document.create_element(e.tag)?.dyn_into()?;
            if let Some(class) = &e.class {
                element.set_class_name(class);
            }
            for (name, value) in &e.attrs {
                element.set_attribute(name, value)?;
            }
            for (name, value) in &e.style {
                element.style().set_property(name, value)?;
            }
            for child in &e.children {
                if let Some(child) = mount(document, child, render_action, panel)? {
                    element.append_child(&child)?;
                }
            }
            Ok(Some(element.into()))
        }
        PanelNode::Text(key) => Ok(Some(document.create_text_node(legend_text(key)).into())),
        PanelNode::Action(name) => render_action(*name),
        PanelNode::Panel => Ok(panel.cloned()),
    }
}
