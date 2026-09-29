//! What the canvas gestures share beyond one gesture (ex-713): the app
//! state snapping reads, the elements and the frame under the pointer
//! (`App.getElementsAtPosition`, `App.getTopLayerFrameAtSceneCoords`),
//! the hover before a press (`App.handleCanvasPointerMove`: the target
//! frame and the pointer's snap lines), and the frame membership a drag
//! or a resize leaves (`onPointerUpFromPointerDownHandler`).

use std::collections::HashSet;

use excali_core::element::{Element, ElementKind};
use excali_editor::binding::{
    get_binding_strategy_for_dragging_binding_element_endpoints, get_snap_outline_mid_point,
    BindingOpts, BindingStrategy,
};
use excali_editor::collision::{get_hovered_element_for_binding, hit_element};
use excali_editor::linear_element_editor::{
    get_point_index_under_cursor, get_segment_midpoint_hit_coords, is_point_handle,
};
use excali_editor::frame::{
    add_elements_to_frame, get_common_frame_id, get_elements_in_resizing_frame, is_cursor_in_frame,
    is_in_frame, replace_all_elements_in_frame, update_frame_membership_of_selected_elements,
    MembershipState,
};
use excali_editor::snapping::{
    get_snap_lines_at_pointer, is_active_tool_non_linear_snappable, SnapAppState, SnapEvent,
    SnapLine,
};
use excali_editor::tools::{Tool, ToolType};
use excali_editor::viewport::ViewportState;
use excali_scene::bounds::ElementsMap;
use excali_scene::frame::is_frame_like;
use excali_scene::render_element::get_containing_frame;
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{json, Map, Value};

use crate::editor::{Editor, PointerInput};

/// `isEligibleFrameChildType(type)` (`typeChecks.ts:413-431`).
pub(crate) fn is_eligible_frame_child_type(tool: &str) -> bool {
    matches!(
        tool,
        "rectangle"
            | "stickynote"
            | "diamond"
            | "ellipse"
            | "arrow"
            | "line"
            | "freedraw"
            | "text"
            | "image"
            | "frame"
            | "embeddable"
    )
}

fn is_iframe_like(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Iframe | ElementKind::Embeddable)
}

fn is_bound_text(e: &Element) -> bool {
    matches!(&e.kind, ElementKind::Text(t) if t.container_id.is_some())
}

/// `snapLines` as the app state holds them.
pub(crate) fn snap_lines_json(lines: &[SnapLine]) -> Value {
    Value::Array(lines.iter().map(SnapLine::to_json).collect())
}

impl<P: TextMetricsProvider + Clone> Editor<P> {
    /// Sets the app state keys that changed; returns whether any did.
    pub(crate) fn set_keys(&mut self, keys: Vec<(&str, Value)>) -> bool {
        let current = self.session.app_state().as_map();
        let patch: Map<String, Value> = keys
            .into_iter()
            .filter(|(k, v)| current.get(*k) != Some(v))
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        if patch.is_empty() {
            return false;
        }
        self.session.set_state(patch);
        true
    }

    /// Sets the keys of `patch` that changed.
    pub(crate) fn set_patch(&mut self, patch: Map<String, Value>) -> bool {
        let current = self.session.app_state().as_map();
        let patch: Map<String, Value> = patch
            .into_iter()
            .filter(|(k, v)| current.get(k) != Some(v))
            .collect();
        if patch.is_empty() {
            return false;
        }
        self.session.set_state(patch);
        true
    }

    /// The element `id` as the app state holds one (`frameToHighlight`,
    /// `newElement`), or `null`.
    pub(crate) fn element_value(&self, id: Option<&str>) -> Value {
        id.and_then(|id| {
            self.session
                .elements()
                .iter()
                .find(|e| e.base.id == id && !e.base.is_deleted)
        })
        .map_or(Value::Null, |e| Value::Object(e.to_map()))
    }

    /// The elements `ids` as `elementsToHighlight` holds them; `null` for
    /// none.
    pub(crate) fn elements_value(&self, ids: &[String]) -> Value {
        if ids.is_empty() {
            return Value::Null;
        }
        Value::Array(
            ids.iter()
                .map(|id| self.element_value(Some(id)))
                .filter(|v| !v.is_null())
                .collect(),
        )
    }

    fn app_flag(&self, key: &str) -> bool {
        self.session
            .app_state()
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// `isGridModeEnabled(app)`: `props.gridModeEnabled ??
    /// state.gridModeEnabled`.
    pub(crate) fn grid_mode_enabled(&self) -> bool {
        self.props
            .grid_mode_enabled
            .or_else(|| self.session.app_state().grid_mode_enabled())
            .unwrap_or(false)
    }

    /// The app state snapping reads.
    pub(crate) fn snap_state(&self) -> SnapAppState {
        let app = self.session.app_state();
        SnapAppState {
            viewport: ViewportState::from_app_state(app),
            objects_snap_mode_enabled: self
                .props
                .objects_snap_mode_enabled
                .unwrap_or_else(|| self.app_flag("objectsSnapModeEnabled")),
            grid_mode_enabled: self.grid_mode_enabled(),
            active_tool: match &self.tools.active_tool.tool {
                Tool::Builtin(t) => *t,
                // a custom tool is none of the tools snapping tests for
                Tool::Custom(_) => ToolType::Hand,
            },
            selected_elements_are_being_dragged: self.app_flag("selectedElementsAreBeingDragged"),
        }
    }

    /// `App.getElementsAtPosition(x, y, { includeLockedElements })`
    /// (`App.tsx:6867-6927`): the elements hit at `point` (bound text left
    /// out), bottom to top, a frame's children only inside their frame,
    /// iframes and embeddables last.
    pub(crate) fn elements_at(&mut self, point: [f64; 2], include_locked: bool) -> Vec<String> {
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        let selected: HashSet<String> = self.selected_ids().into_iter().collect();
        let elements = self.session.elements();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        let mut hits: Vec<&Element> = Vec::new();
        let mut iframes: Vec<&Element> = Vec::new();
        for e in &live {
            if (!include_locked && e.base.locked) || is_bound_text(e) {
                continue;
            }
            let boxed = selected.contains(&e.base.id) && Self::has_bounding_box(&[e]);
            if !hit_element(&mut self.hit_cache, point, e, &map, zoom, true, boxed, None) {
                continue;
            }
            // a frame's child is not hit from outside the frame
            if let Some(frame) = get_containing_frame(e, &map) {
                if !is_iframe_like(e) && !is_cursor_in_frame(point, frame, &map) {
                    continue;
                }
            }
            if is_iframe_like(e) {
                iframes.push(e);
            } else {
                hits.push(e);
            }
        }
        hits.extend(iframes);
        hits.into_iter().map(|e| e.base.id.clone()).collect()
    }

    /// `App.getTopLayerFrameAtSceneCoords(sceneCoords, opts)`
    /// (`App.tsx:7811-7885`): the topmost unlocked frame under the point,
    /// unless an element above it (not one of `exclude`) is hit there; a
    /// hit element in the frame `current_frame` or in another frame under
    /// the point gives that frame.
    pub(crate) fn top_layer_frame_at(
        &mut self,
        point: [f64; 2],
        exclude: Option<&HashSet<String>>,
        current_frame: Option<&str>,
    ) -> Option<String> {
        let elements = self.session.elements().to_vec();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        let under: Vec<&Element> = live
            .iter()
            .copied()
            .filter(|f| is_frame_like(f) && !f.base.locked && is_cursor_in_frame(point, f, &map))
            .collect();
        let top = (*under.last()?).clone();
        let hit_id = self
            .elements_at(point, true)
            .into_iter()
            .rev()
            .find(|id| exclude.is_none_or(|ex| !ex.contains(id)));
        let Some(hit_id) = hit_id else {
            return Some(top.base.id.clone());
        };
        let hit = live.iter().find(|e| e.base.id == hit_id)?;
        if is_frame_like(hit) && !hit.base.locked {
            return Some(top.base.id.clone());
        }
        let index = |id: &str| elements.iter().position(|e| e.base.id == id);
        if let (Some(h), Some(t)) = (index(&hit.base.id), index(&top.base.id)) {
            if h <= t {
                return Some(top.base.id.clone());
            }
        }
        if let Some(current) = current_frame {
            if let Some(f) = under.iter().find(|f| f.base.id == current) {
                return Some(f.base.id.clone());
            }
        }
        let frame = hit.base.frame_id.as_deref()?;
        under
            .iter()
            .find(|f| f.base.id == frame)
            .map(|f| f.base.id.clone())
    }

    /// A move with no button down (`App.handleCanvasPointerMove`): the
    /// frame a creation tool would draw into
    /// (`maybeUpdateFrameToHighlightOnPointerMove`, `App.tsx:7897-7920`),
    /// and for the shape tools the snap lines at the pointer and the
    /// offset that snaps the next press (`getSnapLinesAtPointer`,
    /// `:8035-8080`).
    pub(crate) fn hover(&mut self, input: PointerInput) {
        let point = self.scene_point(input.client_x, input.client_y);
        let tool = self.tools.active_tool.tool.type_name().to_owned();
        let busy = ["newElement", "multiElement", "selectionElement"]
            .iter()
            .any(|k| !matches!(self.session.app_state().get(*k), None | Some(Value::Null)))
            || self.app_flag("selectedElementsAreBeingDragged");
        let mut keys = Vec::new();
        if !busy {
            let frame = if is_eligible_frame_child_type(&tool) {
                self.top_layer_frame_at(point, None, None)
            } else {
                None
            };
            let frame = self.element_value(frame.as_deref());
            keys.push(("frameToHighlight", frame));
        }
        let new_element = !matches!(
            self.session.app_state().get("newElement"),
            None | Some(Value::Null)
        );
        let tool_type = self.snap_state().active_tool;
        if !new_element && is_active_tool_non_linear_snappable(tool_type) {
            let elements = self.session.elements();
            let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
            let map = ElementsMap::new(live.iter().copied());
            let snapped = get_snap_lines_at_pointer(
                &live,
                &self.snap_state(),
                point,
                Some(SnapEvent {
                    ctrl_or_cmd: input.ctrl_or_cmd,
                }),
                &map,
            );
            keys.push(("snapLines", snap_lines_json(&snapped.snap_lines)));
            keys.push((
                "originSnapOffset",
                json!({ "x": snapped.origin_offset[0], "y": snapped.origin_offset[1] }),
            ));
        } else if !new_element
            && !self.app_flag("selectedElementsAreBeingDragged")
            && matches!(
                self.session.app_state().get("selectionElement"),
                None | Some(Value::Null)
            )
        {
            keys.push(("snapLines", json!([])));
        }
        let changed = self.set_keys(keys);
        let hovered_linear = self.hover_linear(point);
        let hovered_binding = self.hover_binding(point, &tool);
        if changed || hovered_linear || hovered_binding {
            self.session.commit();
        }
    }

    /// The selected line or arrow under a still pointer
    /// (`App.tsx:8557-8645`): the point handle it is over
    /// (`hoverPointIndex`) and else the segment midpoint
    /// (`segmentMidPointHoveredCoords`), which the interactive canvas
    /// highlights. Returns whether they changed.
    fn hover_linear(&mut self, point: [f64; 2]) -> bool {
        let Some(mut state) = self.linear_state() else {
            return false;
        };
        let Some(element) = self
            .session
            .elements()
            .iter()
            .find(|e| e.base.id == state.element_id)
            .cloned()
        else {
            return false;
        };
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        let elements = self.session.elements();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        let index = get_point_index_under_cursor(&element, &map, zoom, point[0], point[1]);
        let on_handle = is_point_handle(&element, index);
        let midpoint = if on_handle {
            None
        } else {
            get_segment_midpoint_hit_coords(
                &element,
                state.segment_mid_point_hovered_coords,
                point,
                zoom,
                state.is_editing,
                &map,
            )
        };
        if state.hover_point_index == index && state.segment_mid_point_hovered_coords == midpoint {
            return false;
        }
        state.hover_point_index = index;
        state.segment_mid_point_hovered_coords = midpoint;
        self.set_linear_state(Some(&state));
        true
    }

    /// Hovering with the arrow tool (`App.tsx:8110-8147`): the element an
    /// arrow started here would bind to is suggested. Returns whether the
    /// suggestion changed.
    fn hover_binding(&mut self, point: [f64; 2], tool: &str) -> bool {
        let app = self.session.app_state();
        let new_element = !matches!(app.get("newElement"), None | Some(Value::Null));
        let enabled = app
            .get("isBindingEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if tool != "arrow" || new_element || !enabled {
            return false;
        }
        let zoom = app.zoom().unwrap_or(1.0);
        let elbowed = app.get("currentItemArrowType").and_then(Value::as_str) == Some("elbow");
        let snapping = app
            .get("isMidpointSnappingEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let elements = self.session.elements();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        let value = match get_hovered_element_for_binding(point, &live, &map, zoom) {
            Some(hovered) => {
                let mid = snapping
                    .then(|| get_snap_outline_mid_point(point, hovered, &map, zoom, elbowed))
                    .flatten();
                json!({ "element": Value::Object(hovered.to_map()), "midPoint": mid })
            }
            None => Value::Null,
        };
        self.set_keys(vec![("suggestedBinding", value)])
    }

    /// `originSnapOffset`, when set.
    pub(crate) fn origin_snap_offset(&self) -> Option<[f64; 2]> {
        let o = self.session.app_state().get("originSnapOffset")?;
        Some([o.get("x")?.as_f64()?, o.get("y")?.as_f64()?])
    }

    /// What every release resets (`App.tsx:11546-11565`, `:11617-11620`):
    /// the resize and rotate flags, the box, the highlights, the snap
    /// lines and the snap cache.
    pub(crate) fn reset_after_release(&mut self) {
        self.snap_cache.destroy();
        self.set_keys(vec![
            ("isResizing", json!(false)),
            ("isRotating", json!(false)),
            ("selectionElement", Value::Null),
            ("frameToHighlight", Value::Null),
            ("elementsToHighlight", Value::Null),
            ("snapLines", json!([])),
            ("originSnapOffset", Value::Null),
            ("selectedElementsAreBeingDragged", json!(false)),
            ("suggestedBinding", Value::Null),
        ]);
    }

    /// `appState.suggestedBinding` while an arrow's end is dragged to
    /// `point` (local) with the pointer at `pointer`
    /// (`pointDraggingUpdates`, `linearElementEditor.ts:2386-2600`): the
    /// element the end would bind to, with the side midpoint it snaps to
    /// when midpoint snapping is on; `null` when it would unbind, as it was
    /// when the binding stays.
    pub(crate) fn suggest_binding(
        &mut self,
        arrow_id: &str,
        index: usize,
        point: [f64; 2],
        pointer: [f64; 2],
        origin: [f64; 2],
        new_arrow: bool,
        alt: bool,
    ) {
        let enabled = self
            .session
            .app_state()
            .get("isBindingEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let elements = self.session.elements().to_vec();
        let Some(arrow) = elements
            .iter()
            .find(|e| e.base.id == arrow_id && matches!(e.kind, ElementKind::Arrow(_)))
        else {
            return;
        };
        let last = arrow.kind.points().map_or(0, <[_]>::len).saturating_sub(1);
        if !enabled || (index != 0 && index != last) {
            return;
        }
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        let state = self.binding_app_state(origin, alt);
        let Ok((start, end)) = get_binding_strategy_for_dragging_binding_element_endpoints(
            arrow,
            &[(index, point)],
            pointer,
            &map,
            &live,
            &state,
            &BindingOpts {
                new_arrow,
                alt_key: alt,
                ..BindingOpts::default()
            },
        ) else {
            return;
        };
        let strategy = if index == 0 { start } else { end };
        let value = match strategy {
            BindingStrategy::Keep => return,
            BindingStrategy::Unbind => Value::Null,
            BindingStrategy::Bind { element, .. } => {
                let Some(bindable) = map.get(&element) else {
                    return;
                };
                let snapping = self
                    .session
                    .app_state()
                    .get("isMidpointSnappingEnabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let elbowed = matches!(&arrow.kind, ElementKind::Arrow(a) if a.elbowed);
                let zoom = self.session.app_state().zoom().unwrap_or(1.0);
                let mid = snapping
                    .then(|| get_snap_outline_mid_point(pointer, bindable, &map, zoom, elbowed))
                    .flatten();
                json!({
                    "element": Value::Object(bindable.to_map()),
                    "midPoint": mid,
                })
            }
        };
        self.set_keys(vec![("suggestedBinding", value)]);
    }

    /// The frame membership after a drag of the selection
    /// (`App.tsx:12092-12181`), read before the release resets the drag
    /// state: the selection joins the frame under `point` (unless that
    /// frame is selected), elements leaving a group being edited leave the
    /// group, and selected elements no longer in their frame leave it.
    pub(crate) fn frame_membership_after_drag(&mut self, point: [f64; 2]) {
        let membership = MembershipState::from_app_state(self.session.app_state());
        let elements = self.session.elements().to_vec();
        let selected: Vec<usize> = (0..elements.len())
            .filter(|&i| {
                !elements[i].base.is_deleted
                    && membership
                        .selected_element_ids
                        .contains(&elements[i].base.id)
            })
            .collect();
        let selected_refs: Vec<&Element> = selected.iter().map(|&i| &elements[i]).collect();
        let current = get_common_frame_id(&selected_refs);
        let top = self.top_layer_frame_at(
            point,
            Some(&membership.selected_element_ids),
            current.as_deref(),
        );
        let editing = membership.editing_group_id.clone();
        let mut next = elements;
        let mut changed_group: Vec<usize> = Vec::new();
        match &top {
            Some(frame_id) if !membership.selected_element_ids.contains(frame_id) => {
                let to_add: Vec<usize> = selected
                    .iter()
                    .copied()
                    .filter(|&i| is_in_frame(&next, i, &membership))
                    .collect();
                if let Some(frame) = next.iter().find(|e| &e.base.id == frame_id).cloned() {
                    let ids: Vec<String> =
                        to_add.iter().map(|&i| next[i].base.id.clone()).collect();
                    let groups_left = self.leave_editing_group(&mut next, &ids, editing.as_deref());
                    let to_add: Vec<usize> = ids
                        .iter()
                        .filter_map(|id| next.iter().position(|e| &e.base.id == id))
                        .collect();
                    next = add_elements_to_frame(next, &to_add, &frame, &mut self.session.env);
                    if groups_left {
                        self.set_keys(vec![("editingGroupId", Value::Null)]);
                    }
                }
            }
            None if editing.is_some() => {
                changed_group = selected
                    .iter()
                    .copied()
                    .filter(|&i| {
                        next[i].base.frame_id.is_some() && !is_in_frame(&next, i, &membership)
                    })
                    .collect();
            }
            _ => {}
        }
        if !changed_group.is_empty() {
            let ids: Vec<String> = changed_group
                .iter()
                .map(|&i| next[i].base.id.clone())
                .collect();
            if self.leave_editing_group(&mut next, &ids, editing.as_deref()) {
                self.set_keys(vec![("editingGroupId", Value::Null)]);
            }
        }
        update_frame_membership_of_selected_elements(&mut next, &membership, &mut self.session.env);
        if next != self.session.elements() {
            // Ok: addElementsToFrame syncs the indices it moved
            let _ = self.session.replace_all_elements(next);
        }
    }

    /// `updateGroupIdsAfterEditingGroup(elements)` (`App.tsx:12104-12143`):
    /// the elements leave the group being edited (and its inner groups);
    /// then any group left with fewer than two elements is dissolved.
    /// Returns whether anything left.
    fn leave_editing_group(
        &mut self,
        elements: &mut [Element],
        ids: &[String],
        editing: Option<&str>,
    ) -> bool {
        let Some(editing) = editing else {
            return false;
        };
        if ids.is_empty() {
            return false;
        }
        for e in elements.iter_mut() {
            if !ids.contains(&e.base.id) {
                continue;
            }
            // groupIds.slice(0, indexOf(editingGroupId)); -1 keeps all but
            // the last
            let index = e.base.group_ids.iter().position(|g| g == editing);
            let keep = match index {
                Some(i) => i,
                None => e.base.group_ids.len().saturating_sub(1),
            };
            let next: Vec<String> = e.base.group_ids[..keep].to_vec();
            if next != e.base.group_ids {
                e.base.group_ids = next;
                excali_editor::mutate::bump_version(e, None, &mut self.session.env);
            }
        }
        let snapshot: Vec<Element> = elements.to_vec();
        for e in elements.iter_mut() {
            let Some(last) = e.base.group_ids.last() else {
                continue;
            };
            let count = snapshot
                .iter()
                .filter(|o| o.base.group_ids.contains(last))
                .count();
            if count < 2 {
                e.base.group_ids.clear();
                excali_editor::mutate::bump_version(e, None, &mut self.session.env);
            }
        }
        true
    }

    /// The frame membership after a resize (`App.tsx:12200-12226`):
    /// selected elements no longer in their frame leave it, and each
    /// selected frame holds what it now covers
    /// (`getElementsInResizingFrame`).
    pub(crate) fn frame_membership_after_resize(&mut self) {
        let membership = MembershipState::from_app_state(self.session.app_state());
        let mut next = self.session.elements().to_vec();
        update_frame_membership_of_selected_elements(&mut next, &membership, &mut self.session.env);
        let frames: Vec<Element> = next
            .iter()
            .filter(|e| {
                !e.base.is_deleted
                    && is_frame_like(e)
                    && membership.selected_element_ids.contains(&e.base.id)
            })
            .cloned()
            .collect();
        let app_state = self.session.app_state().clone();
        for frame in frames {
            let in_frame = {
                let live: Vec<&Element> = next.iter().filter(|e| !e.base.is_deleted).collect();
                let map = ElementsMap::new(live.iter().copied());
                let all = self.session.elements().to_vec();
                // upstream passes the scene's elements as they were
                // before this release's changes
                let found = get_elements_in_resizing_frame(&all, &frame, &app_state, &map);
                found
                    .into_iter()
                    .filter_map(|i| {
                        let id = &all[i].base.id;
                        next.iter().position(|e| &e.base.id == id)
                    })
                    .collect::<Vec<usize>>()
            };
            next = replace_all_elements_in_frame(next, &in_frame, &frame, &mut self.session.env);
        }
        if next != self.session.elements() {
            let _ = self.session.replace_all_elements(next);
        }
    }

    /// `getElementsInResizingFrame` for the frame `id` over the scene: the
    /// ids `elementsToHighlight` shows while a frame is drawn or resized
    /// (`App.tsx:13574-13588`, `:13800-13817`).
    pub(crate) fn elements_in_resizing_frame(&self, id: &str) -> Vec<String> {
        let elements = self.session.elements();
        let live: Vec<Element> = elements
            .iter()
            .filter(|e| !e.base.is_deleted)
            .cloned()
            .collect();
        let Some(frame) = live.iter().find(|e| e.base.id == id) else {
            return Vec::new();
        };
        let map = ElementsMap::new(live.iter());
        get_elements_in_resizing_frame(&live, frame, self.session.app_state(), &map)
            .into_iter()
            .map(|i| live[i].base.id.clone())
            .collect()
    }
}
