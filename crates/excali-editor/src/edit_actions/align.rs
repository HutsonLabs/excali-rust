//! The align and distribute actions (`packages/excalidraw/actions/
//! actionAlign.tsx`, `actionDistribute.tsx`) over `alignElements`
//! (`packages/element/src/align.ts`), `distributeElements`
//! (`distribute.ts`) and `getSelectedElementsByGroup` (`groups.ts:417-466`).
//!
//! Both move the selection's groups (each selected group, or loose
//! element with its bound text) in the scene with `scene.mutateElement`,
//! the arrows bound to each moved element following
//! (`updateBoundElements` with the group as simultaneously updated), then
//! take the moved elements out of frames they left
//! (`updateFrameMembershipOfSelectedElements`).

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_scene::bounds::{get_bound_text_element, ElementsMap};
use serde_json::{Map, Value};

use super::properties::{StyleEnv, Triggering, Work};
use super::ActionResult;
use super::{editing_group_id, get_selected_elements, is_bound_to_container, object_key, truthy};
use crate::binding::update_bound_elements;
use crate::frame::{update_frame_membership_of_selected_elements, MembershipState};
use crate::resize_elements::{get_common_bounding_box, BoundingBox};
use crate::scene::ElementUpdate;

/// An axis (`"x" | "y"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Axis {
    X,
    Y,
}

/// Where along the axis (`"start" | "center" | "end"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Position {
    Start,
    Center,
    End,
}

/// `Alignment` (`align.ts:14-17`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Alignment {
    position: Position,
    axis: Axis,
}

impl Alignment {
    pub(super) const TOP: Alignment = Alignment {
        position: Position::Start,
        axis: Axis::Y,
    };
    pub(super) const BOTTOM: Alignment = Alignment {
        position: Position::End,
        axis: Axis::Y,
    };
    pub(super) const LEFT: Alignment = Alignment {
        position: Position::Start,
        axis: Axis::X,
    };
    pub(super) const RIGHT: Alignment = Alignment {
        position: Position::End,
        axis: Axis::X,
    };
    pub(super) const VERTICALLY_CENTERED: Alignment = Alignment {
        position: Position::Center,
        axis: Axis::Y,
    };
    pub(super) const HORIZONTALLY_CENTERED: Alignment = Alignment {
        position: Position::Center,
        axis: Axis::X,
    };
}

/// The box's `[min, mid, max, extent]` along the axis.
fn along(b: &BoundingBox, axis: Axis) -> [f64; 4] {
    match axis {
        Axis::X => [b.min_x, b.mid_x, b.max_x, b.width],
        Axis::Y => [b.min_y, b.mid_y, b.max_y, b.height],
    }
}

/// `getSelectedGroupForElement(appState, element) != null`
/// (`groups.ts:215-232`).
fn is_selected_via_group(element: &Element, app_state: &AppState) -> bool {
    let editing = editing_group_id(app_state);
    let selected = object_key(app_state, "selectedGroupIds");
    element
        .base
        .group_ids
        .iter()
        .filter(|g| Some(g.as_str()) != editing.as_deref())
        .any(|g| truthy(selected.get(g)))
}

/// `getSelectedElementsByGroup(selectedElements, elementsMap, appState)`
/// (`groups.ts:417-466`), non-deleted: the ids of each selected group's
/// elements (when one group holds the whole selection, of each of its
/// inner groups) and of each loose element, bound text after its
/// container.
fn selected_elements_by_group(
    selected: &[&Element],
    map: &ElementsMap<'_>,
    app_state: &AppState,
) -> Vec<Vec<String>> {
    let selected_groups = object_key(app_state, "selectedGroupIds");
    let selected_group_count = selected_groups
        .iter()
        .filter(|(_, v)| truthy(Some(v)))
        .count();
    let single_group =
        selected_group_count == 1 && selected.iter().all(|e| is_selected_via_group(e, app_state));
    let mut buckets: Vec<(String, Vec<String>)> = Vec::new();
    for element in selected {
        // bound text goes after its container
        if is_bound_to_container(element) {
            continue;
        }
        let group_ids = &element.base.group_ids;
        let key = match group_ids.iter().find(|g| truthy(selected_groups.get(*g))) {
            None => format!("{}_element", element.base.id),
            Some(group) => {
                let index = group_ids.iter().position(|g| g == group).unwrap_or(0) as isize;
                let key_index = if single_group { index - 1 } else { index };
                if key_index < 0 {
                    format!("{}_element", element.base.id)
                } else {
                    format!("{}_group", group_ids[key_index as usize])
                }
            }
        };
        let mut members = vec![element.base.id.clone()];
        if let Some(text) = get_bound_text_element(element, map) {
            members.push(text.base.id.clone());
        }
        match buckets.iter_mut().find(|(k, _)| *k == key) {
            Some((_, bucket)) => bucket.extend(members),
            None => buckets.push((key, members)),
        }
    }
    buckets.into_iter().map(|(_, ids)| ids).collect()
}

impl<E: StyleEnv> Work<'_, E> {
    /// `app.scene.getSelectedElements(appState)`: the ids of the selected
    /// non-deleted elements.
    fn selected(&self, app_state: &AppState) -> Vec<String> {
        let live = self.scene.non_deleted();
        get_selected_elements(
            &live,
            &object_key(app_state, "selectedElementIds"),
            false,
            false,
        )
        .into_iter()
        .map(|e| e.base.id.clone())
        .collect()
    }

    /// The ids' elements now, non-deleted.
    fn live(&self, ids: &[String]) -> Vec<&Element> {
        ids.iter()
            .filter_map(|id| self.scene.get(id))
            .filter(|e| !e.base.is_deleted)
            .collect()
    }

    /// The selection's groups, by [`selected_elements_by_group`], without
    /// deleted elements.
    fn groups(&self, selected: &[String], app_state: &AppState) -> Vec<Vec<String>> {
        let live = self.scene.non_deleted();
        let map = ElementsMap::new(live.iter().copied());
        let selected = self.live(selected);
        selected_elements_by_group(&selected, &map, app_state)
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .filter(|id| self.scene.get(id).is_some_and(|e| !e.base.is_deleted))
                    .collect()
            })
            .collect()
    }

    fn bounding_box(&self, ids: &[String]) -> BoundingBox {
        get_common_bounding_box(&self.live(ids))
    }

    /// Each element of the group moved by `(dx, dy)`, its bound arrows
    /// following.
    fn translate(&mut self, group: &[String], dx: f64, dy: f64) {
        for id in group {
            let Some(element) = self.scene.get(id) else {
                continue;
            };
            let update = ElementUpdate::position(element.base.x + dx, element.base.y + dy);
            self.scene
                .mutate_element(id, update, &mut Triggering(self.env));
            update_bound_elements(
                &mut self.scene,
                &mut Triggering(self.env),
                id,
                Some(group),
                None,
            );
        }
    }

    /// `updateFrameMembershipOfSelectedElements(elements, appState, app)`
    /// over the scene: the result's elements.
    fn with_frame_membership(&mut self, app_state: &AppState) -> Vec<Element> {
        let mut elements = self.scene.elements().to_vec();
        let state = MembershipState::from_app_state(app_state);
        update_frame_membership_of_selected_elements(&mut elements, &state, self.env);
        elements
    }
}

fn captured(elements: Vec<Element>) -> Option<ActionResult> {
    Some(ActionResult {
        elements: Some(elements),
        app_state: Map::<String, Value>::new(),
        capture: true,
        never: false,
    })
}

/// The six aligns' `perform` (`actionAlign.tsx`, `alignSelectedElements`,
/// `alignElements` of `align.ts:19-52`): each group of the selection moved
/// so that its box's start, centre or end along the axis meets the
/// selection's.
pub(super) fn align<E: StyleEnv>(
    w: &mut Work<'_, E>,
    app_state: &AppState,
    alignment: Alignment,
) -> Option<ActionResult> {
    let selected = w.selected(app_state);
    let groups = w.groups(&selected, app_state);
    let [min, _, max, _] = along(&w.bounding_box(&selected), alignment.axis);
    for group in &groups {
        // calculateTranslation (align.ts:54-82)
        let [group_min, _, group_max, _] = along(&w.bounding_box(group), alignment.axis);
        let translation = match alignment.position {
            Position::Start => min - group_min,
            Position::End => max - group_max,
            Position::Center => (min + max) / 2.0 - (group_min + group_max) / 2.0,
        };
        let (dx, dy) = match alignment.axis {
            Axis::X => (translation, 0.0),
            Axis::Y => (0.0, translation),
        };
        w.translate(group, dx, dy);
    }
    captured(w.with_frame_membership(app_state))
}

/// The two distributes' `perform` (`actionDistribute.tsx`,
/// `distributeSelectedElements`, `distributeElements` of
/// `distribute.ts:19-114`): the selection's groups, in the order of their
/// centres, spaced with equal gaps between the first's start and the
/// last's end; when they overlap more than the span leaves (a negative
/// gap), the centres between the outermost boxes spaced evenly instead.
pub(super) fn distribute<E: StyleEnv>(
    w: &mut Work<'_, E>,
    app_state: &AppState,
    axis: Axis,
) -> Option<ActionResult> {
    let selected = w.selected(app_state);
    let [start, _, end, extent] = along(&w.bounding_box(&selected), axis);
    let mut groups: Vec<(Vec<String>, [f64; 4])> = w
        .groups(&selected, app_state)
        .into_iter()
        .map(|g| {
            let b = along(&w.bounding_box(&g), axis);
            (g, b)
        })
        .collect();
    groups.sort_by(|a, b| {
        (a.1[1] - b.1[1])
            .partial_cmp(&0.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let span: f64 = groups.iter().map(|(_, b)| b[3]).sum();
    let n = groups.len() as f64;
    let step = (extent - span) / (n - 1.0);
    let moves: Vec<(usize, f64)> = if step < 0.0 {
        // distribute from centers
        let index0 = groups.iter().position(|(_, b)| b[0] == start);
        let index1 = groups.iter().position(|(_, b)| b[2] == end);
        let (Some(index0), Some(index1)) = (index0, index1) else {
            // upstream reads groups[-1] and throws
            return None;
        };
        let step = (groups[index1].1[1] - groups[index0].1[1]) / (n - 1.0);
        let mut pos = groups[index0].1[1];
        groups
            .iter()
            .enumerate()
            .map(|(index, (_, b))| {
                if index != index0 && index != index1 {
                    pos += step;
                    (index, pos - b[1])
                } else {
                    (index, 0.0)
                }
            })
            .collect()
    } else {
        // distribute from gaps
        let mut pos = start;
        groups
            .iter()
            .enumerate()
            .map(|(index, (_, b))| {
                let translation = pos - b[0];
                pos += step;
                pos += b[3];
                (index, translation)
            })
            .collect()
    };
    for (index, translation) in moves {
        let (dx, dy) = match axis {
            Axis::X => (translation, 0.0),
            Axis::Y => (0.0, translation),
        };
        let group = groups[index].0.clone();
        w.translate(&group, dx, dy);
    }
    captured(w.with_frame_membership(app_state))
}
