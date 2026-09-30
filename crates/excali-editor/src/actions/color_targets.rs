//! Who a stroke or background colour pick targets: `resolveColorTarget`
//! (`actions/colorTargets.ts:103-176`), `getColorTargetElement`
//! (`element/src/stickyNote.ts:95-108`), and the colour the two colour
//! actions' pickers show, `getFormValue` as their `PanelComponent`s call
//! it (`actions/actionProperties.tsx:229-270`, :398-404 and :533-541).

use excali_core::color::{
    ColorTuple, PaletteEntry, DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE,
    DEFAULT_ELEMENT_BACKGROUND_PICKS, DEFAULT_ELEMENT_STROKE_COLOR_PALETTE,
    DEFAULT_ELEMENT_STROKE_PICKS, STICKY_NOTE_BACKGROUND_PICKS, STICKY_NOTE_STROKE_PICKS,
};
use excali_core::element::{Element, ElementKind};
use serde_json::Value;

use super::context::{container_id, ActionContext};
use super::shape_predicates::{active_tool_type, has_background, has_stroke_color, state_element};

/// `ColorProperty`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorProperty {
    StrokeColor,
    BackgroundColor,
}

/// `ColorTargetKind`: the regular colour domain, the sticky notes' own,
/// or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorTargetKind {
    Regular,
    Sticky,
    Mixed,
}

/// `ColorTarget` (`colorTargets.ts:50-61`).
#[derive(Debug, Clone, PartialEq)]
pub struct ColorTarget {
    pub kind: ColorTargetKind,
    pub property: ColorProperty,
    /// The current-item defaults a pick is written to.
    pub app_state_keys: Vec<&'static str>,
    /// The default shown when no element is targeted.
    pub current_value: Option<String>,
    pub palette: &'static [PaletteEntry],
    pub top_picks: ColorTuple,
    /// The `appState.colorTopPicks` slot.
    pub customizable_top_picks: &'static str,
    pub excluded_colors: Option<&'static [&'static str]>,
}

/// `STICKY_NOTE_EXCLUDED_COLORS` (`colorTargets.ts:80-82`).
const STICKY_NOTE_EXCLUDED_COLORS: &[&str] = &["transparent"];

fn is_sticky_note(element: Option<&Element>) -> bool {
    element.is_some_and(|e| matches!(e.kind, ElementKind::StickyNote(_)))
}

/// `elementsMap.get(id)` over every element, deleted ones included
/// (`arrayToMap(elements)`).
fn any<'a>(ctx: &ActionContext<'a>, id: &str) -> Option<&'a Element> {
    ctx.elements.iter().find(|e| e.base.id == id)
}

/// `elementsMap.get(id)` over the non-deleted elements.
fn live<'a>(ctx: &ActionContext<'a>, id: &str) -> Option<&'a Element> {
    ctx.non_deleted().find(|e| e.base.id == id)
}

/// `isStickyNoteBoundText(element, elementsMap)`
/// (`stickyNote.ts:387-395`), `lookup` being the map.
fn is_sticky_note_bound_text<'a>(
    element: &Element,
    lookup: impl Fn(&str) -> Option<&'a Element>,
) -> bool {
    container_id(element).is_some_and(|id| is_sticky_note(lookup(id)))
}

/// `getColorTargetElement(element, property, elementsMap)`: a background
/// pick on a note's label lands on the note.
fn color_target_element<'e, 'l: 'e>(
    element: &'e Element,
    property: ColorProperty,
    lookup: impl Fn(&str) -> Option<&'l Element> + Copy,
) -> &'e Element {
    if property == ColorProperty::BackgroundColor
        && matches!(element.kind, ElementKind::Text(_))
        && is_sticky_note_bound_text(element, lookup)
    {
        return container_id(element).and_then(lookup).unwrap_or(element);
    }
    element
}

fn supports(element: &Element, property: ColorProperty) -> bool {
    let ty = element.element_type().as_str();
    match property {
        ColorProperty::StrokeColor => has_stroke_color(ty),
        ColorProperty::BackgroundColor => has_background(ty),
    }
}

/// `resolveColorTarget(appState, elements, property)`: the selected
/// colour-capable elements (with bound labels for the stroke) and the
/// text being edited decide the domain; with none, the active tool does.
pub fn resolve_color_target(ctx: &ActionContext<'_>, property: ColorProperty) -> ColorTarget {
    let lookup = |id: &str| any(ctx, id);
    let mut targets: Vec<&Element> = ctx
        .selected(property == ColorProperty::StrokeColor)
        .into_iter()
        .filter(|e| supports(e, property))
        .collect();
    let editing_text =
        state_element(ctx.get("editingTextElement")).and_then(|e| lookup(&e.base.id));
    if let Some(editing) = editing_text.map(|e| color_target_element(e, property, lookup)) {
        if !editing.base.is_deleted
            && supports(editing, property)
            && !targets.iter().any(|e| e.base.id == editing.base.id)
        {
            targets.push(editing);
        }
    }
    let kind = if targets.is_empty() {
        if active_tool_type(ctx) == "stickynote" {
            ColorTargetKind::Sticky
        } else {
            ColorTargetKind::Regular
        }
    } else {
        let sticky = targets
            .iter()
            .filter(|e| {
                matches!(e.kind, ElementKind::StickyNote(_))
                    || (matches!(e.kind, ElementKind::Text(_))
                        && is_sticky_note_bound_text(e, lookup))
            })
            .count();
        if sticky == 0 {
            ColorTargetKind::Regular
        } else if sticky == targets.len() {
            ColorTargetKind::Sticky
        } else {
            ColorTargetKind::Mixed
        }
    };
    let stroke = property == ColorProperty::StrokeColor;
    let (regular, sticky) = if stroke {
        ("currentItemStrokeColor", "currentItemStickynoteStrokeColor")
    } else {
        (
            "currentItemBackgroundColor",
            "currentItemStickynoteBackgroundColor",
        )
    };
    let is_sticky = kind == ColorTargetKind::Sticky;
    ColorTarget {
        kind,
        property,
        app_state_keys: match kind {
            ColorTargetKind::Mixed => vec![regular, sticky],
            ColorTargetKind::Sticky => vec![sticky],
            ColorTargetKind::Regular => vec![regular],
        },
        current_value: ctx
            .get(if is_sticky { sticky } else { regular })
            .and_then(Value::as_str)
            .map(str::to_string),
        palette: if stroke {
            &DEFAULT_ELEMENT_STROKE_COLOR_PALETTE
        } else {
            &DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE
        },
        top_picks: match (is_sticky, stroke) {
            (true, true) => STICKY_NOTE_STROKE_PICKS,
            (true, false) => STICKY_NOTE_BACKGROUND_PICKS,
            (false, true) => DEFAULT_ELEMENT_STROKE_PICKS,
            (false, false) => DEFAULT_ELEMENT_BACKGROUND_PICKS,
        },
        customizable_top_picks: match (is_sticky, stroke) {
            (true, true) => "stickyNoteStroke",
            (true, false) => "stickyNoteBackground",
            (false, true) => "elementStroke",
            (false, false) => "elementBackground",
        },
        excluded_colors: is_sticky.then_some(STICKY_NOTE_EXCLUDED_COLORS),
    }
}

/// The colour the picker of `target`'s action shows: `getFormValue` with
/// the element's `strokeColor`, or for the background its colour target's
/// `backgroundColor` (over the non-deleted elements), and the target's
/// `currentValue` when nothing is selected; `None` for a selection without
/// a common colour.
pub fn form_color(ctx: &ActionContext<'_>, target: &ColorTarget) -> Option<String> {
    let lookup = |id: &str| live(ctx, id);
    let value = |element: &Element| -> String {
        match target.property {
            ColorProperty::StrokeColor => element.base.stroke_color.clone(),
            ColorProperty::BackgroundColor => {
                color_target_element(element, target.property, lookup)
                    .base
                    .background_color
                    .clone()
            }
        }
    };
    if let Some(editing) = state_element(ctx.get("editingTextElement")) {
        let color = value(&editing);
        if !color.is_empty() {
            return Some(color);
        }
    }
    if !ctx.is_some_element_selected() {
        return target.current_value.clone();
    }
    // reduceToCommonValue (common/src/utils.ts:1160-1182)
    let mut common: Option<String> = None;
    for element in ctx.selected(false) {
        let color = value(element);
        match &common {
            Some(c) if *c != color => return None,
            _ => common = Some(color),
        }
    }
    common
}
