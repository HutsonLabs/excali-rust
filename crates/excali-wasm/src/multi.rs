//! Lines and arrows drawn point by point (`appState.multiElement`), and
//! `actionFinalize` (`packages/excalidraw/actions/actionFinalize.tsx`),
//! which ends them.
//!
//! A click (a press and a release closer than `MINIMUM_ARROW_SIZE`) with
//! the line or arrow tool leaves the element being drawn
//! (`onPointerUpFromPointerDownHandler`, `App.tsx:11819-11848`). Then:
//!
//! - a move with no button down follows the pointer with the last point,
//!   adds a point once the pointer leaves the last committed point's
//!   commit zone (`LINE_CONFIRM_THRESHOLD`), and takes it away again when
//!   the pointer comes back (`App.handleCanvasPointerMove`,
//!   `App.tsx:8150-8270`);
//! - a press finishes the line when it closes a loop, or the arrow when it
//!   is an elbow arrow, lands on the last committed point, or binds its
//!   end outside its start's element (`handleLinearElementOnPointerDown`,
//!   `App.tsx:10211-10331`); otherwise it selects the element and its
//!   release commits the point (`App.tsx:11728-11745`);
//! - Enter or Escape run `actionFinalize`: the point not yet committed is
//!   dropped, a line whose ends meet becomes a polygon, the tool reverts
//!   and the element is selected.
//!
//! Upstream tracks the committed point by identity
//! (`lastCommittedPoint`); a moved point is a new object, so the port
//! keeps the committed point's index, which names the same point. While
//! the pointer moves, upstream's `LinearElementEditor.handlePointerMove`
//! also binds the arrow's end live (`pointDraggingUpdates`); the port
//! binds it when the arrow is finalized.

use excali_core::constants::LINE_CONFIRM_THRESHOLD;
use excali_core::element::{Element, ElementKind};
use excali_editor::binding::{
    bind_or_unbind_binding_element, get_binding_strategy_for_dragging_binding_element_endpoints,
    BindingOpts, BindingStrategy,
};
use excali_editor::collision::is_point_in_element;
use excali_editor::linear_element_editor::{create_point_at, get_point_at_index_global_coordinates};
use excali_editor::mutate::bump_version;
use excali_editor::new_element::get_locked_linear_cursor_align_size;
use excali_editor::scene::{ElementUpdate, Scene};
use excali_editor::store::CaptureUpdateAction;
use excali_editor::transform::get_grid_point;
use excali_math::js;
use excali_scene::new_element_scene::is_invisibly_small_element;
use excali_scene::utils::is_path_a_loop;
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{json, Map, Value};

use crate::editor::{CreateGesture, Editor, Gesture, PointerInput};

/// The element drawn point by point (`multiElement`) and the index of its
/// last committed point (`selectedLinearElement.lastCommittedPoint`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MultiPoint {
    pub(crate) id: String,
    pub(crate) last_committed: Option<usize>,
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    js::hypot(a[0] - b[0], a[1] - b[1])
}

fn is_line(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Line(_))
}

fn is_arrow(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Arrow(_))
}

fn is_elbow_arrow(e: &Element) -> bool {
    matches!(&e.kind, ElementKind::Arrow(a) if a.elbowed)
}

/// `isValidPolygon(points)` (`typeChecks.ts:397-401`).
fn is_valid_polygon(points: &[[f64; 2]]) -> bool {
    points.len() > 3 && points.first() == points.last()
}

impl<P: TextMetricsProvider + Clone> Editor<P> {
    fn multi_element(&self) -> Option<Element> {
        let id = &self.multi.as_ref()?.id;
        // a tool switch clears multiElement (onKeyDown's setActiveTool)
        if matches!(
            self.session.app_state().get("multiElement"),
            None | Some(Value::Null)
        ) {
            return None;
        }
        self.session
            .elements()
            .iter()
            .find(|e| &e.base.id == id && !e.base.is_deleted)
            .cloned()
    }

    /// `multiElement` and `newElement` as the element now is.
    fn sync_multi_keys(&mut self) {
        let id = self.multi.as_ref().map(|m| m.id.clone());
        let value = self.element_value(id.as_deref());
        self.set_keys(vec![("multiElement", value.clone()), ("newElement", value)]);
    }

    /// Where the last point of a line or arrow being drawn goes for the
    /// pointer at `pointer` (`LinearElementEditor.handlePointerMove`,
    /// `linearElementEditor.ts:311-360`): on the grid, or with Shift along
    /// the nearest 15 degree line from the point before.
    pub(crate) fn linear_last_point(
        &self,
        element: &Element,
        pointer: [f64; 2],
        input: PointerInput,
    ) -> Option<[f64; 2]> {
        let points = element.kind.points()?;
        let scene = Scene::new(self.session.elements().to_vec());
        let map = scene.elements_map();
        if input.shift_key {
            let prev = points[points.len().saturating_sub(2)];
            let (w, h) = get_locked_linear_cursor_align_size(
                element.base.x + prev[0],
                element.base.y + prev[1],
                pointer[0],
                pointer[1],
            );
            return Some([prev[0] + w, prev[1] + h]);
        }
        let grid = self.grid_size(input.ctrl_or_cmd);
        Some(create_point_at(element, &map, pointer[0], pointer[1], grid))
    }

    /// The release of a click with the line or arrow tool: the element is
    /// drawn point by point from here (`App.tsx:11819-11848`).
    pub(crate) fn start_multi_point(&mut self, id: &str) {
        self.multi = Some(MultiPoint {
            id: id.to_owned(),
            last_committed: None,
        });
        self.sync_multi_keys();
        self.session.commit();
        self.report();
    }

    /// A move with no button down while drawing point by point
    /// (`App.tsx:8150-8270`).
    pub(crate) fn multi_hover(&mut self, input: PointerInput) {
        let pointer = self.scene_point(input.client_x, input.client_y);
        let Some(element) = self.multi_element() else {
            self.multi = None;
            return;
        };
        let Some(points) = element.kind.points().map(<[_]>::to_vec) else {
            return;
        };
        let Some(multi) = self.multi.clone() else {
            return;
        };
        let local = [pointer[0] - element.base.x, pointer[1] - element.base.y];
        let last = points.len() - 1;
        let mut next = points.clone();
        if multi.last_committed == Some(last) {
            // beyond the commit zone a point is added
            if distance(local, points[last]) < LINE_CONFIRM_THRESHOLD {
                return;
            }
            self.session.store.schedule_capture();
            next.push(local);
        } else if points.len() > 2
            && multi
                .last_committed
                .is_some_and(|c| distance(local, points[c]) < LINE_CONFIRM_THRESHOLD)
        {
            // back in the commit zone the point following the pointer goes
            next.pop();
            if let Some(m) = self.multi.as_mut() {
                m.last_committed = Some(next.len() - 1);
            }
        } else {
            let Some(point) = self.linear_last_point(&element, pointer, input) else {
                return;
            };
            if next[last] == point {
                return;
            }
            next[last] = point;
        }
        let mut scene = Scene::new(self.session.elements().to_vec());
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
        self.sync_multi_keys();
        self.session.commit();
        self.report();
    }

    /// A press with the line or arrow tool while drawing point by point
    /// (`handleLinearElementOnPointerDown`, `App.tsx:10211-10331`).
    pub(crate) fn multi_pointer_down(&mut self, input: PointerInput, tool: &str) {
        let origin = self.scene_point(input.client_x, input.client_y);
        let Some(element) = self.multi_element() else {
            self.multi = None;
            return;
        };
        let Some(points) = element.kind.points().map(<[_]>::to_vec) else {
            return;
        };
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        // a line closing a loop is finished
        if is_line(&element) && is_path_a_loop(&points, zoom) {
            if let Some(m) = self.multi.as_mut() {
                m.last_committed = Some(points.len() - 1);
            }
            self.finalize(None);
            self.gesture = Some(Gesture::Finalized);
            return;
        }
        // an elbow arrow has only its two ends
        if is_elbow_arrow(&element) && points.len() > 1 {
            self.finalize(Some((origin, input)));
            self.gesture = Some(Gesture::Finalized);
            return;
        }
        let local = [origin[0] - element.base.x, origin[1] - element.base.y];
        let in_commit_zone = self
            .multi
            .as_ref()
            .and_then(|m| m.last_committed)
            .is_some_and(|c| distance(local, points[c]) < LINE_CONFIRM_THRESHOLD);
        if self.auto_confirms(&element, &points, origin)
            || (points.len() > 1 && in_commit_zone)
        {
            self.finalize(Some((origin, input)));
            self.gesture = Some(Gesture::Finalized);
            return;
        }
        let mut ids = self
            .session
            .app_state()
            .get("selectedElementIds")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        ids.insert(element.base.id.clone(), Value::Bool(true));
        self.set_keys(vec![("selectedElementIds", Value::Object(ids))]);
        self.session.commit();
        let origin_in_grid = get_grid_point(origin[0], origin[1], self.grid_size(input.ctrl_or_cmd));
        self.gesture = Some(Gesture::Create(CreateGesture {
            id: element.base.id.clone(),
            tool: tool.to_owned(),
            origin,
            origin_in_grid,
            dragged: false,
            multi: true,
        }));
        self.report();
    }

    /// Whether a press at `origin` binds the arrow's end so that it is
    /// finished at once (`App.tsx:10256-10302`): the end orbits an element
    /// the start is not bound to, or both ends bind to one element with
    /// the end on its outline.
    fn auto_confirms(&self, element: &Element, points: &[[f64; 2]], origin: [f64; 2]) -> bool {
        let enabled = self
            .session
            .app_state()
            .get("isBindingEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if !is_arrow(element) || !enabled {
            return false;
        }
        let scene = Scene::new(self.session.elements().to_vec());
        let map = scene.elements_map();
        let elements = scene.non_deleted();
        let last = points.len() - 1;
        let state = self.binding_app_state(origin, false);
        let Ok((start, end)) = get_binding_strategy_for_dragging_binding_element_endpoints(
            element,
            &[(last, points[last])],
            origin,
            &map,
            &elements,
            &state,
            &BindingOpts {
                new_arrow: true,
                ..BindingOpts::default()
            },
        ) else {
            return false;
        };
        let BindingStrategy::Bind {
            mode,
            element: end_id,
            focus_point,
        } = &end
        else {
            return false;
        };
        let end_outside_same = match &start {
            BindingStrategy::Bind { element: s, .. } if s == end_id => map
                .get(end_id)
                .is_some_and(|e| !is_point_in_element(*focus_point, e, &map)),
            _ => false,
        };
        let start_bound = element
            .kind
            .linear()
            .and_then(|l| l.start_binding.as_ref())
            .map(|b| b.element_id.clone());
        let orbit_from_elsewhere = *mode == excali_core::element::BindMode::Orbit
            && start_bound.as_deref() != Some(end_id.as_str());
        orbit_from_elsewhere || end_outside_same
    }

    /// `actionFinalize.perform` (`actionFinalize.tsx:54-419`) for the
    /// element drawn point by point, or for the linear element being
    /// edited. With a press (`data`: its scene point and modifiers) an
    /// arrow's end binds where it was pressed and the element is left as
    /// drawn (`:65-208`); without one (Enter, Escape) the point not yet
    /// committed is dropped, a line whose ends meet is closed into a
    /// polygon, the tool reverts and the element is selected (`:210-419`).
    pub(crate) fn finalize(&mut self, data: Option<([f64; 2], PointerInput)>) {
        let Some(element) = self.multi_element() else {
            self.multi = None;
            return self.finalize_editing();
        };
        let multi = self.multi.take().expect("drawing point by point");
        let id = element.base.id.clone();
        let locked = self.tools.is_tool_locked();
        let mut scene = Scene::new(self.session.elements().to_vec());
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        let mut app_state = self.session.app_state().clone();
        for k in [
            "newElement",
            "multiElement",
            "selectionElement",
            "suggestedBinding",
        ] {
            app_state.insert(k, Value::Null);
        }
        if let Some((point, input)) = data {
            if is_arrow(&element) {
                let map = scene.elements_map();
                let last = element.kind.points().map_or(1, <[_]>::len) - 1;
                let dragged = if input.shift_key {
                    element.kind.points().and_then(|p| p.last().copied())
                } else {
                    Some(create_point_at(
                        &element,
                        &map,
                        point[0],
                        point[1],
                        self.grid_size(input.ctrl_or_cmd),
                    ))
                };
                if let Some(dragged) = dragged {
                    let state = self.binding_app_state(point, input.alt_key);
                    let grid_size = self.grid_size(false);
                    let _ = bind_or_unbind_binding_element(
                        &mut scene,
                        &mut self.session.env,
                        &id,
                        &[(last, dragged)],
                        point,
                        &state,
                        &BindingOpts {
                            new_arrow: true,
                            alt_key: input.alt_key,
                            angle_locked: input.shift_key,
                            grid_size,
                            ..BindingOpts::default()
                        },
                    );
                }
            }
            self.delete_if_invisible(&mut scene, &id);
            app_state.insert(
                "selectedLinearElement",
                if locked {
                    Value::Null
                } else {
                    json!({ "elementId": id, "isEditing": false })
                },
            );
            self.apply(scene, app_state);
            self.session.store.schedule_capture();
            self.session.commit();
            self.report();
            return;
        }

        let mut should_commit = true;
        let points = element.kind.points().map(<[_]>::to_vec).unwrap_or_default();
        if multi.last_committed != Some(points.len().saturating_sub(1)) && !points.is_empty() {
            should_commit = false;
            let mut next = points.clone();
            next.pop();
            scene.mutate_element(
                &id,
                ElementUpdate {
                    points: Some(next),
                    ..ElementUpdate::default()
                },
                &mut self.session.env,
            );
        }
        // the end binds where the arrow now ends
        if let Some(arrow) = scene.get(&id).cloned().filter(is_arrow) {
            let pts = arrow.kind.points().map(<[_]>::to_vec).unwrap_or_default();
            if pts.len() > 1 {
                let map = scene.elements_map();
                let global = get_point_at_index_global_coordinates(&arrow, -1, &map);
                let state = self.binding_app_state(global, false);
                let _ = bind_or_unbind_binding_element(
                    &mut scene,
                    &mut self.session.env,
                    &id,
                    &[(pts.len() - 1, pts[pts.len() - 1])],
                    global,
                    &state,
                    &BindingOpts {
                        new_arrow: true,
                        ..BindingOpts::default()
                    },
                );
            }
        }
        self.delete_if_invisible(&mut scene, &id);
        // a loop is closed onto the first point
        if let Some(line) = scene.get(&id).cloned().filter(is_line) {
            let pts = line.kind.points().map(<[_]>::to_vec).unwrap_or_default();
            if is_path_a_loop(&pts, zoom) {
                let mut closed = pts.clone();
                let n = closed.len();
                closed[n - 1] = closed[0];
                scene.mutate_element(
                    &id,
                    ElementUpdate {
                        points: Some(closed),
                        ..ElementUpdate::default()
                    },
                    &mut self.session.env,
                );
                self.set_line_polygon(&mut scene, &id, true);
            }
            let pts = scene
                .get(&id)
                .and_then(|l| l.kind.points().map(<[_]>::to_vec))
                .unwrap_or_default();
            if !is_valid_polygon(&pts) {
                self.set_line_polygon(&mut scene, &id, false);
            }
        }
        if !locked {
            self.tools.active_tool = self.tools.tool_after_finalize();
            let mut ids = app_state
                .get("selectedElementIds")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            ids.insert(id.clone(), Value::Bool(true));
            app_state.insert("selectedElementIds", Value::Object(ids));
        }
        app_state.insert("frameToHighlight", Value::Null);
        app_state.insert("editingTextElement", Value::Null);
        app_state.insert("activeEmbeddable", Value::Null);
        app_state.insert(
            "selectedLinearElement",
            json!({ "elementId": id, "isEditing": false }),
        );
        if should_commit {
            self.apply(scene, app_state);
            self.session.store.schedule_capture();
            self.session.commit();
        } else {
            let patch: Map<String, Value> = app_state.into_map();
            let _ = self.session.update_scene(
                Some(scene.elements().to_vec()),
                Some(patch),
                Some(CaptureUpdateAction::Never),
            );
        }
        self.report();
    }

    /// `isInvisiblySmallElement`: the element deleted
    /// (`newElementWith(el, { isDeleted: true })`).
    fn delete_if_invisible(&mut self, scene: &mut Scene, id: &str) {
        let Some(mut element) = scene.get(id).cloned() else {
            return;
        };
        if element.kind.points().map_or(0, <[_]>::len) < 2 || is_invisibly_small_element(&element)
        {
            element.base.is_deleted = true;
            bump_version(&mut element, None, &mut self.session.env);
            scene.replace_element(element);
        }
    }

    fn set_line_polygon(&mut self, scene: &mut Scene, id: &str, polygon: bool) {
        let Some(mut element) = scene.get(id).cloned() else {
            return;
        };
        if let ElementKind::Line(line) = &mut element.kind {
            if line.polygon != polygon {
                line.polygon = polygon;
                bump_version(&mut element, None, &mut self.session.env);
                scene.replace_element(element);
            }
        }
    }

    /// `actionFinalize` with no element being drawn: the linear element
    /// editor, if open, closes.
    pub(crate) fn finalize_editing(&mut self) {
        let editing = self
            .session
            .app_state()
            .get("selectedLinearElement")
            .and_then(|l| l.get("isEditing"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !editing {
            return;
        }
        let id = self
            .session
            .app_state()
            .get("selectedLinearElement")
            .and_then(|l| l.get("elementId"))
            .cloned()
            .unwrap_or(Value::Null);
        let mut ids = Map::new();
        if let Some(id) = id.as_str() {
            ids.insert(id.to_owned(), Value::Bool(true));
        }
        self.set_keys(vec![
            (
                "selectedLinearElement",
                json!({ "elementId": id, "isEditing": false }),
            ),
            ("selectedElementIds", Value::Object(ids)),
        ]);
        self.session.store.schedule_capture();
        self.session.commit();
        self.report();
    }
}
