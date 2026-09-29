//! Text editing in the element: the text tool's click (`AppTextTool`,
//! `packages/excalidraw/components/App.textTool.ts`), the canvas's
//! double-click (`App.handleCanvasDoubleClick`, `App.tsx:7340-7562`), a
//! click on a selected text (`App.tsx:12504-12531`), and the open
//! [`TextEditor`] fed with the textarea's events as
//! `excali_ui::text_editor::TextEditingApp` does.
//!
//! The text is started and edited by `excali_editor::text_editing`
//! (`startTextEditing`, `handleTextWysiwyg`, `textWysiwyg`); what it asks
//! of the rest of the app comes from [`EditorHost`]: the text under a
//! point (`getTextElementAtPosition`), the frame on top at a point, and
//! the arrows bound to a container following it as its label grows.
//!
//! A double-click on the selected line (or, with Ctrl/Cmd, arrow) opens
//! its editor and on an elbow arrow's fixed segment frees it
//! (`crate::linear`); on the one selected image it starts cropping
//! (`crate::cropping`).

use excali_core::color::is_transparent;
use excali_core::element::{Element, ElementKind};
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use excali_editor::binding::update_bound_elements_in_map;
use excali_editor::collision::{
    get_element_hit_threshold, hit_element, hit_element_itself, HitTestArgs, HitTestCache,
};
use excali_editor::text_editing::{
    snapped_to_center_position, start_text_editing, AppCall, CaretRequest, KeyDown, PasteOutcome,
    StartTextEditing, TextEditingContext, TextEditingHost, TextEditor, TextTarget,
    TextareaAttributes,
};
use excali_editor::text_layout::TextLayouter;
use excali_scene::bounds::{get_element_absolute_coords, get_element_bounds, ElementsMap};
use excali_scene::frame::is_frame_like;
use excali_text::text_measurements::{CharWidthCache, TextMetricsProvider};
use excali_ui::text_editor::{Handled, TextareaEvent, TextareaState};
use serde_json::{json, Map, Value};

use crate::editor::{Editor, Gesture, PointerInput};
use excali_editor::arrow_endpoint_text::{
    drag_new_text_element, get_endpoint_bound_text_drag_anchor, get_unbound_arrow_endpoint_at_point,
    is_endpoint_bound_text, ArrowEndpoint,
};
use excali_editor::transform::get_grid_point;
use excali_text::font_metadata::get_font_string;
use excali_text::text_measurements::get_min_text_element_width;
use crate::env::{EditorEnv, StampBinding};

/// `TEXT_AUTOWRAP_THRESHOLD` (`common/src/constants.ts:24`): how far a
/// press on an empty container's centre may travel (screen px) and still
/// be a click.
pub const TEXT_AUTOWRAP_THRESHOLD: f64 = 36.0;

/// What text editing asks of the element.
pub struct EditorHost<P> {
    zoom: f64,
    grid_mode_enabled: Option<bool>,
    provider: P,
}

impl<P: TextMetricsProvider> TextEditingHost for EditorHost<P> {
    /// `getTextElementAtPosition(x, y)` (`App.tsx:6741-6752`): the topmost
    /// element hit, bound text included, when it is a text.
    fn text_element_at(&self, elements: &[Element], x: f64, y: f64) -> Option<String> {
        text_element_at(elements, [x, y], self.zoom)
    }

    /// `getTopLayerFrameAtSceneCoords({x, y})`: the unlocked frame on top
    /// whose box holds the point.
    fn top_layer_frame_at(&self, elements: &[Element], x: f64, y: f64) -> Option<String> {
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        live.iter()
            .rev()
            .filter(|e| is_frame_like(e) && !e.base.locked)
            .find(|e| {
                let [x1, y1, x2, y2] = get_element_bounds(e, &map);
                x1 <= x && x <= x2 && y1 <= y && y <= y2
            })
            .map(|e| e.base.id.clone())
    }

    fn update_bound_elements(
        &mut self,
        stamp: &mut dyn ChangeStamp,
        elements: &mut SceneElementsMap,
        id: &str,
    ) -> Result<(), String> {
        let mut char_widths = CharWidthCache::new();
        let mut env = StampBinding {
            stamp,
            provider: &self.provider,
            char_widths: &mut char_widths,
        };
        update_bound_elements_in_map(elements, id, &SceneElementsMap::new(), &mut env);
        Ok(())
    }

    fn grid_mode_enabled(&self) -> Option<bool> {
        self.grid_mode_enabled
    }
}

/// The topmost non-deleted element hit at `point` (bound text included)
/// when it is a text.
fn text_element_at(elements: &[Element], point: [f64; 2], zoom: f64) -> Option<String> {
    let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
    let map = ElementsMap::new(live.iter().copied());
    let mut cache = HitTestCache::new();
    live.iter()
        .rev()
        .find(|e| hit_element(&mut cache, point, e, &map, zoom, true, false, None))
        .filter(|e| matches!(e.kind, ElementKind::Text(_)))
        .map(|e| e.base.id.clone())
}

/// `getTextBindableContainerAtPosition(x, y)` (`App.tsx:6989-7024`): the
/// topmost arrow hit, or element whose box holds the point (frames
/// skipped), when it can hold a label (locked ones excluded).
fn text_bindable_container_at(elements: &[Element], point: [f64; 2], zoom: f64) -> Option<String> {
    let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
    let map = ElementsMap::new(live.iter().copied());
    let mut cache = HitTestCache::new();
    let [x, y] = point;
    let hit = live.iter().rev().find(|e| {
        if matches!(e.kind, ElementKind::Arrow(_)) {
            let args = HitTestArgs {
                point,
                element: e,
                threshold: get_element_hit_threshold(e.base.stroke_width, zoom),
                elements_map: &map,
                frame_name_bound: None,
                override_should_test_inside: false,
            };
            if hit_element_itself(&mut cache, &args) {
                return true;
            }
        }
        let [x1, y1, x2, y2, ..] = get_element_absolute_coords(e, &map, false);
        x1 < x && x < x2 && y1 < y && y < y2 && !is_frame_like(e)
    })?;
    is_text_bindable_container(hit, false).then(|| hit.base.id.clone())
}

/// `isTextBindableContainer(element, includeLocked)`.
fn is_text_bindable_container(element: &Element, include_locked: bool) -> bool {
    (!element.base.locked || include_locked) && element.element_type().is_text_container()
}

/// `hasBoundTextElement(element)`.
fn has_bound_text(element: &Element) -> bool {
    is_text_bindable_container(element, true)
        && element.base.bound_elements.as_ref().is_some_and(|b| {
            b.iter()
                .any(|b| b.kind == excali_core::element::BoundElementType::Text)
        })
}

/// `getContainerCenter(container, elementsMap)` for a shape: the middle
/// of its box (an arrow is left where it was pressed; the editor snaps to
/// its label point).
fn container_center(container: &Element) -> Option<[f64; 2]> {
    if matches!(container.kind, ElementKind::Arrow(_)) {
        return None;
    }
    let b = &container.base;
    Some([b.x + b.width / 2.0, b.y + b.height / 2.0])
}

impl<P: TextMetricsProvider + Clone> Editor<P> {
    /// Runs `f` with the text editing context: the session, its text
    /// layout and [`EditorHost`].
    fn with_text<R>(
        &mut self,
        f: impl FnOnce(&mut TextEditingContext<'_, EditorEnv<P>, P>) -> R,
    ) -> R {
        let provider = self.session.env.layouter.provider.clone();
        let mut layouter = std::mem::replace(
            &mut self.session.env.layouter,
            TextLayouter::new(provider.clone()),
        );
        let mut host = EditorHost {
            zoom: self.session.app_state().zoom().unwrap_or(1.0),
            grid_mode_enabled: self.props.grid_mode_enabled,
            provider,
        };
        let result = {
            let mut ctx = TextEditingContext {
                session: &mut self.session,
                layouter: &mut layouter,
                host: &mut host,
            };
            f(&mut ctx)
        };
        self.session.env.layouter = layouter;
        result
    }

    /// `startTextEditing(args)`: the editor opened, or the new text left
    /// as `newElement` (whose id is returned).
    fn start_text(&mut self, args: &StartTextEditing) -> Option<String> {
        match self.with_text(|ctx| start_text_editing(ctx, args)) {
            Ok(Some(editor)) => {
                self.text_editor = Some(editor);
                None
            }
            Ok(None) => self
                .session
                .app_state()
                .get("newElement")
                .and_then(|e| e.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            Err(_) => None,
        }
    }

    /// `handleTextWysiwyg(element, { isExistingElement: true })` for the
    /// text left as `newElement`.
    fn open_new_text(&mut self, id: &str) {
        let Some(element) = self
            .session
            .elements()
            .iter()
            .find(|e| e.base.id == id && !e.base.is_deleted)
            .cloned()
        else {
            return;
        };
        let mut patch = Map::new();
        patch.insert("newElement".into(), Value::Null);
        self.session.set_state(patch);
        if let Ok(editor) = self.with_text(|ctx| TextEditor::open(ctx, element, true, None)) {
            self.text_editor = Some(editor);
        }
    }

    /// `AppTextTool.finish`: the tool reverts unless it is locked.
    fn finish_text_tool(&mut self) {
        if !self.tools.is_tool_locked() {
            self.tools.active_tool = self.tools.tool_after_finalize();
        }
    }

    /// The text tool's press (`AppTextTool.handlePointerDown`): a text
    /// under the pointer is edited; an empty container near its centre is
    /// armed for a label (decided on release); anywhere else a new text
    /// starts, opened on release.
    pub(crate) fn text_pointer_down(&mut self, input: PointerInput) {
        if self.text_editor.is_some() {
            // a click while editing only finishes the edit
            return;
        }
        let origin = self.scene_point(input.client_x, input.client_y);
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        let origin_in_grid = get_grid_point(origin[0], origin[1], self.grid_size(input.ctrl_or_cmd));
        // a free arrow endpoint gets a label (AppArrowText,
        // App.arrowText.ts:52-100), sized by the drag
        if let Some(endpoint) = self.bindable_endpoint_at(origin) {
            let created = self.start_text(&StartTextEditing {
                auto_edit: false,
                text_element: TextTarget::New,
                arrow_endpoint: Some(endpoint),
                ..StartTextEditing::at(origin[0], origin[1])
            });
            self.finish_text_tool();
            if let Some(id) = created {
                self.gesture = Some(Gesture::TextCreate { id, origin_in_grid });
            }
            self.report();
            return;
        }
        let elements = self.session.elements().to_vec();
        if let Some(text) = text_element_at(&elements, origin, zoom) {
            self.start_text(&StartTextEditing {
                text_element: TextTarget::Existing(text),
                auto_edit: false,
                initial_caret: Some(origin),
                ..StartTextEditing::at(origin[0], origin[1])
            });
            self.finish_text_tool();
            self.report();
            return;
        }
        if !input.ctrl_or_cmd {
            let container = text_bindable_container_at(&elements, origin, zoom)
                .and_then(|id| elements.iter().find(|e| e.base.id == id).cloned())
                .filter(|c| !has_bound_text(c))
                .filter(|c| {
                    snapped_to_center_position(origin[0], origin[1], Some(c), &elements).is_some()
                });
            if let Some(container) = container {
                self.gesture = Some(Gesture::TextLabel {
                    container: container.base.id,
                    origin,
                });
                return;
            }
        }
        let created = self.start_text(&StartTextEditing {
            container: None,
            text_element: TextTarget::New,
            insert_at_parent_center: false,
            auto_edit: false,
            ..StartTextEditing::at(origin[0], origin[1])
        });
        self.finish_text_tool();
        if let Some(id) = created {
            self.gesture = Some(Gesture::TextCreate { id, origin_in_grid });
        }
        self.report();
    }

    /// `AppArrowText.getBindableEndpointAtPosition(x, y)`
    /// (`App.arrowText.ts:52-100`): with binding on, the free arrow
    /// endpoint under the point, unless an element stacked above the arrow
    /// is hit there.
    fn bindable_endpoint_at(&mut self, point: [f64; 2]) -> Option<ArrowEndpoint> {
        let enabled = self
            .session
            .app_state()
            .get("isBindingEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if !enabled {
            return None;
        }
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        let live: Vec<Element> = self
            .session
            .elements()
            .iter()
            .filter(|e| !e.base.is_deleted)
            .cloned()
            .collect();
        let endpoint = {
            let map = ElementsMap::new(live.iter());
            get_unbound_arrow_endpoint_at_point(point, &live, &map, zoom)?
        };
        if let Some(hit) = self.element_at_with(point, true) {
            let index = |id: &str| self.session.elements().iter().position(|e| e.base.id == id);
            if hit != endpoint.arrow_id && index(&hit) > index(&endpoint.arrow_id) {
                return None;
            }
        }
        Some(endpoint)
    }

    /// A move while the text tool's press lasts (`maybeDragNewGenericElement`
    /// with `dragNewTextElement`, `dragElements.ts:227-292`): an armed
    /// container's centre press past [`TEXT_AUTOWRAP_THRESHOLD`] becomes
    /// free text at the press (`resolvePending`); a new text is sized by
    /// the drag, pinned where the press was (or, bound to an arrow
    /// endpoint, where the binding put it).
    pub(crate) fn text_drag(&mut self, input: PointerInput) {
        let pointer = self.scene_point(input.client_x, input.client_y);
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        if let Some(Gesture::TextLabel { container, origin }) = self.gesture.clone() {
            if (pointer[0] - origin[0]).abs() * zoom <= TEXT_AUTOWRAP_THRESHOLD {
                return;
            }
            let exists = self
                .session
                .elements()
                .iter()
                .any(|e| e.base.id == container && !e.base.is_deleted);
            self.gesture = None;
            let created = exists
                .then(|| {
                    self.start_text(&StartTextEditing {
                        container: None,
                        text_element: TextTarget::New,
                        insert_at_parent_center: false,
                        auto_edit: false,
                        ..StartTextEditing::at(origin[0], origin[1])
                    })
                })
                .flatten();
            self.finish_text_tool();
            let Some(id) = created else {
                return self.report();
            };
            let origin_in_grid =
                get_grid_point(origin[0], origin[1], self.grid_size(input.ctrl_or_cmd));
            self.gesture = Some(Gesture::TextCreate { id, origin_in_grid });
        }
        let Some(Gesture::TextCreate { id, origin_in_grid }) = self.gesture.clone() else {
            return;
        };
        let Some(text) = self
            .session
            .elements()
            .iter()
            .find(|e| e.base.id == id && !e.base.is_deleted)
            .cloned()
        else {
            return;
        };
        let provider = self.session.env.layouter.provider.clone();
        let bound = {
            let live: Vec<&Element> = self
                .session
                .elements()
                .iter()
                .filter(|e| !e.base.is_deleted)
                .collect();
            let map = ElementsMap::new(live.iter().copied());
            is_endpoint_bound_text(&text, &map)
        };
        let update = if bound {
            // AppArrowText.maybeDragNewText (App.arrowText.ts:137-165)
            let anchor = get_endpoint_bound_text_drag_anchor(&text);
            drag_new_text_element(
                &text,
                anchor.anchor_x,
                anchor.anchor_ratio,
                pointer[0],
                None,
                zoom,
                &provider,
            )
        } else {
            let [gx, _] = get_grid_point(pointer[0], pointer[1], self.grid_size(input.ctrl_or_cmd));
            let [dx, dy] = self.origin_snap_offset().unwrap_or([0.0, 0.0]);
            let [ox, oy] = origin_in_grid;
            let ratio = if input.alt_key {
                0.5
            } else if gx < ox {
                1.0
            } else {
                0.0
            };
            drag_new_text_element(&text, ox + dx, ratio, gx, Some(oy + dy), zoom, &provider)
        };
        let mut scene = excali_editor::scene::Scene::new(self.session.elements().to_vec());
        scene.mutate_element(
            &id,
            excali_editor::scene::ElementUpdate {
                x: Some(update.x),
                y: update.y,
                width: Some(update.width),
                auto_resize: update.auto_resize,
                ..Default::default()
            },
            &mut self.session.env,
        );
        let app_state = self.session.app_state().clone();
        self.apply(scene, app_state);
        self.report();
    }

    /// The release of a text tool press: a new text opens its editor; an
    /// armed container gets its label (`resolvePending`), or free text at
    /// the press when the pointer travelled past
    /// [`TEXT_AUTOWRAP_THRESHOLD`].
    pub(crate) fn text_pointer_up(&mut self, input: PointerInput, gesture: Gesture) {
        match gesture {
            Gesture::TextCreate { id, .. } => {
                // a text narrower than one character grows by itself again
                // (App.tsx:11891-11911)
                let narrow = self
                    .session
                    .elements()
                    .iter()
                    .find(|e| e.base.id == id)
                    .and_then(|e| match &e.kind {
                        ElementKind::Text(t) => Some(
                            e.base.width
                                < get_min_text_element_width(
                                    &get_font_string(t.font_size, t.font_family),
                                    t.line_height,
                                    &self.session.env.layouter.provider,
                                ),
                        ),
                        _ => None,
                    })
                    .unwrap_or(false);
                if narrow {
                    let mut scene =
                        excali_editor::scene::Scene::new(self.session.elements().to_vec());
                    scene.mutate_element(
                        &id,
                        excali_editor::scene::ElementUpdate {
                            auto_resize: Some(true),
                            ..Default::default()
                        },
                        &mut self.session.env,
                    );
                    let app_state = self.session.app_state().clone();
                    self.apply(scene, app_state);
                }
                self.open_new_text(&id)
            }
            Gesture::TextLabel { container, origin } => {
                let pointer = self.scene_point(input.client_x, input.client_y);
                let zoom = self.session.app_state().zoom().unwrap_or(1.0);
                let dragged = (pointer[0] - origin[0]).abs() * zoom > TEXT_AUTOWRAP_THRESHOLD;
                let exists = self
                    .session
                    .elements()
                    .iter()
                    .any(|e| e.base.id == container && !e.base.is_deleted);
                if exists {
                    let created = self.start_text(&StartTextEditing {
                        container: (!dragged).then(|| container.clone()),
                        text_element: TextTarget::New,
                        insert_at_parent_center: !dragged,
                        auto_edit: !dragged,
                        ..StartTextEditing::at(origin[0], origin[1])
                    });
                    if let Some(id) = created {
                        self.open_new_text(&id);
                    }
                }
                self.finish_text_tool();
            }
            _ => {}
        }
        self.report();
    }

    /// A release of the selection tool on the one selected text, without a
    /// drag or modifier, and not the press that selected it
    /// (`App.tsx:12504-12531`): the text is edited, the caret where the
    /// pointer is.
    pub(crate) fn maybe_edit_selected_text(&mut self, input: PointerInput, hit: Option<&str>) {
        let Some(hit) = hit else {
            return;
        };
        if input.shift_key || input.alt_key || input.ctrl_or_cmd || self.text_editor.is_some() {
            return;
        }
        let selected = self.selected_ids();
        let is_text = self
            .session
            .elements()
            .iter()
            .any(|e| e.base.id == hit && matches!(e.kind, ElementKind::Text(_)));
        if !is_text || selected != [hit.to_owned()] {
            return;
        }
        let point = self.scene_point(input.client_x, input.client_y);
        self.start_text(&StartTextEditing {
            initial_caret: Some(point),
            ..StartTextEditing::at(point[0], point[1])
        });
        self.report();
    }

    /// The canvas's `dblclick` (`handleCanvasDoubleClick`): with the
    /// selection tool, a group member enters its group; otherwise the text
    /// under the pointer, or the label of the container (the one selected
    /// element, else the one under the pointer; not with Ctrl/Cmd), is
    /// edited, or a new text starts there (at the container's centre when
    /// it is filled, labelled or its outline was hit; Alt keeps the point).
    pub fn double_click(&mut self, input: PointerInput) {
        if !self.tools.is_interaction_enabled() || self.text_editor.is_some() {
            return;
        }
        if self.tools.active_tool.tool.type_name() != "selection" {
            return;
        }
        let [mut x, mut y] = self.scene_point(input.client_x, input.client_y);
        // a line's editor, an elbow arrow's fixed segment (App.tsx:7379-7462)
        if self.linear_double_click(input, [x, y]) {
            return;
        }
        // an image is cropped (App.tsx:7468-7471)
        if let Some(id) = self.selected_image() {
            self.start_image_cropping(&id);
            return;
        }
        let elements = self.session.elements().to_vec();
        let selected: Vec<Element> = {
            let ids = self.selected_ids();
            elements
                .iter()
                .filter(|e| !e.base.is_deleted && ids.contains(&e.base.id))
                .cloned()
                .collect()
        };
        // entering a group
        let selected_groups: Vec<String> = self
            .session
            .app_state()
            .get("selectedGroupIds")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter(|(_, v)| v.as_bool() == Some(true))
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default();
        if !selected_groups.is_empty() {
            if let Some(hit) = self.element_at([x, y]) {
                let group = elements.iter().find(|e| e.base.id == hit).and_then(|e| {
                    e.base
                        .group_ids
                        .iter()
                        .find(|g| selected_groups.contains(g))
                        .cloned()
                });
                if let Some(group) = group {
                    let live: Vec<&Element> =
                        elements.iter().filter(|e| !e.base.is_deleted).collect();
                    let mut ids = Map::new();
                    ids.insert(hit, Value::Bool(true));
                    let next = excali_editor::groups::select_groups_for_selected_elements(
                        &ids,
                        Some(&group),
                        &live,
                    );
                    let mut patch = Map::new();
                    patch.insert(
                        "selectedElementIds".into(),
                        Value::Object(next.selected_element_ids),
                    );
                    patch.insert(
                        "selectedGroupIds".into(),
                        Value::Object(next.selected_group_ids),
                    );
                    patch.insert("editingGroupId".into(), json!(group));
                    self.session.set_state(patch);
                    self.session.store.schedule_capture();
                    self.session.commit();
                    self.report();
                    return;
                }
            }
        }
        let view_mode = self
            .session
            .app_state()
            .get("viewModeEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let editing_linear = self
            .session
            .app_state()
            .get("selectedLinearElement")
            .and_then(|l| l.get("isEditing"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if view_mode || editing_linear {
            return;
        }
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        let container: Option<Element> = if input.ctrl_or_cmd {
            None
        } else if selected.len() == 1 {
            Some(selected[0].clone()).filter(|c| is_text_bindable_container(c, false))
        } else {
            text_bindable_container_at(&elements, [x, y], zoom)
                .and_then(|id| elements.iter().find(|e| e.base.id == id).cloned())
        };
        if let Some(c) = &container {
            let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
            let map = ElementsMap::new(live.iter().copied());
            let hit_outline = hit_element_itself(
                &mut HitTestCache::new(),
                &HitTestArgs {
                    point: [x, y],
                    element: c,
                    threshold: get_element_hit_threshold(c.base.stroke_width, zoom),
                    elements_map: &map,
                    frame_name_bound: None,
                    override_should_test_inside: false,
                },
            );
            if has_bound_text(c) || !is_transparent(&c.base.background_color) || hit_outline {
                if let Some([cx, cy]) = container_center(c) {
                    x = cx;
                    y = cy;
                }
            }
        }
        self.start_text(&StartTextEditing {
            insert_at_parent_center: !input.alt_key,
            container: container.map(|c| c.base.id),
            ..StartTextEditing::at(x, y)
        });
        self.report();
    }

    /// The textarea of the open editor, as it should look.
    pub fn textarea(&self) -> Option<TextareaState> {
        self.text_editor
            .as_ref()
            .filter(|e| e.is_open())
            .map(TextareaState::of)
    }

    /// The open editor's textarea attributes (`dir="auto"`, `wrap="off"`).
    pub fn textarea_attributes(&self) -> Option<TextareaAttributes> {
        self.text_editor
            .as_ref()
            .filter(|e| e.is_open())
            .map(TextEditor::attributes)
    }

    /// Where the open editor wants the caret, which the page's layout
    /// answers ([`Editor::resolve_caret`]).
    pub fn caret_request(&self) -> Option<CaretRequest> {
        self.text_editor
            .as_ref()
            .and_then(|e| e.caret_request().cloned())
    }

    /// The caret offset the page measured for [`Editor::caret_request`].
    pub fn resolve_caret(&mut self, offset: Option<usize>) {
        if let Some(editor) = self.text_editor.as_mut() {
            editor.resolve_caret(offset);
        }
    }

    /// An event of the textarea for the open editor, as
    /// `TextEditingApp::on_event` hands it over. The editor is dropped once
    /// it closes (submitted); the answer's state says so.
    pub fn textarea_event(&mut self, event: TextareaEvent) -> Handled {
        let Some(mut editor) = self.text_editor.take() else {
            return Handled::default();
        };
        let answer = self.with_text(|ctx| match event {
            TextareaEvent::Focused => {
                editor.focused();
                Ok(false)
            }
            TextareaEvent::Select { selection } => {
                editor.set_selection(selection.0, selection.1);
                Ok(false)
            }
            TextareaEvent::Input { value, selection } => {
                editor.input(ctx, &value, selection).map(|_| false)
            }
            TextareaEvent::KeyDown { key, selection } => {
                editor.set_selection(selection.0, selection.1);
                editor
                    .keydown(
                        ctx,
                        &KeyDown {
                            key: &key.key,
                            code: &key.code,
                            shift_key: key.shift_key,
                            alt_key: key.alt_key,
                            ctrl_or_cmd: key.ctrl_or_cmd,
                            is_composing: key.is_composing,
                            key_code: key.key_code,
                        },
                    )
                    .map(|o| o.prevent_default)
            }
            TextareaEvent::Paste {
                types,
                text,
                selection,
            } => {
                editor.set_selection(selection.0, selection.1);
                let types: Vec<&str> = types.iter().map(String::as_str).collect();
                editor
                    .paste(ctx, &types, text.as_deref())
                    .map(|o| o == PasteOutcome::Prevented)
            }
            TextareaEvent::Submit => editor.submit(ctx).map(|_| false),
            TextareaEvent::EditorBoxScrolled { left, top } => {
                editor.editor_box_scrolled(ctx, left, top).map(|_| false)
            }
            TextareaEvent::Resized => editor.relayout(ctx).map(|_| false),
            TextareaEvent::PanStart { .. } => Ok(false),
        });
        for call in editor.take_calls() {
            if let AppCall::ExecuteAction(name) = call {
                if let Some(action) = excali_editor::actions::ActionName::from_name(name) {
                    self.perform_action(action);
                }
            }
        }
        let state = TextareaState::of(&editor);
        if editor.is_open() {
            self.text_editor = Some(editor);
        }
        self.report();
        Handled {
            prevent_default: answer.unwrap_or(false),
            state: Some(state),
        }
    }

    /// The open editor laid out again after the scene or viewport changed
    /// under it (`TextEditor::app_changed`); its textarea's state.
    pub fn text_editor_app_changed(&mut self) -> Option<TextareaState> {
        let mut editor = self.text_editor.take()?;
        let _ = self.with_text(|ctx| editor.app_changed(ctx));
        let state = TextareaState::of(&editor);
        if editor.is_open() {
            self.text_editor = Some(editor);
        }
        Some(state)
    }
}
