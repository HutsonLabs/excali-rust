//! Frame membership and clipping: the parts of
//! `packages/element/src/frame.ts` the static scene reads to clip a
//! frame's children (`clipElementToFrame` and `frameClip`,
//! `packages/excalidraw/renderer/staticScene.ts:165-189, 358-393`).
//!
//! An element is clipped to its target frame ([`get_target_frame`]: the
//! frame it belongs to, or the highlighted frame it is being dragged over)
//! when frames render and clip, and either it or the frame is drawn
//! translated by a render override, or [`should_apply_frame_clip`] says
//! so: it crosses the frame's outline or contains the frame, or it is
//! outside the frame but in a group that belongs there. The clip is a
//! `roundRect` of the frame's size with radius `FRAME_STYLE.radius / zoom`
//! at the frame's corner ([`frame_clip`]); a rotated frame clips
//! unrotated, as upstream does.
//!
//! The outlines are [`get_element_line_segments`]'s, so an element whose
//! shape cannot be built (where upstream's drawing throws) fails here too.

use std::collections::{HashMap, HashSet};

use excali_core::element::{Element, ElementKind};
use excali_math::segments_intersect_at;

use crate::bounds::{
    bounds_contain_bounds, get_common_bounds, get_container_element, get_element_absolute_coords,
    get_element_bounds, get_element_line_segments, ElementsMap,
};
use crate::display::{Clip, FillRule, Path, Transform};
use crate::export::frame_style;
use crate::render_element::get_containing_frame;
use crate::shape::ShapeError;
use crate::static_scene::StaticCanvasAppState;

/// The groups whose frame membership is already decided while a scene is
/// drawn (`checkedGroups`, the static scene's `inFrameGroupsMap`): group
/// id to whether it is in the frame.
pub type CheckedGroups = HashMap<String, bool>;

/// `isFrameLikeElement(element)` (`typeChecks.ts`): frames and magic
/// frames.
pub fn is_frame_like(element: &Element) -> bool {
    matches!(
        element.kind,
        ElementKind::Frame(_) | ElementKind::MagicFrame(_)
    )
}

/// `isElementIntersectingFrame(element, frame, elementsMap)`
/// (`frame.ts:75-91`): some segment of the frame's outline crosses some
/// segment of the element's ([`get_element_line_segments`],
/// `segmentsIntersectAt`).
pub fn is_element_intersecting_frame(
    element: &Element,
    frame: &Element,
    elements_map: &ElementsMap<'_>,
) -> Result<bool, ShapeError> {
    let frame_segments = get_element_line_segments(frame, elements_map)?;
    let element_segments = get_element_line_segments(element, elements_map)?;
    Ok(frame_segments.iter().any(|&f| {
        element_segments
            .iter()
            .any(|&e| segments_intersect_at(f, e).is_some())
    }))
}

/// `isElementContainingFrame(element, frame, elementsMap)`
/// (`frame.ts:106-115`): the element's bounds hold the frame's, edges
/// included.
pub fn is_element_containing_frame(
    element: &Element,
    frame: &Element,
    elements_map: &ElementsMap<'_>,
) -> bool {
    bounds_contain_bounds(
        get_element_bounds(element, elements_map),
        get_element_bounds(frame, elements_map),
    )
}

/// `elementsAreInFrameBounds(elements, frame, elementsMap)`
/// (`frame.ts:127-146`): the elements' common bounds (looked up among the
/// elements alone, `getCommonBounds(elements)`) lie within the frame's
/// unrotated box, edges included.
pub fn elements_are_in_frame_bounds(
    elements: &[&Element],
    frame: &Element,
    elements_map: &ElementsMap<'_>,
) -> bool {
    let [frame_x1, frame_y1, frame_x2, frame_y2, _, _] =
        get_element_absolute_coords(frame, elements_map, false);
    let [element_x1, element_y1, element_x2, element_y2] = get_common_bounds(elements);
    frame_x1 <= element_x1
        && frame_y1 <= element_y1
        && frame_x2 >= element_x2
        && frame_y2 >= element_y2
}

/// `elementOverlapsWithFrame(element, frame, elementsMap)`
/// (`frame.ts:148-158`): inside the frame's box, crossing its outline, or
/// containing it.
pub fn element_overlaps_with_frame(
    element: &Element,
    frame: &Element,
    elements_map: &ElementsMap<'_>,
) -> Result<bool, ShapeError> {
    Ok(
        elements_are_in_frame_bounds(&[element], frame, elements_map)
            || is_element_intersecting_frame(element, frame, elements_map)?
            || is_element_containing_frame(element, frame, elements_map),
    )
}

/// `getElementsInGroup(elements, groupId)` (`groups.ts:289-304`): the
/// elements whose `groupIds` hold `group_id`.
pub fn get_elements_in_group<'a>(
    elements_map: &ElementsMap<'a>,
    group_id: &str,
) -> Vec<&'a Element> {
    elements_map
        .values()
        .filter(|e| e.base.group_ids.iter().any(|g| g == group_id))
        .collect()
}

/// The element a text stands for in frame membership: its container when
/// the map has it.
fn frame_member<'a>(element: &'a Element, elements_map: &ElementsMap<'a>) -> &'a Element {
    match element.kind {
        ElementKind::Text(_) => get_container_element(element, elements_map).unwrap_or(element),
        _ => element,
    }
}

/// `element.frameId` is truthy.
fn frame_id(element: &Element) -> Option<&str> {
    element.base.frame_id.as_deref().filter(|id| !id.is_empty())
}

/// `getTargetFrame(element, elementsMap, appState)` (`frame.ts:790-815`):
/// the frame the element (a text: its container) is going to be added to
/// or removed from. Its own frame when it and that frame are both
/// selected; the highlighted frame when it is selected and being dragged;
/// otherwise its own frame (`getContainingFrame`).
pub fn get_target_frame<'a>(
    element: &'a Element,
    elements_map: &ElementsMap<'a>,
    app_state: &'a StaticCanvasAppState,
) -> Option<&'a Element> {
    let member = frame_member(element, elements_map);
    let selected = |id: &str| app_state.selected_element_ids.contains(id);
    // if the element and its containing frame are both selected, then the
    // containing frame is the target frame
    if frame_id(member).is_some_and(|frame| selected(&member.base.id) && selected(frame)) {
        return get_containing_frame(member, elements_map);
    }
    if selected(&member.base.id) && app_state.selected_elements_are_being_dragged {
        app_state.frame_to_highlight.as_ref()
    } else {
        get_containing_frame(member, elements_map)
    }
}

/// `isElementInFrame(element, allElementsMap, appState, { targetFrame,
/// checkedGroups })` (`frame.ts:817-909`): whether the element (a text:
/// its container) is in `target_frame` (default: [`get_target_frame`]).
///
/// - No frame: no.
/// - Unless the element is selected and being dragged without its frame
///   selected too, its membership cannot change: yes.
/// - Ungrouped: whether it overlaps the frame.
/// - Grouped: a group already decided in `checked_groups` answers; else
///   the other members of its undecided groups decide (while editing a
///   group, a highlighted frame answers yes, and the selection is left
///   out): no if any is a frame, yes if any overlaps the frame. The answer
///   is recorded for every group of the element.
pub fn is_element_in_frame<'a>(
    element: &'a Element,
    all_elements_map: &ElementsMap<'a>,
    app_state: &'a StaticCanvasAppState,
    target_frame: Option<&'a Element>,
    mut checked_groups: Option<&mut CheckedGroups>,
) -> Result<bool, ShapeError> {
    let Some(frame) =
        target_frame.or_else(|| get_target_frame(element, all_elements_map, app_state))
    else {
        return Ok(false);
    };
    let member = frame_member(element, all_elements_map);
    let selected = |id: &str| app_state.selected_element_ids.contains(id);
    let dragging = app_state.selected_elements_are_being_dragged;

    if !selected(&member.base.id)
        || !dragging
        // if both frame and element are selected, won't update
        // membership, so return true
        || (selected(&member.base.id) && selected(&frame.base.id))
    {
        return Ok(true);
    }

    let groups = &member.base.group_ids;
    if groups.is_empty() {
        return element_overlaps_with_frame(member, frame, all_elements_map);
    }

    if let Some(checked) = checked_groups.as_deref() {
        for gid in groups {
            if let Some(&in_frame) = checked.get(gid) {
                return Ok(in_frame);
            }
        }
    }

    let mut seen = HashSet::new();
    let mut in_group: Vec<&Element> = groups
        .iter()
        .filter(|gid| {
            checked_groups
                .as_deref()
                .is_none_or(|checked| !checked.contains_key(*gid))
        })
        .flat_map(|gid| get_elements_in_group(all_elements_map, gid))
        .filter(|e| seen.insert(std::ptr::from_ref(*e)))
        .collect();

    if app_state
        .editing_group_id
        .as_deref()
        .is_some_and(|g| !g.is_empty())
        && dragging
    {
        let editing_group_overlaps_frame = app_state.frame_to_highlight.is_some();
        if editing_group_overlaps_frame {
            return Ok(true);
        }
        // getSelectedElements: the selected elements that are not deleted
        in_group.retain(|e| !(selected(&e.base.id) && !e.base.is_deleted));
    }

    let mut set_groups_in_frame = |in_frame: bool| {
        if let Some(checked) = checked_groups.as_deref_mut() {
            for gid in groups {
                checked.insert(gid.clone(), in_frame);
            }
        }
    };

    if in_group.iter().any(|e| is_frame_like(e)) {
        set_groups_in_frame(false);
        return Ok(false);
    }
    for e in in_group {
        if element_overlaps_with_frame(e, frame, all_elements_map)? {
            set_groups_in_frame(true);
            return Ok(true);
        }
    }
    Ok(false)
}

/// `shouldApplyFrameClip(element, frame, appState, elementsMap,
/// checkedGroups)` (`frame.ts:911-972`): whether to clip the element to
/// `frame` when frames clip.
///
/// - It crosses the frame's outline or contains the frame (a backdrop):
///   yes, and its groups are recorded as in the frame.
/// - It is grouped and outside the frame's box: when nothing is dragged,
///   whether it belongs to the frame; while dragging, whether it is in the
///   frame ([`is_element_in_frame`]); recorded for its groups.
/// - Otherwise (inside the frame, or ungrouped outside it): no.
pub fn should_apply_frame_clip<'a>(
    element: &'a Element,
    frame: &'a Element,
    app_state: &'a StaticCanvasAppState,
    elements_map: &ElementsMap<'a>,
    mut checked_groups: Option<&mut CheckedGroups>,
) -> Result<bool, ShapeError> {
    if !app_state.frame_rendering.clip {
        return Ok(false);
    }
    // for individual elements, only clip when the element is
    // a. overlapping with the frame, or
    // b. containing the frame, for example when an element is used as a
    //    background and is therefore bigger than the frame and completely
    //    contains the frame
    let should_clip_element_itself = is_element_intersecting_frame(element, frame, elements_map)?
        || is_element_containing_frame(element, frame, elements_map);
    let groups = &element.base.group_ids;
    let record = |checked: Option<&mut CheckedGroups>, value: bool| {
        if let Some(checked) = checked {
            for gid in groups {
                checked.insert(gid.clone(), value);
            }
        }
    };
    if should_clip_element_itself {
        record(checked_groups.as_deref_mut(), true);
        return Ok(true);
    }
    // if an element is outside the frame, but is part of a group that has
    // some elements "in" the frame, we should clip the element
    if !groups.is_empty() && !elements_are_in_frame_bounds(&[element], frame, elements_map) {
        // if no elements are being dragged, we can skip the geometry check
        // because we know if the element is in the given frame or not
        let should_clip = if app_state.selected_elements_are_being_dragged {
            is_element_in_frame(
                element,
                elements_map,
                app_state,
                Some(frame),
                checked_groups.as_deref_mut(),
            )?
        } else {
            element.base.frame_id.as_deref() == Some(frame.base.id.as_str())
        };
        record(checked_groups, should_clip);
        return Ok(should_clip);
    }
    Ok(false)
}

/// `frameClip(frame, context, renderConfig, appState)`
/// (`staticScene.ts:165-189`): the translation to the frame's corner plus
/// the scroll, the clip there (`roundRect(0, 0, width, height,
/// FRAME_STYLE.radius / zoom)`), and the translation back, which the
/// clipped drawing is under.
pub fn frame_clip(
    frame: &Element,
    app_state: &StaticCanvasAppState,
) -> (Transform, Clip, Transform) {
    let b = &frame.base;
    let x = b.x + app_state.scroll_x;
    let y = b.y + app_state.scroll_y;
    let clip = Clip {
        path: Path::round_rect(
            0.0,
            0.0,
            b.width,
            b.height,
            frame_style::RADIUS / app_state.zoom,
        ),
        rule: FillRule::NonZero,
    };
    (
        Transform::translate(x, y),
        clip,
        Transform::translate(-x, -y),
    )
}
