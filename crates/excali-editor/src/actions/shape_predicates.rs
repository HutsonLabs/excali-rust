//! Which styles-panel controls are relevant now:
//! `getShapeActionPredicates` (`components/shapeActionPredicates.ts`), the
//! element and tool type tests it reads (`element/src/comparisons.ts`),
//! `getTargetElements` (`element/src/selection.ts:215-231`) and
//! `showSelectedShapeActions` (`element/src/showSelectedShapeActions.ts`).
//!
//! `elementsMap` is what LayerUI passes, `app.scene.getNonDeletedElementsMap()`
//! (`LayerUI.tsx:290`): the context's non-deleted elements.

use excali_core::color::is_transparent;
use excali_core::element::{Element, ElementKind, ElementType};
use serde_json::Value;

use super::context::{container_id, has_bound_text_element, ActionContext};
use super::menus::PanelGate;
use super::registry::align_actions_predicate;
use crate::js_value::truthy;

/// `hasBackground(type)` (`comparisons.ts:3-14`), for element and tool
/// types.
pub fn has_background(ty: &str) -> bool {
    matches!(
        ty,
        "rectangle"
            | "stickynote"
            | "iframe"
            | "embeddable"
            | "ellipse"
            | "diamond"
            | "line"
            | "freedraw"
            | "autoshape"
            | "bucketfill"
    )
}

/// `hasFillStyle(type)` (`comparisons.ts:16-17`).
pub fn has_fill_style(ty: &str) -> bool {
    has_background(ty) && ty != "stickynote"
}

/// `hasStrokeColor(type)` (`comparisons.ts:19-29`).
pub fn has_stroke_color(ty: &str) -> bool {
    matches!(
        ty,
        "rectangle"
            | "stickynote"
            | "ellipse"
            | "diamond"
            | "freedraw"
            | "arrow"
            | "line"
            | "text"
            | "embeddable"
            | "autoshape"
    )
}

/// `hasStrokeWidth(type)` (`comparisons.ts:31-40`).
pub fn has_stroke_width(ty: &str) -> bool {
    matches!(
        ty,
        "rectangle"
            | "iframe"
            | "embeddable"
            | "ellipse"
            | "diamond"
            | "freedraw"
            | "arrow"
            | "line"
            | "autoshape"
    )
}

/// `hasStrokeStyle(type)` (`comparisons.ts:42-50`).
pub fn has_stroke_style(ty: &str) -> bool {
    matches!(
        ty,
        "rectangle"
            | "iframe"
            | "embeddable"
            | "ellipse"
            | "diamond"
            | "arrow"
            | "line"
            | "autoshape"
    )
}

/// `hasRoughness(type)` (`comparisons.ts:52-53`).
pub fn has_roughness(ty: &str) -> bool {
    has_stroke_style(ty) || ty == "stickynote"
}

/// `hasFreedrawMode(type)` (`comparisons.ts:55`).
pub fn has_freedraw_mode(ty: &str) -> bool {
    ty == "freedraw"
}

/// `canChangeRoundness(type)` (`comparisons.ts:57-64`).
pub fn can_change_roundness(ty: &str) -> bool {
    matches!(
        ty,
        "rectangle" | "iframe" | "embeddable" | "line" | "diamond" | "stickynote" | "image"
    )
}

/// `toolIsArrow(type)` (`comparisons.ts:66`).
pub fn tool_is_arrow(ty: &str) -> bool {
    ty == "arrow"
}

/// `canHaveArrowheads(type)` (`comparisons.ts:68`).
pub fn can_have_arrowheads(ty: &str) -> bool {
    ty == "arrow"
}

fn type_of(element: &Element) -> &'static str {
    element.element_type().as_str()
}

/// `appState.activeTool.type`.
pub(crate) fn active_tool_type<'a>(ctx: &ActionContext<'a>) -> &'a str {
    ctx.field("activeTool", "type")
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// An element held in the app state (`editingTextElement`,
/// `newElement`), when it is one.
pub(crate) fn state_element(value: Option<&Value>) -> Option<Element> {
    match value {
        Some(Value::Object(map)) => Element::from_map(map.clone()).ok(),
        _ => None,
    }
}

/// `getTargetElements(elementsMap, appState)` (`selection.ts:215-231`):
/// the text being edited; else the element being drawn, unless it is the
/// autoshape tool's recognition preview; else the selection with bound
/// text.
pub fn get_target_elements(ctx: &ActionContext<'_>) -> Vec<Element> {
    if truthy(ctx.get("editingTextElement")) {
        return state_element(ctx.get("editingTextElement"))
            .into_iter()
            .collect();
    }
    if truthy(ctx.get("newElement")) && active_tool_type(ctx) != "autoshape" {
        return state_element(ctx.get("newElement")).into_iter().collect();
    }
    ctx.selected(true).into_iter().cloned().collect()
}

/// The non-deleted element `id`, `elementsMap.get(id)`.
fn live<'a>(ctx: &ActionContext<'a>, id: &str) -> Option<&'a Element> {
    ctx.non_deleted().find(|e| e.base.id == id)
}

/// `canChangeStrokeColor(appState, targetElements)`
/// (`shapeActionPredicates.ts:39-61`).
pub fn can_change_stroke_color(active_tool: &str, targets: &[Element]) -> bool {
    let first = targets.first().map(Element::element_type);
    let common = first.filter(|t| targets.iter().all(|e| e.element_type() == *t));
    (has_stroke_color(active_tool)
        && !matches!(
            common,
            Some(ElementType::Image | ElementType::Frame | ElementType::MagicFrame)
        ))
        || targets.iter().any(|e| has_stroke_color(type_of(e)))
}

/// `getColorTargetElement(element, "backgroundColor", elementsMap)`
/// (`stickyNote.ts:95-108`): a sticky note's label stands for the note.
fn background_color_target<'e>(ctx: &ActionContext<'e>, element: &'e Element) -> &'e Element {
    container_id(element)
        .and_then(|id| live(ctx, id))
        .filter(|c| matches!(c.kind, ElementKind::StickyNote(_)))
        .unwrap_or(element)
}

/// `canChangeBackgroundColor(appState, targetElements, elementsMap)`
/// (`shapeActionPredicates.ts:63-77`).
pub fn can_change_background_color(ctx: &ActionContext<'_>, targets: &[Element]) -> bool {
    has_background(active_tool_type(ctx))
        || targets
            .iter()
            .any(|e| has_background(type_of(background_color_target(ctx, e))))
}

/// The container of bound text is not an arrow (a missing container
/// is not one either): `shouldAllowVerticalAlign` and
/// `suppportsHorizontalAlign` (`textElement.ts:475-506`).
fn bound_to_non_arrow(ctx: &ActionContext<'_>, id: &str) -> bool {
    live(ctx, id).is_none_or(|c| !matches!(c.kind, ElementKind::Arrow(_)))
}

/// `shouldAllowVerticalAlign(selectedElements, elementsMap)`.
fn should_allow_vertical_align(ctx: &ActionContext<'_>, targets: &[Element]) -> bool {
    targets
        .iter()
        .any(|e| container_id(e).is_some_and(|id| bound_to_non_arrow(ctx, id)))
}

/// `suppportsHorizontalAlign(selectedElements, elementsMap)`.
fn supports_horizontal_align(ctx: &ActionContext<'_>, targets: &[Element]) -> bool {
    targets.iter().any(|e| match container_id(e) {
        Some(id) => bound_to_non_arrow(ctx, id),
        None => matches!(e.kind, ElementKind::Text(_)),
    })
}

/// `isElbowArrow(element)`.
fn is_elbow_arrow(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Arrow(a) if a.elbowed)
}

/// The flags of `getShapeActionPredicates` (`shapeActionPredicates.ts:
/// 87-189`): whether each styles-panel control is relevant for the active
/// tool and the target elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShapeActionPredicates {
    /// Some element is targeted.
    pub has_selection: bool,
    /// Actions on the selection (duplicate, delete, group, ...): a
    /// selection, and no text being edited or element being drawn.
    pub show_extra_actions: bool,
    pub stroke_color: bool,
    pub background_color: bool,
    pub fill: bool,
    pub stroke_width: bool,
    pub freedraw_mode: bool,
    pub stroke_style: bool,
    pub sloppiness: bool,
    pub roundness: bool,
    pub arrow_type: bool,
    pub arrowheads: bool,
    pub text: bool,
    pub text_align: bool,
    pub vertical_align: bool,
    pub opacity: bool,
    pub layers: bool,
    pub align: bool,
    pub distribute: bool,
    /// The full panel's link: one element, or a container with its text.
    pub link: bool,
    /// The compact and mobile panels' link: exactly one element.
    pub link_single_only: bool,
    pub crop_editor: bool,
    pub line_editor: bool,
}

impl ShapeActionPredicates {
    /// The flag behind a [`PanelGate`].
    pub fn gate(&self, gate: PanelGate) -> bool {
        match gate {
            PanelGate::StrokeColor => self.stroke_color,
            PanelGate::BackgroundColor => self.background_color,
            PanelGate::Fill => self.fill,
            PanelGate::StrokeWidth => self.stroke_width,
            PanelGate::StrokeStyle => self.stroke_style,
            PanelGate::FreedrawMode => self.freedraw_mode,
            PanelGate::Sloppiness => self.sloppiness,
            PanelGate::Roundness => self.roundness,
            PanelGate::ArrowType => self.arrow_type,
            PanelGate::Text => self.text,
            PanelGate::TextAlign => self.text_align,
            PanelGate::VerticalAlign => self.vertical_align,
            PanelGate::Arrowheads => self.arrowheads,
            PanelGate::Opacity => self.opacity,
            PanelGate::Layers => self.layers,
            PanelGate::Align => self.align,
            PanelGate::Distribute => self.distribute,
            PanelGate::ShowExtraActions => self.show_extra_actions,
            PanelGate::Link => self.link,
            PanelGate::CropEditor => self.crop_editor,
            PanelGate::LineEditor => self.line_editor,
        }
    }
}

/// `getShapeActionPredicates(appState, getTargetElements(elementsMap,
/// appState), elementsMap, app)`, as the styles panels call it.
pub fn get_shape_action_predicates(ctx: &ActionContext<'_>) -> ShapeActionPredicates {
    let targets = get_target_elements(ctx);
    shape_action_predicates(ctx, &targets)
}

/// `getShapeActionPredicates(appState, targetElements, elementsMap, app)`
/// (`shapeActionPredicates.ts:87-189`).
pub fn shape_action_predicates(
    ctx: &ActionContext<'_>,
    targets: &[Element],
) -> ShapeActionPredicates {
    let tool = active_tool_type(ctx);
    // relevant to the active tool (preconfigured before drawing) or to a
    // target element
    let for_tool_or_selection =
        |p: fn(&str) -> bool| p(tool) || targets.iter().any(|e| p(type_of(e)));
    let single_selected = targets.len() == 1;
    // a container and its bound text read as one element
    let single_bound_container = targets.len() == 2 && targets.iter().any(has_bound_text_element);
    let editing_or_new = truthy(ctx.get("editingTextElement")) || truthy(ctx.get("newElement"));
    let has_selection = !targets.is_empty();
    let current_background = ctx
        .get("currentItemBackgroundColor")
        .and_then(Value::as_str)
        .unwrap_or("");
    let is_type = |ty: ElementType| targets.iter().any(|e| e.element_type() == ty);
    let first = targets.first();
    ShapeActionPredicates {
        has_selection,
        show_extra_actions: has_selection && !editing_or_new,
        stroke_color: can_change_stroke_color(tool, targets),
        background_color: can_change_background_color(ctx, targets),
        // bucket fill never renders transparent, so its fill style stays
        // relevant either way
        fill: tool == "bucketfill"
            || (has_fill_style(tool) && !is_transparent(current_background))
            || targets
                .iter()
                .any(|e| has_fill_style(type_of(e)) && !is_transparent(&e.base.background_color)),
        stroke_width: for_tool_or_selection(has_stroke_width),
        freedraw_mode: for_tool_or_selection(has_freedraw_mode),
        stroke_style: for_tool_or_selection(has_stroke_style),
        sloppiness: for_tool_or_selection(has_roughness),
        roundness: for_tool_or_selection(can_change_roundness),
        arrow_type: for_tool_or_selection(tool_is_arrow),
        arrowheads: for_tool_or_selection(can_have_arrowheads),
        text: tool == "text" || is_type(ElementType::Text),
        text_align: tool == "text" || supports_horizontal_align(ctx, targets),
        vertical_align: should_allow_vertical_align(ctx, targets),
        opacity: tool != "autoshape" || has_selection,
        // z-order is hidden while the freedraw or autoshape tool is active
        // without a freedraw element selected
        layers: (tool != "freedraw" && tool != "autoshape" && !is_type(ElementType::Freedraw))
            || ctx
                .selected(false)
                .iter()
                .any(|e| e.element_type() == ElementType::Freedraw),
        align: !single_bound_container && align_actions_predicate(ctx),
        distribute: targets.len() > 2,
        link: single_selected || single_bound_container,
        link_single_only: single_selected,
        crop_editor: !truthy(ctx.get("croppingElementId"))
            && single_selected
            && first.is_some_and(|e| matches!(e.kind, ElementKind::Image(_))),
        line_editor: !truthy(ctx.field("selectedLinearElement", "isEditing"))
            && single_selected
            && first.is_some_and(|e| {
                matches!(e.kind, ElementKind::Line(_) | ElementKind::Arrow(_)) && !is_elbow_arrow(e)
            }),
    }
}

/// Tools whose panel shows without a selection
/// (`showSelectedShapeActions.ts:14-20`): not these.
const TOOLS_WITHOUT_PANEL: [&str; 5] = ["selection", "lasso", "eraser", "hand", "laser"];

/// `showSelectedShapeActions(appState, elements)`
/// (`showSelectedShapeActions.ts:7-22`) over the non-deleted elements, as
/// LayerUI calls it: not in view mode or while picking a link target, and
/// either a drawing tool (or text being edited) outside a custom tool, or
/// a selection.
pub fn show_selected_shape_actions(ctx: &ActionContext<'_>) -> bool {
    let tool = active_tool_type(ctx);
    !ctx.flag("viewModeEnabled")
        && ctx.open_dialog_name() != Some("elementLinkSelector")
        && ((tool != "custom"
            && (truthy(ctx.get("editingTextElement")) || !TOOLS_WITHOUT_PANEL.contains(&tool)))
            || ctx.is_some_element_selected())
}
