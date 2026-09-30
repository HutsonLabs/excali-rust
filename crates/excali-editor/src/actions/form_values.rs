//! What the styles panel's controls show: upstream's generic `getFormValue`
//! (`actions/actionProperties.tsx:229-270`) over `reduceToCommonValue`
//! (`common/src/utils.ts:1160-1182`), the value each action's
//! `PanelComponent` reads through it (`actionProperties.tsx:588-2300`),
//! and the other state those components read: the fill's zigzag rule,
//! the polygon and line editor buttons (`actionLinearEditor.tsx`), the
//! align, distribute and group gates (`actionAlign.tsx`,
//! `actionDistribute.tsx`, `actionGroup.tsx`), the link button
//! (`actionLink.tsx`, `components/hyperlink/Hyperlink.tsx:371-382`) and
//! the bucket fill's colour (`App.bucketFill.ts:287-300`).
//!
//! `None` is upstream's `null`: no common value (or, for an arrowhead,
//! none). The colour pickers' values are [`super::form_color`].

use excali_core::constants::stroke_width_by_key;
use excali_core::constants::DEFAULT_FONT_SIZE;
use excali_core::element::{
    Arrowhead, Element, ElementKind, ElementType, FillStyle, FontFamily, RoundnessType,
    StrokeStyle, StrokeVariability, StrokeWidthKey, TextAlign, VerticalAlign,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::context::{container_id, is_frame_like, ActionContext};
use super::registry::{align_actions_predicate, p_group};
use super::shape_predicates::{
    can_have_arrowheads, get_target_elements, has_fill_style, state_element,
};
use crate::bucket_fill::get_bucket_fill_background_color;
use crate::tools::ArrowType;

/// A value `getFormValue` returns, with JavaScript's truthiness (the
/// edited text's value stands only when truthy).
pub trait FormValue: Clone + PartialEq {
    /// `!!value`.
    fn is_truthy(&self) -> bool {
        true
    }
}

impl FormValue for f64 {
    fn is_truthy(&self) -> bool {
        *self != 0.0 && !self.is_nan()
    }
}

impl FormValue for String {
    fn is_truthy(&self) -> bool {
        !self.is_empty()
    }
}

impl FormValue for FontFamily {
    fn is_truthy(&self) -> bool {
        self.0 != 0
    }
}

impl FormValue for FillStyle {}
impl FormValue for StrokeStyle {}
impl FormValue for StrokeWidthKey {}
impl FormValue for StrokeVariability {}
impl FormValue for TextAlign {}
impl FormValue for VerticalAlign {}
impl FormValue for Arrowhead {}
impl FormValue for ArrowType {}
impl FormValue for EdgeRoundness {}

/// `reduceToCommonValue(collection, getValue)`: the value every item has,
/// `None` for an empty collection or when one differs or is null.
pub fn reduce_to_common_value<'a, T: PartialEq>(
    items: impl IntoIterator<Item = &'a Element>,
    get_value: impl Fn(&'a Element) -> Option<T>,
) -> Option<T> {
    let mut common: Option<T> = None;
    for item in items {
        match get_value(item) {
            Some(value) if common.is_none() || common.as_ref() == Some(&value) => {
                common = Some(value)
            }
            _ => return None,
        }
    }
    common
}

/// `getFormValue(elements, app, getValue, elementPredicate, defaultValue)`:
/// the value of the text being edited when truthy; else, with a
/// selection, the common value of the selected elements `predicate`
/// keeps, or `default(true)`; else `default(false)`.
pub fn get_form_value<T: FormValue>(
    ctx: &ActionContext<'_>,
    get_value: impl Fn(&Element) -> Option<T>,
    predicate: impl Fn(&Element) -> bool,
    default: impl Fn(bool) -> Option<T>,
) -> Option<T> {
    let mut ret = None;
    if let Some(editing) = state_element(ctx.get("editingTextElement")) {
        ret = get_value(&editing);
    }
    if !ret.as_ref().is_some_and(T::is_truthy) {
        ret = if ctx.is_some_element_selected() {
            let targets = ctx.selected(false).into_iter().filter(|e| predicate(e));
            reduce_to_common_value(targets, &get_value).or_else(|| default(true))
        } else {
            default(false)
        };
    }
    ret
}

/// `appState[key]` read as `T` (its JSON string or number).
fn state<T: DeserializeOwned>(ctx: &ActionContext<'_>, key: &str) -> Option<T> {
    ctx.get(key)
        .filter(|v| !v.is_null())
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

fn state_number(ctx: &ActionContext<'_>, key: &str) -> Option<f64> {
    ctx.get(key).and_then(Value::as_f64)
}

/// `hasSelection ? null : appState[key]`.
fn unless_selected<'c, T: DeserializeOwned>(
    ctx: &'c ActionContext<'_>,
    key: &'static str,
) -> impl Fn(bool) -> Option<T> + 'c {
    move |has_selection| if has_selection { None } else { state(ctx, key) }
}

// -- fill, stroke, sloppiness, opacity ---------------------------------------------

/// `changeFillStyle`'s value: the fill-capable elements' common fill.
pub fn form_fill_style(ctx: &ActionContext<'_>) -> Option<FillStyle> {
    get_form_value(
        ctx,
        |e| Some(e.base.fill_style),
        |e| has_fill_style(e.element_type().as_str()),
        unless_selected(ctx, "currentItemFillStyle"),
    )
}

/// The fill styles of the selected fill-capable elements
/// (`selectedFillStyleElements`, `actionProperties.tsx:630-633`): all
/// zigzag shows the hachure button as zigzag, and Alt-click on hachure
/// turns an all-hachure selection zigzag.
pub fn selected_fill_styles(ctx: &ActionContext<'_>) -> Vec<FillStyle> {
    ctx.selected(false)
        .into_iter()
        .filter(|e| has_fill_style(e.element_type().as_str()))
        .map(|e| e.base.fill_style)
        .collect()
}

/// `StrokeWidthKey`'s string.
pub fn stroke_width_key_str(key: StrokeWidthKey) -> &'static str {
    match key {
        StrokeWidthKey::Thin => "thin",
        StrokeWidthKey::Medium => "medium",
        StrokeWidthKey::Bold => "bold",
    }
}

const STROKE_WIDTH_KEYS: [StrokeWidthKey; 3] = [
    StrokeWidthKey::Thin,
    StrokeWidthKey::Medium,
    StrokeWidthKey::Bold,
];

/// `changeStrokeWidth`'s value: the key whose width (by element type)
/// each element has (`getStrokeWidthKeyForElement`).
pub fn form_stroke_width_key(ctx: &ActionContext<'_>) -> Option<StrokeWidthKey> {
    get_form_value(
        ctx,
        |e| {
            STROKE_WIDTH_KEYS
                .into_iter()
                .find(|k| stroke_width_by_key(e.element_type(), *k) == e.base.stroke_width)
        },
        |_| true,
        |has_selection| {
            if has_selection {
                return None;
            }
            let key = ctx
                .get("currentItemStrokeWidthKey")
                .and_then(Value::as_str)?;
            STROKE_WIDTH_KEYS
                .into_iter()
                .find(|k| stroke_width_key_str(*k) == key)
        },
    )
}

/// `changeSloppiness`'s value, the roughness.
pub fn form_roughness(ctx: &ActionContext<'_>) -> Option<f64> {
    get_form_value(
        ctx,
        |e| Some(e.base.roughness),
        |_| true,
        |has_selection| {
            if has_selection {
                None
            } else {
                state_number(ctx, "currentItemRoughness")
            }
        },
    )
}

/// `changeFreedrawMode`'s value: the freedraw elements' common pressure
/// mode, `?? appState.currentItemStrokeVariability`.
pub fn form_stroke_variability(ctx: &ActionContext<'_>) -> Option<StrokeVariability> {
    get_form_value(
        ctx,
        |e| match &e.kind {
            ElementKind::Freedraw(f) => Some(f.stroke_options.variability),
            _ => None,
        },
        |e| e.element_type() == ElementType::Freedraw,
        unless_selected(ctx, "currentItemStrokeVariability"),
    )
    .or_else(|| state(ctx, "currentItemStrokeVariability"))
}

/// `changeStrokeStyle`'s value.
pub fn form_stroke_style(ctx: &ActionContext<'_>) -> Option<StrokeStyle> {
    get_form_value(
        ctx,
        |e| Some(e.base.stroke_style),
        |_| true,
        unless_selected(ctx, "currentItemStrokeStyle"),
    )
}

/// `changeOpacity`'s value; the Range shows `?? currentItemOpacity`.
pub fn form_opacity(ctx: &ActionContext<'_>) -> Option<f64> {
    get_form_value(
        ctx,
        |e| Some(e.base.opacity),
        |_| true,
        |has_selection| {
            if has_selection {
                None
            } else {
                state_number(ctx, "currentItemOpacity")
            }
        },
    )
}

// -- text ----------------------------------------------------------------------------

fn text_fields(element: &Element) -> Option<&excali_core::element::TextFields> {
    match &element.kind {
        ElementKind::Text(t) => Some(t),
        _ => None,
    }
}

/// The value of a text element, or of a container's label
/// (`getBoundTextElement` over the non-deleted elements).
fn text_or_label<'a, T>(
    ctx: &ActionContext<'a>,
    element: &'a Element,
    read: impl Fn(&Element) -> Option<T>,
) -> Option<T> {
    if text_fields(element).is_some() {
        return read(element);
    }
    ctx.bound_text_of(element).and_then(read)
}

/// `isTextElement(element) || getBoundTextElement(element) !== null`.
fn has_text(ctx: &ActionContext<'_>, element: &Element) -> bool {
    text_fields(element).is_some() || ctx.bound_text_of(element).is_some()
}

/// `getBaseFontSize(text, elementsMap)` (`stickyNote.ts:405-412`): a
/// note's label's picked size (`baseFontSize`), else its font size.
fn base_font_size(ctx: &ActionContext<'_>, text: &Element) -> Option<f64> {
    let t = text_fields(text)?;
    let on_note = container_id(text)
        .and_then(|id| ctx.non_deleted().find(|e| e.base.id == id))
        .is_some_and(|c| c.element_type() == ElementType::StickyNote);
    Some(if on_note {
        t.base_font_size.unwrap_or(t.font_size)
    } else {
        t.font_size
    })
}

/// `changeFontSize`'s value (a note's label by its base size); the
/// default is `currentItemFontSize || DEFAULT_FONT_SIZE`.
pub fn form_font_size(ctx: &ActionContext<'_>) -> Option<f64> {
    get_form_value(
        ctx,
        |e| {
            if text_fields(e).is_some() {
                return base_font_size(ctx, e);
            }
            ctx.bound_text_of(e).and_then(|t| base_font_size(ctx, t))
        },
        |e| has_text(ctx, e),
        |has_selection| {
            if has_selection {
                return None;
            }
            Some(
                state_number(ctx, "currentItemFontSize")
                    .filter(|n| n.is_truthy())
                    .unwrap_or(DEFAULT_FONT_SIZE),
            )
        },
    )
}

/// `changeFontFamily`'s value, the picker's `selectedFontFamily` with its
/// popup closed; the default is `currentItemFontFamily ||
/// DEFAULT_FONT_FAMILY`.
pub fn form_font_family(ctx: &ActionContext<'_>) -> Option<FontFamily> {
    get_form_value(
        ctx,
        |e| text_or_label(ctx, e, |t| text_fields(t).map(|f| f.font_family)),
        |e| has_text(ctx, e),
        |has_selection| {
            if has_selection {
                return None;
            }
            Some(
                state_number(ctx, "currentItemFontFamily")
                    .filter(|n| n.is_truthy())
                    .map_or(FontFamily::DEFAULT, |n| FontFamily(n as u32)),
            )
        },
    )
}

/// `changeTextAlign`'s value.
pub fn form_text_align(ctx: &ActionContext<'_>) -> Option<TextAlign> {
    get_form_value(
        ctx,
        |e| text_or_label(ctx, e, |t| text_fields(t).map(|f| f.text_align)),
        |e| has_text(ctx, e),
        unless_selected(ctx, "currentItemTextAlign"),
    )
}

/// `changeVerticalAlign`'s value: a bound text's, or a container's
/// label's (a free text has none); `middle` with nothing selected.
pub fn form_vertical_align(ctx: &ActionContext<'_>) -> Option<VerticalAlign> {
    get_form_value(
        ctx,
        |e| {
            if let Some(t) = text_fields(e) {
                if t.container_id.as_deref().is_some_and(|c| !c.is_empty()) {
                    return Some(t.vertical_align);
                }
            }
            ctx.bound_text_of(e)
                .and_then(text_fields)
                .map(|t| t.vertical_align)
        },
        |e| has_text(ctx, e),
        |has_selection| (!has_selection).then_some(VerticalAlign::Middle),
    )
}

// -- edges and arrows ------------------------------------------------------------------

/// `changeRoundness`'s values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeRoundness {
    Sharp,
    Round,
}

impl EdgeRoundness {
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeRoundness::Sharp => "sharp",
            EdgeRoundness::Round => "round",
        }
    }

    fn parse(s: &str) -> Option<EdgeRoundness> {
        match s {
            "sharp" => Some(EdgeRoundness::Sharp),
            "round" => Some(EdgeRoundness::Round),
            _ => None,
        }
    }
}

/// `changeRoundness`'s value: the non-arrows' common edge, none when a
/// target (`getTargetElements`) has the legacy roundness.
pub fn form_roundness(ctx: &ActionContext<'_>) -> Option<EdgeRoundness> {
    let legacy = get_target_elements(ctx).iter().any(|e| {
        e.base
            .roundness
            .is_some_and(|r| r.kind == RoundnessType::Legacy)
    });
    get_form_value(
        ctx,
        |e| {
            if legacy {
                None
            } else if e.base.roundness.is_some() {
                Some(EdgeRoundness::Round)
            } else {
                Some(EdgeRoundness::Sharp)
            }
        },
        |e| e.element_type() != ElementType::Arrow,
        |has_selection| {
            if has_selection {
                return None;
            }
            ctx.get("currentItemRoundness")
                .and_then(Value::as_str)
                .and_then(EdgeRoundness::parse)
        },
    )
}

/// Which end an arrowhead is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowheadPosition {
    Start,
    End,
}

impl ArrowheadPosition {
    pub fn as_str(self) -> &'static str {
        match self {
            ArrowheadPosition::Start => "start",
            ArrowheadPosition::End => "end",
        }
    }

    fn state_key(self) -> &'static str {
        match self {
            ArrowheadPosition::Start => "currentItemStartArrowhead",
            ArrowheadPosition::End => "currentItemEndArrowhead",
        }
    }
}

/// `appState.currentItem{Start,End}Arrowhead`, normalized.
fn current_arrowhead(ctx: &ActionContext<'_>, position: ArrowheadPosition) -> Option<Arrowhead> {
    ctx.get(position.state_key())
        .and_then(Value::as_str)
        .and_then(Arrowhead::normalize)
}

/// `changeArrowhead`'s value at `position` (`None` is no arrowhead, or
/// none in common): a linear element's arrowhead for the picker
/// (`getArrowheadForPicker`), any other element standing for the
/// current item's.
pub fn form_arrowhead(ctx: &ActionContext<'_>, position: ArrowheadPosition) -> Option<Arrowhead> {
    get_form_value(
        ctx,
        |e| match e.kind.linear() {
            Some(l) if can_have_arrowheads(e.element_type().as_str()) => match position {
                ArrowheadPosition::Start => l.start_arrowhead,
                ArrowheadPosition::End => l.end_arrowhead,
            },
            _ => current_arrowhead(ctx, position),
        },
        |_| true,
        |has_selection| {
            if has_selection {
                None
            } else {
                current_arrowhead(ctx, position)
            }
        },
    )
}

/// `ARROW_TYPE`'s string.
pub fn arrow_type_str(t: ArrowType) -> &'static str {
    match t {
        ArrowType::Sharp => "sharp",
        ArrowType::Round => "round",
        ArrowType::Elbow => "elbow",
    }
}

/// `changeArrowType`'s value: elbow, round (with roundness) or sharp.
pub fn form_arrow_type(ctx: &ActionContext<'_>) -> Option<ArrowType> {
    get_form_value(
        ctx,
        |e| match &e.kind {
            ElementKind::Arrow(a) if a.elbowed => Some(ArrowType::Elbow),
            ElementKind::Arrow(_) if e.base.roundness.is_some() => Some(ArrowType::Round),
            ElementKind::Arrow(_) => Some(ArrowType::Sharp),
            _ => None,
        },
        |e| e.element_type() == ElementType::Arrow,
        |has_selection| {
            if has_selection {
                return None;
            }
            let s = ctx.get("currentItemArrowType").and_then(Value::as_str)?;
            [ArrowType::Sharp, ArrowType::Round, ArrowType::Elbow]
                .into_iter()
                .find(|t| arrow_type_str(*t) == s)
        },
    )
}

// -- buttons ---------------------------------------------------------------------------

/// `togglePolygon`'s button (`actionLinearEditor.tsx:170-205`): shown
/// when every selected element is a polygon line of 3 points or more,
/// `Some(checked)` (every one a polygon); `None` renders nothing.
pub fn polygon_toggle(ctx: &ActionContext<'_>) -> Option<bool> {
    let selected = ctx.selected(false);
    let polygon = |e: &Element| match &e.kind {
        ElementKind::Line(l) => l.polygon && l.linear.points.len() >= 3,
        _ => false,
    };
    if selected.is_empty() || !selected.iter().all(|e| polygon(e)) {
        return None;
    }
    Some(
        selected
            .iter()
            .all(|e| matches!(&e.kind, ElementKind::Line(l) if l.polygon)),
    )
}

/// `toggleLinearEditor`'s element (`actionLinearEditor.tsx:80-102`): the
/// first selected element, whose type labels the button; `None` renders
/// nothing.
pub fn linear_editor_target<'a>(ctx: &ActionContext<'a>) -> Option<&'a Element> {
    ctx.selected(false).into_iter().next()
}

/// `alignActionsPredicate` (`actionAlign.tsx:36-52`): the align buttons
/// are hidden unless more than one unit, and no frame, is selected.
pub fn align_enabled(ctx: &ActionContext<'_>) -> bool {
    align_actions_predicate(ctx)
}

/// `actionDistribute.tsx`'s `enableActionGroup` (:35-46): more than two
/// units and no frame.
pub fn distribute_enabled(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    ctx.selected_units(&selected) > 2 && !selected.iter().any(|e| is_frame_like(e))
}

/// `actionGroup.tsx`'s `enableActionGroup` (:73-84).
pub fn group_enabled(ctx: &ActionContext<'_>) -> bool {
    p_group(ctx)
}

/// `getSelectedGroupIds(appState).length > 0`: the ungroup button shows.
pub fn has_selected_groups(ctx: &ActionContext<'_>) -> bool {
    !ctx.selected_group_ids().is_empty()
}

/// `isSomeElementSelected(getNonDeletedElements(elements), appState)`.
pub fn is_some_element_selected(ctx: &ActionContext<'_>) -> bool {
    ctx.is_some_element_selected()
}

/// The link button's state (`actionLink.tsx:45-66`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkPanelState {
    /// The `aria-label`'s locale key (`getContextMenuLabel`).
    pub label: &'static str,
    /// The title's locale key: `labels.link.labelEmbed` when the scene's
    /// first element is an embeddable, as upstream reads `elements[0]`.
    pub title: &'static str,
    /// One element selected, with a link.
    pub checked: bool,
}

/// [`LinkPanelState`] for the selection.
pub fn link_panel_state(ctx: &ActionContext<'_>) -> LinkPanelState {
    let selected = ctx.selected(false);
    let first = selected.first();
    let is_embeddable = |e: &Element| e.element_type() == ElementType::Embeddable;
    let has_link = |e: &Element| e.base.link.as_deref().is_some_and(|l| !l.is_empty());
    let label = match first {
        Some(e) if is_embeddable(e) => "labels.link.editEmbed",
        Some(e) if has_link(e) => "labels.link.edit",
        _ => "labels.link.create",
    };
    let title = if ctx.elements.first().is_some_and(is_embeddable) {
        "labels.link.labelEmbed"
    } else {
        "labels.link.label"
    };
    LinkPanelState {
        label,
        title,
        checked: selected.len() == 1 && has_link(selected[0]),
    }
}

/// The bucket fill picker's colour: `currentItemBackgroundColor`, or the
/// tool's green when that is transparent (`getBucketFillBackgroundColor`
/// without a theme).
pub fn bucket_fill_color(ctx: &ActionContext<'_>) -> String {
    let current = ctx
        .get("currentItemBackgroundColor")
        .and_then(Value::as_str)
        .unwrap_or("");
    get_bucket_fill_background_color(current, false)
}
