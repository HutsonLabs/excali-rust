//! The linear element editor on the canvas (`LinearElementEditor`,
//! `packages/element/src/linearElementEditor.ts`, as `App` drives it):
//! the selected line or arrow's point handles and segment midpoints, and
//! the editor a double-click (a line) or Ctrl/Cmd+double-click (an arrow)
//! opens.
//!
//! - A press on a point handle selects the point (Shift adds it) and a
//!   drag moves the selected points (`handlePointerDown`,
//!   `handlePointDragging`), along the nearest 15 degree line with Shift;
//!   the release closes a line whose end was dragged onto its other end
//!   (`handlePointerUp`) and binds a dragged arrow end (`actionFinalize`
//!   with the release).
//! - A drag from a segment's midpoint handle inserts a point there
//!   (`shouldAddMidpoint`, `addMidpoint`); outside the editor only past
//!   the drag threshold. On an elbow arrow it fixes the segment where the
//!   pointer is (`moveFixedSegment`), and a double-click on a fixed
//!   segment frees it (`deleteFixedSegment`).
//! - In the editor, a press with Alt adds the pointer as the last point.
//!
//! `appState.selectedLinearElement` holds `elementId`, `isEditing`,
//! `selectedPointsIndices`, `hoverPointIndex`,
//! `segmentMidPointHoveredCoords` and `isDragging`; what upstream keeps in
//! `initialState` for the press lives in [`LinearPress`].
//!
//! Reduced from upstream: while a point is dragged the arrow's ends are
//! not bound live (`pointDraggingUpdates`); they bind on the release. A
//! press on an arrow's label does not drag the label along the arrow
//! (`arrowText.maybeDragLabel`), and the focus point handles of bound
//! arrow ends (`handleFocusPointPointerDown`) are not offered.

use excali_core::element::{Element, ElementKind};
use excali_editor::binding::{bind_or_unbind_binding_element, BindingOpts};
use excali_editor::linear_element_editor::{
    add_midpoint, create_point_at, delete_fixed_segment, get_point_index_under_cursor,
    get_points_global_coordinates, get_segment_mid_point_index, get_segment_midpoint_hit_coords,
    move_fixed_segment, move_points, should_add_midpoint, PointUpdate, SegmentMidpoint,
};
use excali_editor::mutate::bump_version;
use excali_editor::new_element::get_locked_linear_cursor_align_size_with_angle;
use excali_editor::scene::{ElementUpdate, Scene};
use excali_editor::transform::get_grid_point;
use excali_math::{js, point_from, point_rotate_rads, GlobalPoint, Radians};
use excali_scene::bounds::get_element_absolute_coords;
use excali_scene::utils::is_path_a_loop;
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{json, Value};

use crate::editor::{Editor, PointerInput};

/// `appState.selectedLinearElement`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LinearState {
    pub(crate) element_id: String,
    pub(crate) is_editing: bool,
    pub(crate) selected_points: Option<Vec<usize>>,
    pub(crate) hover_point_index: isize,
    pub(crate) segment_mid_point_hovered_coords: Option<[f64; 2]>,
    pub(crate) is_dragging: bool,
}

impl LinearState {
    /// `new LinearElementEditor(element, elementsMap, isEditing)`.
    pub(crate) fn new(element_id: &str, is_editing: bool) -> LinearState {
        LinearState {
            element_id: element_id.to_owned(),
            is_editing,
            hover_point_index: -1,
            ..LinearState::default()
        }
    }

    fn from_json(value: &Value) -> Option<LinearState> {
        let point = |v: &Value| Some([v.get(0)?.as_f64()?, v.get(1)?.as_f64()?]);
        Some(LinearState {
            element_id: value.get("elementId")?.as_str()?.to_owned(),
            is_editing: value
                .get("isEditing")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            selected_points: value
                .get("selectedPointsIndices")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_u64)
                        .map(|i| i as usize)
                        .collect()
                }),
            hover_point_index: value
                .get("hoverPointIndex")
                .and_then(Value::as_i64)
                .map_or(-1, |i| i as isize),
            segment_mid_point_hovered_coords: value
                .get("segmentMidPointHoveredCoords")
                .and_then(point),
            is_dragging: value
                .get("isDragging")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "elementId": self.element_id,
            "isEditing": self.is_editing,
            "selectedPointsIndices": self.selected_points,
            "hoverPointIndex": self.hover_point_index,
            "segmentMidPointHoveredCoords": self.segment_mid_point_hovered_coords,
            "isDragging": self.is_dragging,
        })
    }
}

/// A press on the selected linear element (`initialState` and
/// `pointerOffset`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LinearPress {
    pub(crate) id: String,
    /// `initialState.lastClickedPoint`: the point pressed, or -1.
    pub(crate) last_clicked: isize,
    pub(crate) prev_selected: Option<Vec<usize>>,
    pub(crate) origin: [f64; 2],
    pub(crate) pointer_offset: [f64; 2],
    pub(crate) midpoint: SegmentMidpoint,
    /// `hitElement`: a point or a midpoint was pressed.
    pub(crate) hit: bool,
    /// The drag's `customLineAngle`.
    pub(crate) custom_line_angle: Option<f64>,
}

/// `normalizeSelectedPoints(points)`: unique, sorted, without -1; `None`
/// when empty.
fn normalize_selected_points(points: &[isize]) -> Option<Vec<usize>> {
    let mut next: Vec<usize> = points
        .iter()
        .filter(|&&p| p >= 0)
        .map(|&p| p as usize)
        .collect();
    next.sort_unstable();
    next.dedup();
    (!next.is_empty()).then_some(next)
}

fn is_elbow_arrow(e: &Element) -> bool {
    matches!(&e.kind, ElementKind::Arrow(a) if a.elbowed)
}

fn is_line(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Line(_))
}

fn rotate(p: [f64; 2], c: [f64; 2], angle: f64) -> [f64; 2] {
    let r: GlobalPoint = point_rotate_rads(
        point_from(p[0], p[1]),
        point_from(c[0], c[1]),
        Radians(angle),
    );
    [r.x, r.y]
}

impl<P: TextMetricsProvider + Clone> Editor<P> {
    /// `appState.selectedLinearElement`, when its element is in the scene.
    pub(crate) fn linear_state(&self) -> Option<LinearState> {
        let state = LinearState::from_json(self.session.app_state().get("selectedLinearElement")?)?;
        self.linear_element(&state.element_id)?;
        Some(state)
    }

    pub(crate) fn set_linear_state(&mut self, state: Option<&LinearState>) {
        let value = state.map_or(Value::Null, LinearState::to_json);
        self.set_keys(vec![("selectedLinearElement", value)]);
    }

    fn linear_element(&self, id: &str) -> Option<Element> {
        self.session
            .elements()
            .iter()
            .find(|e| e.base.id == id && !e.base.is_deleted && e.kind.linear().is_some())
            .cloned()
    }

    /// `LinearElementEditor.handlePointerDown` (`linearElementEditor.ts:
    /// 1048-1243`) for a press at `origin`: the press, and whether Alt
    /// added a point (which ends the press's handling).
    pub(crate) fn linear_pointer_down(
        &mut self,
        input: PointerInput,
        origin: [f64; 2],
    ) -> Option<(LinearPress, bool)> {
        let mut state = self.linear_state()?;
        let element = self.linear_element(&state.element_id)?;
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        let scene = Scene::new(self.session.elements().to_vec());
        let map = scene.elements_map();
        let midpoint = get_segment_midpoint_hit_coords(
            &element,
            state.segment_mid_point_hovered_coords,
            origin,
            zoom,
            state.is_editing,
            &map,
        );
        let midpoint_index = midpoint
            .map(|m| get_segment_mid_point_index(&element, state.is_editing, zoom, m, &map));
        let segment = SegmentMidpoint {
            value: midpoint,
            index: midpoint_index.filter(|&i| i >= 0).map(|i| i as usize),
            added: false,
        };
        if midpoint.is_none() && input.alt_key && state.is_editing {
            // the pointer becomes the last point
            let point = create_point_at(
                &element,
                &map,
                origin[0],
                origin[1],
                self.grid_size(input.ctrl_or_cmd),
            );
            drop(map);
            let mut scene = scene;
            let mut next = element.kind.points().map(<[_]>::to_vec).unwrap_or_default();
            next.push(point);
            let last = next.len() - 1;
            scene.mutate_element(
                &element.base.id,
                ElementUpdate {
                    points: Some(next),
                    ..ElementUpdate::default()
                },
                &mut self.session.env,
            );
            let app_state = self.session.app_state().clone();
            self.apply(scene, app_state);
            self.session.store.schedule_capture();
            state.selected_points = Some(vec![last]);
            self.set_linear_state(Some(&state));
            let press = LinearPress {
                id: element.base.id.clone(),
                last_clicked: -1,
                prev_selected: None,
                origin,
                pointer_offset: [0.0, 0.0],
                midpoint: segment,
                hit: false,
                custom_line_angle: None,
            };
            return Some((press, true));
        }
        let clicked = get_point_index_under_cursor(&element, &map, zoom, origin[0], origin[1]);
        let hit = clicked >= 0 || midpoint.is_some();
        let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(&element, &map, false);
        let center = [(x1 + x2) / 2.0, (y1 + y2) / 2.0];
        let pts = element.kind.points().unwrap_or(&[]);
        let target = (clicked > -1).then(|| {
            let p = pts[clicked as usize];
            rotate(
                [element.base.x + p[0], element.base.y + p[1]],
                center,
                element.base.angle.0,
            )
        });
        let prev = state.selected_points.clone();
        let next_selected = if clicked > -1 || input.shift_key {
            if input.shift_key
                || prev
                    .as_ref()
                    .is_some_and(|s| clicked >= 0 && s.contains(&(clicked as usize)))
            {
                let mut all: Vec<isize> = prev
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|i| i as isize)
                    .collect();
                all.push(clicked);
                normalize_selected_points(&all)
            } else {
                Some(vec![clicked as usize])
            }
        } else {
            None
        };
        state.selected_points = next_selected;
        drop(map);
        self.set_linear_state(Some(&state));
        Some((
            LinearPress {
                id: element.base.id.clone(),
                last_clicked: clicked,
                prev_selected: prev,
                origin,
                pointer_offset: target.map_or([0.0, 0.0], |t| [origin[0] - t[0], origin[1] - t[1]]),
                midpoint: segment,
                hit,
                custom_line_angle: None,
            },
            false,
        ))
    }

    /// A move while a press on the selected linear element lasts
    /// (`App.tsx:10716-10985`): an elbow arrow's segment follows the
    /// pointer; a midpoint dragged far enough becomes a point; a pressed
    /// point is dragged. Returns whether the move was the editor's (else
    /// the element itself is dragged).
    pub(crate) fn linear_drag(&mut self, press: &mut LinearPress, input: PointerInput) -> bool {
        let pointer = self.scene_point(input.client_x, input.client_y);
        let Some(mut state) = self.linear_state() else {
            return false;
        };
        let Some(element) = self.linear_element(&press.id) else {
            return false;
        };
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        // an elbow arrow's segment (App.tsx:10716-10772)
        if is_elbow_arrow(&element) {
            if let Some(index) = press.midpoint.index {
                let [gx, gy] =
                    get_grid_point(pointer[0], pointer[1], self.grid_size(input.ctrl_or_cmd));
                let mut scene = Scene::new(self.session.elements().to_vec());
                let moved =
                    move_fixed_segment(&mut scene, &mut self.session.env, &press.id, index, gx, gy);
                let app_state = self.session.app_state().clone();
                self.apply(scene, app_state);
                if let Some((point, index)) = moved {
                    state.segment_mid_point_hovered_coords = Some(point);
                    press.midpoint = SegmentMidpoint {
                        value: Some(point),
                        index: Some(index),
                        added: false,
                    };
                }
                state.is_dragging = true;
                self.set_linear_state(Some(&state));
                return true;
            }
        }
        if should_add_midpoint(
            &element,
            &press.midpoint,
            Some(press.origin),
            pointer,
            state.is_editing,
            zoom,
        ) {
            let Some(index) = press.midpoint.index else {
                return true;
            };
            let mut scene = Scene::new(self.session.elements().to_vec());
            let grid = (!input.ctrl_or_cmd)
                .then(|| self.grid_size(false))
                .flatten();
            let Some(added) = add_midpoint(
                &mut scene,
                &mut self.session.env,
                &press.id,
                index,
                pointer,
                grid,
            ) else {
                return true;
            };
            let app_state = self.session.app_state().clone();
            self.apply(scene, app_state);
            press.last_clicked = added.last_clicked_point as isize;
            press.midpoint.added = true;
            state.selected_points = Some(added.selected_points_indices);
            state.segment_mid_point_hovered_coords = None;
            state.is_dragging = true;
            self.set_linear_state(Some(&state));
            return true;
        }
        if press.midpoint.value.is_some() && !press.midpoint.added {
            return true;
        }
        if press.last_clicked < 0 {
            return false;
        }
        self.drag_points(press, &mut state, &element, pointer, input);
        true
    }

    /// `LinearElementEditor.handlePointDragging` (`linearElementEditor.ts:
    /// 471-719`) without live binding.
    fn drag_points(
        &mut self,
        press: &mut LinearPress,
        state: &mut LinearState,
        element: &Element,
        pointer: [f64; 2],
        input: PointerInput,
    ) {
        let pts = element.kind.points().map(<[_]>::to_vec).unwrap_or_default();
        if pts.len() < 2 {
            return;
        }
        let mut selected = state.selected_points.clone().unwrap_or_default();
        let mut last_clicked = press.last_clicked;
        let elbowed = is_elbow_arrow(element);
        if elbowed {
            if let Some(slot) = selected
                .iter_mut()
                .find(|i| **i > 0 && **i != pts.len() - 1)
            {
                *slot = pts.len() - 1;
                last_clicked = pts.len() as isize - 1;
            }
        }
        if last_clicked < 0
            || !selected.contains(&(last_clicked as usize))
            || last_clicked as usize >= pts.len()
        {
            last_clicked = pts.len() as isize - 1;
        }
        let clicked = last_clicked as usize;
        let dragging_point = pts[clicked];
        let pivot = pts[if clicked == 0 { 1 } else { clicked - 1 }];
        let single = selected.len() == 1;
        let custom_angle = *press.custom_line_angle.get_or_insert_with(|| {
            js::atan2(dragging_point[1] - pivot[1], dragging_point[0] - pivot[0])
        });
        let scene = Scene::new(self.session.elements().to_vec());
        let map = scene.elements_map();
        let grid = self.grid_size(input.ctrl_or_cmd);
        let (dx, dy) = if input.shift_key && single {
            // _getShiftLockedDelta (linearElementEditor.ts:1895-1935)
            let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(element, &map, false);
            let reference = rotate(
                [element.base.x + pivot[0], element.base.y + pivot[1]],
                [(x1 + x2) / 2.0, (y1 + y2) / 2.0],
                element.base.angle.0,
            );
            let (w, h) = if elbowed {
                (pointer[0] - reference[0], pointer[1] - reference[1])
            } else {
                let [gx, gy] = get_grid_point(pointer[0], pointer[1], grid);
                let (w, h) = get_locked_linear_cursor_align_size_with_angle(
                    reference[0],
                    reference[1],
                    gx,
                    gy,
                    Some(custom_angle),
                );
                let r = rotate([w, h], [0.0, 0.0], -element.base.angle.0);
                (r[0], r[1])
            };
            (
                w + pivot[0] - dragging_point[0],
                h + pivot[1] - dragging_point[1],
            )
        } else {
            let next = create_point_at(
                element,
                &map,
                pointer[0] - press.pointer_offset[0],
                pointer[1] - press.pointer_offset[1],
                grid,
            );
            (next[0] - dragging_point[0], next[1] - dragging_point[1])
        };
        drop(map);
        let updates: Vec<(usize, PointUpdate)> = selected
            .iter()
            .map(|&i| {
                let p = pts.get(i).copied().unwrap_or(pts[pts.len() - 1]);
                (
                    i,
                    PointUpdate {
                        point: [p[0] + dx, p[1] + dy],
                        is_dragging: true,
                    },
                )
            })
            .collect();
        // the binding the dragged end suggests, before it moves
        // (pointDraggingUpdates)
        let start = selected.contains(&0);
        let end = selected.contains(&(pts.len() - 1));
        if start != end {
            let index = if start { 0 } else { pts.len() - 1 };
            let p = pts[index];
            self.suggest_binding(
                &press.id,
                index,
                [p[0] + dx, p[1] + dy],
                pointer,
                press.origin,
                false,
                input.alt_key,
            );
        }
        let mut scene = scene;
        move_points(
            &mut scene,
            &mut self.session.env,
            &press.id,
            &updates,
            Default::default(),
        );
        let app_state = self.session.app_state().clone();
        self.apply(scene, app_state);
        let after = self
            .linear_element(&press.id)
            .and_then(|e| e.kind.points().map(<[_]>::len))
            .unwrap_or(pts.len());
        if elbowed {
            selected = vec![if end { after - 1 } else { 0 }];
            last_clicked = selected[0] as isize;
        }
        press.last_clicked = last_clicked;
        state.segment_mid_point_hovered_coords = if !start && !end {
            let moved = self.linear_element(&press.id);
            moved.map(|m| {
                let s = Scene::new(self.session.elements().to_vec());
                let map = s.elements_map();
                get_points_global_coordinates(&m, &map)
                    .get(clicked)
                    .copied()
                    .unwrap_or([0.0, 0.0])
            })
        } else {
            None
        };
        state.selected_points = Some(selected);
        state.hover_point_index = last_clicked;
        state.is_dragging = true;
        self.set_linear_state(Some(state));
    }

    /// The release of a press on the selected linear element: a dragged
    /// end dropped on the other end closes a line (`handlePointerUp`,
    /// `linearElementEditor.ts:719-809`), a dragged arrow end binds where
    /// it was dropped (`actionFinalize` with the release), and the point
    /// selection settles.
    pub(crate) fn linear_pointer_up(&mut self, press: &LinearPress, input: PointerInput) {
        let Some(mut state) = self.linear_state() else {
            return;
        };
        let Some(element) = self.linear_element(&press.id) else {
            return;
        };
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        let pointer = self.scene_point(input.client_x, input.client_y);
        let mut scene = Scene::new(self.session.elements().to_vec());
        let pts = element.kind.points().map(<[_]>::to_vec).unwrap_or_default();
        let selected = state.selected_points.clone();
        if state.is_dragging {
            for &p in selected.iter().flatten() {
                if (p == 0 || p + 1 == pts.len()) && is_path_a_loop(&pts, zoom) {
                    if is_line(&element) {
                        let mut closed = element.clone();
                        if let ElementKind::Line(l) = &mut closed.kind {
                            if !l.polygon {
                                l.polygon = true;
                                bump_version(&mut closed, None, &mut self.session.env);
                                scene.replace_element(closed);
                            }
                        }
                    }
                    let other = if p == 0 { pts[pts.len() - 1] } else { pts[0] };
                    move_points(
                        &mut scene,
                        &mut self.session.env,
                        &press.id,
                        &[(p, PointUpdate::to(other))],
                        Default::default(),
                    );
                }
            }
            // a dragged arrow end binds where it was dropped
            if matches!(element.kind, ElementKind::Arrow(_)) && press.midpoint.value.is_none() {
                let current = scene.get(&press.id).cloned();
                if let (Some(arrow), Some(sel)) = (current, selected.as_ref()) {
                    let map = scene.elements_map();
                    let dragged: Vec<(usize, [f64; 2])> = sel
                        .iter()
                        .map(|&i| {
                            let point = if input.shift_key {
                                arrow.kind.points().and_then(|p| p.get(i).copied())
                            } else {
                                Some(create_point_at(
                                    &arrow,
                                    &map,
                                    pointer[0] - press.pointer_offset[0],
                                    pointer[1] - press.pointer_offset[1],
                                    self.grid_size(input.ctrl_or_cmd),
                                ))
                            };
                            (i, point.unwrap_or([0.0, 0.0]))
                        })
                        .collect();
                    drop(map);
                    let binding = self.binding_app_state(press.origin, input.alt_key);
                    let grid_size = self.grid_size(false);
                    let _ = bind_or_unbind_binding_element(
                        &mut scene,
                        &mut self.session.env,
                        &press.id,
                        &dragged,
                        pointer,
                        &binding,
                        &BindingOpts {
                            alt_key: input.alt_key,
                            angle_locked: input.shift_key,
                            grid_size,
                            ..BindingOpts::default()
                        },
                    );
                }
            }
        }
        let app_state = self.session.app_state().clone();
        self.apply(scene, app_state);
        // selectedPointsIndices (linearElementEditor.ts:777-797)
        let clicked = press.last_clicked;
        state.selected_points = if state.is_dragging || input.shift_key {
            if !state.is_dragging
                && input.shift_key
                && press
                    .prev_selected
                    .as_ref()
                    .is_some_and(|p| clicked >= 0 && p.contains(&(clicked as usize)))
            {
                selected.map(|s| s.into_iter().filter(|&i| i as isize != clicked).collect())
            } else {
                selected
            }
        } else if clicked >= 0
            && selected
                .as_ref()
                .is_some_and(|s| s.contains(&(clicked as usize)))
        {
            Some(vec![clicked as usize])
        } else {
            selected
        };
        state.is_dragging = false;
        state.hover_point_index = -1;
        state.segment_mid_point_hovered_coords = None;
        self.set_linear_state(Some(&state));
        self.session.store.schedule_capture();
        self.session.commit();
        self.report();
    }

    /// `actionToggleLinearEditor` (`actionLinearEditor.tsx:28-80`) for the
    /// one selected line or arrow (not an elbow arrow): the editor opens or
    /// closes; captured.
    pub(crate) fn toggle_linear_editor(&mut self) {
        let selected = self.selected_ids();
        let [id] = selected.as_slice() else {
            return;
        };
        let Some(element) = self.linear_element(id) else {
            return;
        };
        if is_elbow_arrow(&element) {
            return;
        }
        let mut state = self
            .linear_state()
            .filter(|s| &s.element_id == id)
            .unwrap_or_else(|| LinearState::new(id, false));
        if state.is_editing {
            return;
        }
        state.is_editing = true;
        self.set_linear_state(Some(&state));
        self.session.store.schedule_capture();
        self.session.commit();
        self.report();
    }

    /// The double-click's linear element branch (`App.tsx:7379-7462`):
    /// a line, or with Ctrl/Cmd a simple arrow, opens its editor; on an
    /// elbow arrow a fixed segment under the pointer is freed. Returns
    /// whether the double-click was handled.
    pub(crate) fn linear_double_click(&mut self, input: PointerInput, point: [f64; 2]) -> bool {
        let selected = self.selected_ids();
        let [id] = selected.as_slice() else {
            return false;
        };
        let Some(element) = self.linear_element(id) else {
            return false;
        };
        let state = self.linear_state();
        let editing_this = state
            .as_ref()
            .is_some_and(|s| s.is_editing && &s.element_id == id);
        let simple_arrow = matches!(&element.kind, ElementKind::Arrow(a) if !a.elbowed);
        if ((input.ctrl_or_cmd && simple_arrow) || is_line(&element)) && !editing_this {
            if self.linear_state().is_none_or(|s| &s.element_id != id) {
                self.set_linear_state(Some(&LinearState::new(id, false)));
            }
            self.toggle_linear_editor();
            return true;
        }
        if let (Some(mut state), true) = (state, is_elbow_arrow(&element)) {
            let zoom = self.session.app_state().zoom().unwrap_or(1.0);
            let mut scene = Scene::new(self.session.elements().to_vec());
            let index = {
                let map = scene.elements_map();
                get_segment_midpoint_hit_coords(
                    &element,
                    state.segment_mid_point_hovered_coords,
                    point,
                    zoom,
                    state.is_editing,
                    &map,
                )
                .map(|m| get_segment_mid_point_index(&element, state.is_editing, zoom, m, &map))
            };
            if let Some(index) = index.filter(|&i| i > 0) {
                self.session.store.schedule_capture();
                delete_fixed_segment(&mut scene, &mut self.session.env, id, index as usize);
                let app_state = self.session.app_state().clone();
                self.apply(scene, app_state);
                let moved = self.linear_element(id);
                let next = moved.and_then(|m| {
                    let s = Scene::new(self.session.elements().to_vec());
                    let map = s.elements_map();
                    get_segment_midpoint_hit_coords(&m, None, point, zoom, state.is_editing, &map)
                });
                state.segment_mid_point_hovered_coords = next;
                self.set_linear_state(Some(&state));
                self.session.commit();
                self.report();
                return true;
            }
            return false;
        }
        editing_this && is_line(&element)
    }
}
