//! The styles panel: the full `SelectedShapeActions`
//! (`components/Actions.tsx:63-217`), the compact
//! `CompactShapeActions` (:219-717), whose popovers open from
//! `appState.openPopup`, and the phone's `MobileShapeActions` row
//! (:719-876), inside LayerUI's section and island
//! (`components/LayerUI.tsx:249-297`), as a tree of [`PanelNode`]s and
//! mounted with `web-sys`. Which one LayerUI renders is the styles panel
//! mode ([`crate::editor_interface::derive_styles_panel_mode`]: compact on
//! a tablet or in the desktop's compact UI mode).
//!
//! Which controls show is `getShapeActionPredicates`
//! ([`get_shape_action_predicates`]); each control is its action's
//! `PanelComponent`, which the caller renders where the tree holds
//! [`PanelNode::Action`] (upstream's `renderAction`). Whether the panel
//! shows at all is [`excali_editor::actions::show_selected_shape_actions`].
//! See `site/content/research/ui-design-system.md` sections 1.4, 3.2 and
//! 8.

use std::rc::Rc;

use excali_core::color::{ColorTuple, PaletteEntry};
use excali_core::element::{Element, ElementKind};
use excali_core::json::number_to_string;
use excali_editor::actions::{
    form_color, get_shape_action_predicates, get_target_elements, resolve_color_target,
    ActionContext, ActionName, ColorProperty, ColorTargetKind, ShapeActionPredicates,
};
use excali_scene::shape::Theme;
use serde_json::Value;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Document, Event, HtmlElement, KeyboardEvent, Node};

use crate::color_picker::{ColorPickerType, ColorTopPicksSlot, StylesPanelMode};
use crate::icons::{self, Icon};

/// The panel's rules of upstream's `css/styles.scss`
/// (`.selected-shape-actions`, `.App-menu__left`), then the action
/// panels' (`.buttonList`'s buttons and `IconPicker.scss`,
/// [`crate::action_panels`]).
pub const STYLES_PANEL_CSS: &str = concat!(
    include_str!("styles_panel.css"),
    include_str!("action_panels.css")
);

/// Adds [`STYLES_PANEL_CSS`] to the document's head once, after the
/// primitives' stylesheet.
pub fn install_stylesheet(document: &Document) -> Result<(), JsValue> {
    const ID: &str = "styles-panel";
    if document
        .query_selector(&format!("style[data-excali-ui=\"{ID}\"]"))?
        .is_some()
    {
        return Ok(());
    }
    let style = document.create_element("style")?;
    style.set_attribute("data-excali-ui", ID)?;
    style.set_text_content(Some(STYLES_PANEL_CSS));
    let head = document
        .head()
        .ok_or_else(|| JsValue::from_str("the document has no head"))?;
    let before = match document.query_selector("style[data-excali-ui=\"primitives\"]")? {
        Some(p) => p.next_sibling(),
        None => head.first_child(),
    };
    head.insert_before(&style, before.as_ref())?;
    Ok(())
}

/// `CLASSES.SHAPE_ACTIONS_MENU` (`common/src/constants.ts:113`).
pub const SHAPE_ACTIONS_MENU: &str = "App-menu__left";

/// What LayerUI subtracts from the app height for the island's max height:
/// the approximate height of the hamburger menu and the footer
/// (`LayerUI.tsx:282-284`).
pub const SHAPE_ACTIONS_HEIGHT_OFFSET: f64 = 166.0;

/// `CLASSES.SHAPE_ACTIONS_THEME_SCOPE` (`common/src/constants.ts`).
pub const SHAPE_ACTIONS_THEME_SCOPE: &str = "shape-actions-theme-scope";

/// A node of the panel's tree.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelNode {
    Element(PanelElement),
    /// Text: a locale key, in English through [`legend_text`].
    Text(&'static str),
    /// `renderAction(name)`: the action's panel component.
    Action(ActionName),
    /// `renderAction(name, { cycle: true })`: the compact panel's freedraw
    /// pressure button, cycling the mode (`Actions.tsx:662-666`).
    CycleAction(ActionName),
    /// An `icons.tsx` icon.
    Icon(&'static Icon),
    /// An open `PropertiesPopover` (`components/PropertiesPopover.tsx`).
    Popover(PanelPopover),
    /// Where the panel goes inside [`shape_actions_section`].
    Panel,
}

/// A DOM element: tag, class, attributes, inline style (CSS property
/// names) and children; a compact panel trigger toggles `popup_trigger`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PanelElement {
    pub tag: &'static str,
    pub class: Option<String>,
    pub attrs: Vec<(String, String)>,
    pub style: Vec<(String, String)>,
    pub children: Vec<PanelNode>,
    pub popup_trigger: Option<PopupTrigger>,
}

/// The compact panel's popovers: their `appState.openPopup` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactPopup {
    /// "compactStrokeStyles": fill, stroke width and style, sloppiness,
    /// edges, opacity (`Actions.tsx:219-305`).
    StrokeStyles,
    /// "compactArrowProperties": the arrow type (:307-401).
    ArrowProperties,
    /// "compactTextProperties": font size, text and vertical align
    /// (:403-487).
    TextProperties,
    /// "compactOtherProperties": layers, align, group, link, crop
    /// (:489-581).
    OtherProperties,
}

impl CompactPopup {
    pub fn as_str(self) -> &'static str {
        match self {
            CompactPopup::StrokeStyles => "compactStrokeStyles",
            CompactPopup::ArrowProperties => "compactArrowProperties",
            CompactPopup::TextProperties => "compactTextProperties",
            CompactPopup::OtherProperties => "compactOtherProperties",
        }
    }

    /// The popover `open_popup` (`appState.openPopup`) names, if any.
    pub fn from_popup(open_popup: &str) -> Option<CompactPopup> {
        [
            CompactPopup::StrokeStyles,
            CompactPopup::ArrowProperties,
            CompactPopup::TextProperties,
            CompactPopup::OtherProperties,
        ]
        .into_iter()
        .find(|p| p.as_str() == open_popup)
    }
}

/// A compact panel trigger: its popover, and whether it is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupTrigger {
    pub popup: CompactPopup,
    pub open: bool,
}

impl PopupTrigger {
    /// The `openPopup` a click sets: `isOpen ? null : popup`
    /// (`Actions.tsx:263-270`).
    pub fn next(self) -> Option<CompactPopup> {
        (!self.open).then_some(self.popup)
    }
}

/// An open `PropertiesPopover`: radix's `Popover.Content` placed beside
/// its trigger, holding an island (`PropertiesPopover.tsx:45-104`).
#[derive(Debug, Clone, PartialEq)]
pub struct PanelPopover {
    /// The popover's `className`.
    pub class: &'static str,
    /// Radix's `side` and `align`: right and start except on a phone in
    /// portrait, which never shows the compact panel.
    pub side: &'static str,
    pub align: &'static str,
    pub side_offset: f64,
    pub align_offset: f64,
    /// The island.
    pub children: Vec<PanelNode>,
}

impl PanelPopover {
    /// The content's class: `clsx("focus-visible-none", className)`.
    pub fn class_name(&self) -> String {
        format!("focus-visible-none {}", self.class)
    }
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
        "labels.stroke" => "Stroke",
        "labels.arrowtypes" => "Arrow type",
        "labels.textAlign" => "Text align",
        "labels.textColor" => "Text color",
        "labels.background" => "Background",
        other => other,
    }
}

/// `<fieldset><legend>{t(legend)}</legend>{children}</fieldset>`.
fn fieldset(legend: &'static str, children: Vec<PanelNode>) -> PanelNode {
    let mut all = vec![el("legend", None, vec![PanelNode::Text(legend)])];
    all.extend(children);
    el("fieldset", None, all)
}

/// `LayersFieldset` (`Actions.tsx:63-79`).
fn layers_fieldset() -> PanelNode {
    use ActionName as N;
    let a = PanelNode::Action;
    fieldset(
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
    )
}

/// `AlignFieldset` (`Actions.tsx:81-123`): the horizontal row mirrored in
/// right-to-left documents.
fn align_fieldset(rtl: bool, distribute: bool) -> PanelNode {
    use ActionName as N;
    let a = PanelNode::Action;
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
    if distribute {
        horizontal.push(a(N::DistributeHorizontally));
        vertical.push(a(N::DistributeVertically));
    }
    fieldset(
        "labels.align",
        vec![el(
            "div",
            Some("buttonList align-buttons"),
            vec![
                el("div", Some("align-buttons__row"), horizontal),
                el("div", Some("align-buttons__row"), vertical),
            ],
        )],
    )
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
        out.push(layers_fieldset());
    }
    if p.align {
        out.push(align_fieldset(rtl, p.distribute));
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

/// A compact panel item: `<div className="compact-action-item">`.
fn item(children: Vec<PanelNode>) -> PanelNode {
    el("div", Some("compact-action-item"), children)
}

/// A popover's island: `Island` with padding 3 and `style`
/// (`PropertiesPopover.tsx:92-94`).
fn popover_island(style: &[(&str, &str)], children: Vec<PanelNode>) -> PanelNode {
    let mut all = vec![("--padding".to_string(), "3".to_string())];
    all.extend(style.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    PanelNode::Element(PanelElement {
        tag: "div",
        class: Some("Island".into()),
        style: all,
        children,
        ..PanelElement::default()
    })
}

/// A compact panel popover: its item holding the trigger (the popup's
/// title and icon; `active` while open) and, while open, the
/// `PropertiesPopover` (`Actions.tsx:246-302`, and alike for the others).
fn popup_item(
    popup: CompactPopup,
    open_popup: Option<&str>,
    title: &'static str,
    icon: &'static Icon,
    class: &'static str,
    style: &[(&str, &str)],
    body: impl FnOnce() -> Vec<PanelNode>,
) -> PanelNode {
    let open = open_popup == Some(popup.as_str());
    let trigger = PanelNode::Element(PanelElement {
        tag: "button",
        class: Some(if open {
            "compact-action-button properties-trigger active".into()
        } else {
            "compact-action-button properties-trigger".into()
        }),
        attrs: vec![
            ("type".into(), "button".into()),
            ("title".into(), legend_text(title).into()),
        ],
        children: vec![PanelNode::Icon(icon)],
        popup_trigger: Some(PopupTrigger { popup, open }),
        ..PanelElement::default()
    });
    let mut children = vec![trigger];
    if open {
        children.push(PanelNode::Popover(PanelPopover {
            class,
            side: "right",
            align: "start",
            side_offset: 20.0,
            align_offset: -16.0,
            children: vec![popover_island(style, body())],
        }));
    }
    item(children)
}

/// `PROPERTIES_CLASSES` (`Actions.tsx:57-60`).
const PROPERTIES_CLASSES: &str = "shape-actions-theme-scope properties-content";

fn truthy_id(ctx: &ActionContext<'_>, id: &str) -> bool {
    match ctx
        .app_state
        .get("selectedElementIds")
        .and_then(|ids| ids.get(id))
    {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// The arrow type trigger's icon (`Actions.tsx:356-384`): `getFormValue`
/// over the target elements (`actionProperties.tsx:229-270`): the selected
/// arrows' common type when some target is selected, else
/// `currentItemArrowType`; sharp when neither gives one. The text being
/// edited is never an arrow, so it never answers.
fn arrow_type_icon(ctx: &ActionContext<'_>, targets: &[Element]) -> &'static Icon {
    let arrow_type = |e: &Element| match &e.kind {
        ElementKind::Arrow(a) if a.elbowed => Some("elbow"),
        ElementKind::Arrow(_) if e.base.roundness.is_some() => Some("round"),
        ElementKind::Arrow(_) => Some("sharp"),
        _ => None,
    };
    let has_selection = targets
        .iter()
        .any(|e| !e.base.is_deleted && truthy_id(ctx, &e.base.id));
    let value = if has_selection {
        // reduceToCommonValue over app.scene.getSelectedElements(appState)
        let mut common = None;
        let mut any = false;
        for e in ctx
            .elements
            .iter()
            .filter(|e| !e.base.is_deleted && truthy_id(ctx, &e.base.id))
            .filter(|e| matches!(e.kind, ElementKind::Arrow(_)))
        {
            let v = arrow_type(e);
            if !any || common == v {
                common = v;
                any = true;
            } else {
                common = None;
                break;
            }
        }
        common
    } else {
        ctx.app_state
            .get("currentItemArrowType")
            .and_then(Value::as_str)
    };
    match value {
        Some("elbow") => &icons::elbowArrowIcon,
        Some("round") => &icons::roundArrowIcon,
        _ => &icons::sharpArrowIcon,
    }
}

/// The popovers' state: the active tool type and `appState.openPopup`.
struct Popups<'a> {
    tool: &'a str,
    open: Option<&'a str>,
}

impl<'a> Popups<'a> {
    fn of(ctx: &'a ActionContext<'_>) -> Popups<'a> {
        Popups {
            tool: ctx
                .app_state
                .get("activeTool")
                .and_then(|t| t.get("type"))
                .and_then(Value::as_str)
                .unwrap_or(""),
            open: ctx.app_state.get("openPopup").and_then(Value::as_str),
        }
    }
}

/// The stroke and background colour items (`Actions.tsx:623-644`, and
/// alike in the mobile row).
fn colour_items(p: &ShapeActionPredicates, popups: &Popups<'_>, out: &mut Vec<PanelNode>) {
    let a = PanelNode::Action;
    if p.stroke_color {
        out.push(item(vec![a(ActionName::ChangeStrokeColor)]));
    }
    if p.background_color {
        // the bucket fill variant excludes `transparent`
        out.push(item(vec![a(if popups.tool == "bucketfill" {
            ActionName::ChangeBucketFillBackgroundColor
        } else {
            ActionName::ChangeBackgroundColor
        })]));
    }
}

/// `CombinedShapeProperties` (`Actions.tsx:219-305`).
fn shape_properties(p: &ShapeActionPredicates, popups: &Popups<'_>) -> Option<PanelNode> {
    use ActionName as N;
    let a = PanelNode::Action;
    let passive = matches!(
        popups.tool,
        "selection" | "eraser" | "hand" | "laser" | "lasso"
    );
    if !(p.has_selection || !passive) {
        return None;
    }
    Some(popup_item(
        CompactPopup::StrokeStyles,
        popups.open,
        "labels.stroke",
        &icons::adjustmentsIcon,
        PROPERTIES_CLASSES,
        &[("max-width", "13rem")],
        || {
            let gated = [
                (p.fill, N::ChangeFillStyle),
                (p.stroke_width, N::ChangeStrokeWidth),
                (p.freedraw_mode, N::ChangeFreedrawMode),
                (p.stroke_style, N::ChangeStrokeStyle),
                (p.sloppiness, N::ChangeSloppiness),
                (p.roundness, N::ChangeRoundness),
                (p.opacity, N::ChangeOpacity),
            ];
            let actions = gated.iter().filter(|(on, _)| *on).map(|(_, n)| a(*n));
            vec![el("div", Some("selected-shape-actions"), actions.collect())]
        },
    ))
}

/// `CombinedArrowProperties` (`Actions.tsx:307-401`).
fn arrow_properties(
    ctx: &ActionContext<'_>,
    p: &ShapeActionPredicates,
    popups: &Popups<'_>,
) -> Option<PanelNode> {
    if !p.arrow_type {
        return None;
    }
    let targets = get_target_elements(ctx);
    Some(popup_item(
        CompactPopup::ArrowProperties,
        popups.open,
        "labels.arrowtypes",
        arrow_type_icon(ctx, &targets),
        "properties-content",
        &[("max-width", "13rem")],
        || vec![PanelNode::Action(ActionName::ChangeArrowProperties)],
    ))
}

/// `LinearEditorAction` (`Actions.tsx:583-603`).
fn linear_editor(p: &ShapeActionPredicates) -> Option<PanelNode> {
    p.line_editor
        .then(|| item(vec![PanelNode::Action(ActionName::ToggleLinearEditor)]))
}

/// The font family item and `CombinedTextProperties`
/// (`Actions.tsx:403-487`).
fn text_items(p: &ShapeActionPredicates, popups: &Popups<'_>, out: &mut Vec<PanelNode>) {
    use ActionName as N;
    let a = PanelNode::Action;
    if !p.text {
        return;
    }
    out.push(item(vec![a(N::ChangeFontFamily)]));
    out.push(popup_item(
        CompactPopup::TextProperties,
        popups.open,
        "labels.textAlign",
        &icons::TextSizeIcon,
        PROPERTIES_CLASSES,
        &[("max-width", "13rem")],
        || {
            let gated = [
                (p.text, N::ChangeFontSize),
                (p.text_align, N::ChangeTextAlign),
                (p.vertical_align, N::ChangeVerticalAlign),
            ];
            let actions = gated.iter().filter(|(on, _)| *on).map(|(_, n)| a(*n));
            vec![el("div", Some("selected-shape-actions"), actions.collect())]
        },
    ));
}

/// `CombinedExtraActions` (`Actions.tsx:489-581`): its "…" popover, with
/// duplicate and delete when asked.
fn extra_actions(
    p: &ShapeActionPredicates,
    popups: &Popups<'_>,
    rtl: bool,
    show_duplicate: bool,
    show_delete: bool,
) -> Option<PanelNode> {
    use ActionName as N;
    let a = PanelNode::Action;
    if !p.show_extra_actions {
        return None;
    }
    Some(popup_item(
        CompactPopup::OtherProperties,
        popups.open,
        "labels.actions",
        &icons::DotsHorizontalIcon,
        PROPERTIES_CLASSES,
        &[
            ("max-width", "12rem"),
            ("justify-content", "center"),
            ("align-items", "center"),
        ],
        || {
            let mut body = Vec::new();
            if p.layers {
                body.push(layers_fieldset());
            }
            if p.align {
                body.push(align_fieldset(rtl, p.distribute));
            }
            let mut buttons = vec![a(N::Group), a(N::Ungroup)];
            let gated = [
                (p.link_single_only, N::Hyperlink),
                (p.crop_editor, N::CropEditor),
                (show_duplicate, N::DuplicateSelection),
                (show_delete, N::DeleteSelectedElements),
            ];
            buttons.extend(gated.iter().filter(|(on, _)| *on).map(|(_, n)| a(*n)));
            body.push(fieldset(
                "labels.actions",
                vec![el("div", Some("buttonList"), buttons)],
            ));
            vec![el("div", Some("selected-shape-actions"), body)]
        },
    ))
}

/// `CompactShapeActions` (`Actions.tsx:605-717`): the compact styles
/// panel of tablets and the desktop's compact UI mode, for the context's
/// active tool and targets. The colours and the freedraw pressure cycle
/// button show inline; the stroke styles, arrow type, text properties and
/// other actions are popovers, open while `appState.openPopup` names them.
/// `rtl` mirrors the align row as in [`selected_shape_actions`].
pub fn compact_shape_actions(ctx: &ActionContext<'_>, rtl: bool) -> PanelNode {
    use ActionName as N;
    let p: ShapeActionPredicates = get_shape_action_predicates(ctx);
    let popups = Popups::of(ctx);
    let mut out = Vec::new();
    colour_items(&p, &popups, &mut out);
    if p.freedraw_mode {
        out.push(item(vec![PanelNode::CycleAction(N::ChangeFreedrawMode)]));
    }
    out.extend(shape_properties(&p, &popups));
    out.extend(arrow_properties(ctx, &p, &popups));
    out.extend(linear_editor(&p));
    text_items(&p, &popups, &mut out);
    if p.show_extra_actions {
        out.push(item(vec![PanelNode::Action(N::DuplicateSelection)]));
        out.push(item(vec![PanelNode::Action(N::DeleteSelectedElements)]));
    }
    // the panel passes neither showDuplicate nor showDelete
    out.extend(extra_actions(&p, &popups, rtl, false, false));
    el("div", Some("compact-shape-actions"), out)
}

/// The mobile row's button size (`WIDTH`, `Actions.tsx:754`).
pub const MOBILE_ACTION_WIDTH: f64 = 32.0;

/// The gap between the mobile row's buttons (`GAP`, `Actions.tsx:753`).
pub const MOBILE_ACTION_GAP: f64 = 6.0;

/// The width of the mobile row's 9 fixed buttons (7 actions, undo and
/// redo) and their gaps (`MIN_WIDTH`, `Actions.tsx:750-756`); delete moves
/// out of the "…" popover at one more button, duplicate at two.
pub const MOBILE_ACTIONS_MIN_WIDTH: f64 = 9.0 * MOBILE_ACTION_WIDTH + 8.0 * MOBILE_ACTION_GAP;

/// `MobileShapeActions` (`Actions.tsx:719-876`): the phone's styles row
/// above the bottom toolbar, for the context's active tool and targets,
/// at the row's measured `width` (upstream reads its island's width on
/// render, 0 on the first). The compact panel's colours, popovers, line
/// editor and text items on the left, undo and redo on the right, with
/// duplicate and delete beside them once the row is wide enough and in
/// the "…" popover otherwise.
pub fn mobile_shape_actions(ctx: &ActionContext<'_>, rtl: bool, width: f64) -> PanelNode {
    use ActionName as N;
    let p: ShapeActionPredicates = get_shape_action_predicates(ctx);
    let popups = Popups::of(ctx);
    let extra = MOBILE_ACTION_WIDTH + MOBILE_ACTION_GAP;
    let delete_outside = width >= MOBILE_ACTIONS_MIN_WIDTH + extra;
    let duplicate_outside = width >= MOBILE_ACTIONS_MIN_WIDTH + 2.0 * extra;
    let px = |n: f64| format!("{}px", number_to_string(n));
    let mut left = Vec::new();
    colour_items(&p, &popups, &mut left);
    left.extend(shape_properties(&p, &popups));
    left.extend(arrow_properties(ctx, &p, &popups));
    left.extend(linear_editor(&p));
    text_items(&p, &popups, &mut left);
    left.extend(extra_actions(
        &p,
        &popups,
        rtl,
        !duplicate_outside,
        !delete_outside,
    ));
    let mut right = vec![
        item(vec![PanelNode::Action(N::Undo)]),
        item(vec![PanelNode::Action(N::Redo)]),
    ];
    if duplicate_outside {
        right.push(item(vec![PanelNode::Action(N::DuplicateSelection)]));
    }
    if delete_outside {
        right.push(item(vec![PanelNode::Action(N::DeleteSelectedElements)]));
    }
    let row = |style: Vec<(&str, String)>, children| {
        PanelNode::Element(PanelElement {
            tag: "div",
            style: style.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            children,
            ..PanelElement::default()
        })
    };
    let gap = px(MOBILE_ACTION_GAP);
    PanelNode::Element(PanelElement {
        tag: "div",
        class: Some("Island compact-shape-actions mobile-shape-actions".into()),
        style: [
            ("flex-direction", "row".to_string()),
            ("box-shadow", "none".into()),
            ("padding", "0".into()),
            ("z-index", "2".into()),
            ("background-color", "transparent".into()),
            ("height", px(MOBILE_ACTION_WIDTH * 1.35)),
            ("margin-bottom", "4px".into()),
            ("align-items", "center".into()),
            ("gap", gap.clone()),
            ("pointer-events", "none".into()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect(),
        children: vec![
            row(
                vec![
                    ("display", "flex".into()),
                    ("flex-direction", "row".into()),
                    ("gap", gap.clone()),
                    ("flex", "1".into()),
                ],
                left,
            ),
            row(
                vec![
                    ("display", "flex".into()),
                    ("flex-direction", "row".into()),
                    ("gap", gap),
                ],
                right,
            ),
        ],
        ..PanelElement::default()
    })
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
    section(
        app_height,
        zen_mode_enabled,
        container_id,
        &format!("Island {SHAPE_ACTIONS_MENU}"),
        "2",
        panel,
    )
}

/// LayerUI's `renderSelectedShapeActions` in compact mode
/// (`LayerUI.tsx:249-275`): as [`shape_actions_section`], with the
/// `compact-shape-actions-island` Island and no padding.
pub fn compact_shape_actions_section(
    app_height: f64,
    zen_mode_enabled: bool,
    container_id: &str,
    panel: PanelNode,
) -> PanelNode {
    section(
        app_height,
        zen_mode_enabled,
        container_id,
        "Island compact-shape-actions-island",
        "0",
        panel,
    )
}

fn section(
    app_height: f64,
    zen_mode_enabled: bool,
    container_id: &str,
    island_class: &str,
    padding: &str,
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
        children: vec![PanelNode::Text("headings.selectedShapeActions")],
        ..PanelElement::default()
    });
    let max_height = format!(
        "{}px",
        number_to_string(app_height - SHAPE_ACTIONS_HEIGHT_OFFSET)
    );
    let island = PanelNode::Element(PanelElement {
        tag: "div",
        class: Some(island_class.into()),
        attrs: vec![
            ("data-viewport-ui".into(), "side".into()),
            ("data-viewport-ui-name".into(), "stylesPanel".into()),
        ],
        style: vec![
            ("--padding".into(), padding.into()),
            ("max-height".into(), max_height),
        ],
        children: vec![panel],
        ..PanelElement::default()
    });
    PanelNode::Element(PanelElement {
        tag: "section",
        class: Some(class.into()),
        attrs: vec![("aria-labelledby".into(), title_id)],
        children: vec![heading, island],
        ..PanelElement::default()
    })
}

/// Sets `appState.openPopup` to a compact panel popover, or clears it.
pub type OnPopup = Rc<dyn Fn(Option<CompactPopup>)>;

/// What [`mount`] builds the panel with besides the tree.
#[derive(Default)]
pub struct MountOptions<'a> {
    /// What [`PanelNode::Panel`] is.
    pub panel: Option<&'a Node>,
    /// Called by a compact panel trigger's click with the `openPopup` it
    /// sets ([`PopupTrigger::next`]), and by Escape in an open popover
    /// with `None` (radix's `onOpenChange(false)`).
    pub on_popup: Option<OnPopup>,
}

/// Builds `node` in `document`: each [`PanelNode::Action`] is what
/// `render_action` returns for it (nothing for `None`, as `renderAction`
/// renders nothing for an action without a panel component; its second
/// argument is `cycle`, true for [`PanelNode::CycleAction`]), and
/// [`PanelNode::Panel`] is `options.panel`, when given. An open popover
/// is radix's popper wrapper holding the content (the island and the
/// arrow) after its trigger; [`place_popovers`] places it once the panel
/// is in the document. Listeners live as long as the page.
pub fn mount(
    document: &Document,
    node: &PanelNode,
    render_action: &mut dyn FnMut(ActionName, bool) -> Result<Option<Node>, JsValue>,
    options: &MountOptions<'_>,
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
            if let Some(trigger) = e.popup_trigger {
                // radix's Popover.Trigger
                element.set_attribute("aria-haspopup", "dialog")?;
                element.set_attribute("aria-expanded", &trigger.open.to_string())?;
                let state = if trigger.open { "open" } else { "closed" };
                element.set_attribute("data-state", state)?;
                if let Some(on_popup) = options.on_popup.clone() {
                    listen(&element, "click", move |e| {
                        e.prevent_default();
                        e.stop_propagation();
                        on_popup(trigger.next());
                    })?;
                }
            }
            for child in &e.children {
                if let Some(child) = mount(document, child, render_action, options)? {
                    element.append_child(&child)?;
                }
            }
            Ok(Some(element.into()))
        }
        PanelNode::Text(key) => Ok(Some(document.create_text_node(legend_text(key)).into())),
        PanelNode::Action(name) => render_action(*name, false),
        PanelNode::CycleAction(name) => render_action(*name, true),
        PanelNode::Icon(icon) => match icon.element(Theme::Light) {
            Some(svg) => Ok(Some(crate::dom::create_detached(&svg.into(), document)?)),
            None => Ok(None),
        },
        PanelNode::Popover(p) => mount_popover(document, p, render_action, options).map(Some),
        PanelNode::Panel => Ok(options.panel.cloned()),
    }
}

fn listen(
    target: &web_sys::Element,
    event: &str,
    handler: impl Fn(&Event) + 'static,
) -> Result<(), JsValue> {
    let closure = Closure::<dyn FnMut(Event)>::new(move |e: Event| handler(&e));
    target.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref())?;
    closure.forget();
    Ok(())
}

/// `PropertiesPopover` (`PropertiesPopover.tsx:45-104`) as radix renders
/// it: the popper wrapper, the dialog content (its class, the z-index of
/// the styles popups) with the island and the arrow.
fn mount_popover(
    document: &Document,
    p: &PanelPopover,
    render_action: &mut dyn FnMut(ActionName, bool) -> Result<Option<Node>, JsValue>,
    options: &MountOptions<'_>,
) -> Result<Node, JsValue> {
    let wrapper = document.create_element("div")?;
    wrapper.set_attribute("data-radix-popper-content-wrapper", "")?;
    let content: HtmlElement = document.create_element("div")?.dyn_into()?;
    content.set_class_name(&p.class_name());
    for (name, value) in [
        ("data-prevent-outside-click", "true"),
        ("data-state", "open"),
        ("data-side", p.side),
        ("data-align", p.align),
        ("role", "dialog"),
        ("tabindex", "-1"),
    ] {
        content.set_attribute(name, value)?;
    }
    content
        .style()
        .set_property("z-index", "var(--zIndex-ui-styles-popup)")?;
    if let Some(on_popup) = options.on_popup.clone() {
        listen(&content, "keydown", move |e| {
            if e.dyn_ref::<KeyboardEvent>()
                .map(KeyboardEvent::key)
                .as_deref()
                == Some("Escape")
            {
                e.stop_propagation();
                on_popup(None);
            }
        })?;
    }
    for child in &p.children {
        if let Some(child) = mount(document, child, render_action, options)? {
            content.append_child(&child)?;
        }
    }
    // Popover.Arrow: width 20, height 10
    let arrow = document.create_element("span")?;
    arrow.set_inner_html(
        "<svg width=\"20\" height=\"10\" viewBox=\"0 0 30 10\" preserveAspectRatio=\"none\" \
         style=\"fill: var(--popup-bg-color); filter: drop-shadow(rgba(0, 0, 0, 0.05) 0px 3px 2px); \
         display: block;\"><polygon points=\"0,0 30,0 15,10\"></polygon></svg>",
    );
    content.append_child(&arrow)?;
    wrapper.append_child(&content)?;
    Ok(wrapper.into())
}

/// Places each open popover under `root` beside its trigger (the button
/// before it), as radix's `Popover.Content` does with floating-ui
/// (`side="right"`, `align="start"`, `alignOffset={-16}`,
/// `sideOffset={20}`), flipping left when it would leave the editor.
/// Call once the mounted panel is in the document.
pub fn place_popovers(root: &web_sys::Element) {
    let Ok(wrappers) = root.query_selector_all("[data-radix-popper-content-wrapper]") else {
        return;
    };
    for i in 0..wrappers.length() {
        let Some(wrapper) = wrappers
            .item(i)
            .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
        else {
            continue;
        };
        // IconPicker's popover places itself
        // (crate::action_panels::place_icon_pickers)
        if wrapper
            .first_element_child()
            .is_some_and(|c| c.class_list().contains("picker"))
        {
            continue;
        }
        let below = wrapper
            .first_element_child()
            .and_then(|c| c.get_attribute("data-side"))
            .as_deref()
            == Some("bottom");
        if let Some(trigger) = wrapper.previous_element_sibling() {
            crate::color_picker::place_beside(&wrapper, &trigger, below);
        }
    }
}

// -- the colour actions' panel components ----------------------------------------

/// What the `PanelComponent` of `actionChangeStrokeColor` or
/// `actionChangeBackgroundColor` renders (`actionProperties.tsx:385-435`,
/// :508-551): in the full panel an `aria-hidden` heading, then the
/// `ColorPicker` with these props, as [`resolve_color_target`] and
/// [`form_color`] decide them.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorActionPanel {
    /// The heading's locale key; `None` outside the full panel.
    pub heading: Option<&'static str>,
    pub ty: ColorPickerType,
    /// The picker's label's locale key.
    pub label: &'static str,
    /// The colour shown; `None` for a selection without a common one.
    pub color: Option<String>,
    pub palette: &'static [PaletteEntry],
    pub top_picks: ColorTuple,
    pub customizable_top_picks: ColorTopPicksSlot,
    pub excluded_colors: Option<&'static [&'static str]>,
}

/// The colour action `name`'s panel component in `mode`; `None` for any
/// other action.
pub fn color_action_panel(
    ctx: &ActionContext<'_>,
    name: ActionName,
    mode: StylesPanelMode,
) -> Option<ColorActionPanel> {
    let (property, ty) = match name {
        ActionName::ChangeStrokeColor => {
            (ColorProperty::StrokeColor, ColorPickerType::ElementStroke)
        }
        ActionName::ChangeBackgroundColor => (
            ColorProperty::BackgroundColor,
            ColorPickerType::ElementBackground,
        ),
        _ => return None,
    };
    let target = resolve_color_target(ctx, property);
    // a note has no stroke: its "stroke" is the ink of its text and footer
    let label = match property {
        ColorProperty::StrokeColor if target.kind == ColorTargetKind::Sticky => "labels.textColor",
        ColorProperty::StrokeColor => "labels.stroke",
        ColorProperty::BackgroundColor => "labels.background",
    };
    let slot = ColorTopPicksSlot::ALL
        .into_iter()
        .find(|s| s.as_str() == target.customizable_top_picks)
        .expect("a colorTopPicks slot");
    Some(ColorActionPanel {
        heading: (mode == StylesPanelMode::Full).then_some(label),
        ty,
        label,
        color: form_color(ctx, &target),
        palette: target.palette,
        top_picks: target.top_picks,
        customizable_top_picks: slot,
        excluded_colors: target.excluded_colors,
    })
}

impl ColorActionPanel {
    /// The heading, `<h3 aria-hidden="true">`, when it shows.
    pub fn heading_node(&self) -> Option<PanelNode> {
        self.heading.map(|key| {
            PanelNode::Element(PanelElement {
                tag: "h3",
                attrs: vec![("aria-hidden".into(), "true".into())],
                children: vec![PanelNode::Text(key)],
                ..PanelElement::default()
            })
        })
    }
}
