//! The stats panel's element property edits: the `dragInputCallback` of
//! each component (`components/Stats/{Position,Dimension,Angle,FontSize,
//! MultiPosition,MultiDimension,MultiAngle,MultiFontSize}.tsx`), their
//! shared helpers (`Stats/utils.ts`) and `DragInput`'s pointer arithmetic
//! (`Stats/DragInput.tsx`).
//!
//! A [`StatsGesture`] is what `DragInput` holds while a value is typed or
//! its label dragged: the component's elements and a copy of the scene
//! taken at the start, and the app state then. [`StatsGesture::apply`]
//! runs the component's callback once per typed value or pointer move
//! ([`StatsDrag`]); [`StatsGesture::finish`] is the `dragFinishedCallback`
//! at pointer up. The caller captures the result as one history entry
//! (`app.syncActionResult({ captureUpdate: IMMEDIATELY })`).

use excali_core::app_state::AppState;
use excali_core::constants::DRAGGING_THRESHOLD;
use excali_core::element::{Element, ElementKind};
use excali_core::fractional_index::sync_invalid_indices;
use excali_math::{
    clamp, degrees_to_radians, point_rotate_rads, radians_to_degrees, Degrees, GlobalPoint, Radians,
};
use excali_scene::bounds::{get_bound_text_element, get_common_bounds, ElementsMap};
use excali_scene::crop::get_uncropped_width_and_height;
use excali_scene::frame::is_frame_like;
use serde_json::Value;

use crate::actions::frame_and_children_selected_together;
use crate::binding::{unbind_binding_element, update_bindings, BindingEnd};
use crate::crop::MINIMAL_CROP_SIZE;
use crate::edit_actions::EditEnv;
use crate::frame::{get_elements_in_resizing_frame, replace_all_elements_in_frame};
use crate::keyboard::{binding_app_state, get_selected_elements};
use crate::mutate::bump_version;
use crate::resize_elements::{
    get_sticky_note_resize_intent, handle_bind_text_resize, rescale_points_in_element,
    resize_single_element, update_sticky_note_layout, ResizeOptions, StickyNoteLayoutAnchor,
    TransformEnv,
};
use crate::scene::{ElementUpdate, Scene};
use crate::text_layout::normalize_sticky_note_font_size;
use crate::transform_handles::TransformHandleDirection;

/// `SMALLEST_DELTA` (`Stats/utils.ts:43`).
pub const SMALLEST_DELTA: f64 = 0.01;
/// `STEP_SIZE` (`Stats/utils.ts:44`): Position's and MultiPosition's step.
pub const STEP_SIZE: f64 = 10.0;
/// `MIN_WIDTH_OR_HEIGHT` (`common/src/constants.ts`).
const MIN_WIDTH_OR_HEIGHT: f64 = 1.0;
/// Dimension's and MultiDimension's step.
const DIMENSION_STEP_SIZE: f64 = 10.0;
/// Angle's and MultiAngle's step, in degrees.
const ANGLE_STEP_SIZE: f64 = 15.0;
/// FontSize's and MultiFontSize's step and smallest size.
const FONT_STEP_SIZE: f64 = 4.0;
const MIN_FONT_SIZE: f64 = 4.0;

/// A value the panel edits (`StatsInputProperty`, `Stats/utils.ts:34-41`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatsProperty {
    X,
    Y,
    Width,
    Height,
    Angle,
    FontSize,
    GridStep,
}

/// What the editor supplies to the callbacks besides a transform's needs:
/// `redrawTextBoundingBox(text, container, scene)` (`textElement.ts:51-152`)
/// on the scene.
pub trait StatsEnv: TransformEnv + EditEnv {
    fn redraw_text_bounding_box(
        &mut self,
        scene: &mut Scene,
        text_id: &str,
        container_id: Option<&str>,
    );
}

/// One call of a `dragInputCallback`: `accumulatedChange`, `instantChange`,
/// `shouldChangeByStepSize` and `nextValue` (a typed value).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StatsChange {
    pub accumulated_change: f64,
    pub instant_change: f64,
    pub should_change_by_step_size: bool,
    pub next_value: Option<f64>,
}

impl StatsChange {
    /// A typed value (`handleInputValue`, `DragInput.tsx:155-170`).
    pub fn typed(value: f64) -> StatsChange {
        StatsChange {
            next_value: Some(value),
            ..StatsChange::default()
        }
    }
}

/// What a callback passed to `setAppState`: `elementsToHighlight`, the ids
/// (`None` for `null`).
pub type HighlightPatch = Option<Vec<String>>;

/// `DragInput`'s pointer arithmetic (`DragInput.tsx:255-305`): the label's
/// drag turns pointer moves along x into changes of `sensitivity` pixels
/// each.
#[derive(Clone, Debug, Default)]
pub struct StatsDrag {
    last_x: Option<f64>,
    step_change: f64,
    accumulated_change: f64,
}

impl StatsDrag {
    pub fn new() -> StatsDrag {
        StatsDrag::default()
    }

    /// A `pointermove` at `client_x`: `(accumulatedChange, instantChange)`
    /// when the callback runs. The first move only records the pointer.
    pub fn pointer_move(&mut self, client_x: f64, sensitivity: f64) -> Option<(f64, f64)> {
        let mut out = None;
        if let Some(last_x) = self.last_x {
            let instant_change = client_x - last_x;
            if instant_change != 0.0 {
                self.step_change += instant_change;
                if self.step_change.abs() >= sensitivity {
                    self.step_change =
                        sign(self.step_change) * (self.step_change.abs() / sensitivity).floor();
                    self.accumulated_change += self.step_change;
                    out = Some((self.accumulated_change, self.step_change));
                    self.step_change = 0.0;
                }
            }
        }
        self.last_x = Some(client_x);
        out
    }
}

fn sign(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        v
    }
}

/// `CanvasGrid`'s callback for a drag (`CanvasGrid.tsx:29-58`, sensitivity
/// 8): the grid step moved by the instant change, or to the next multiple
/// of [`STEP_SIZE`] in its direction with Shift, normalised
/// (`getNormalizedGridStep`); `None` leaves it.
pub fn grid_step_after_drag(grid_step: f64, instant_change: f64, shift: bool) -> Option<f64> {
    if instant_change == 0.0 || instant_change.is_nan() {
        return None;
    }
    let next = if shift {
        get_step_sized_value(grid_step + STEP_SIZE * sign(instant_change), STEP_SIZE)
    } else {
        grid_step + instant_change
    };
    if next == 0.0 || next.is_nan() {
        return None;
    }
    Some(clamp(next.round_js(), 1.0, 100.0))
}

/// `CanvasGrid`'s `sensitivity`.
pub const GRID_STEP_SENSITIVITY: f64 = 8.0;

// -- utils.ts ---------------------------------------------------------------------

/// `isPropertyEditable(element, property)` (`Stats/utils.ts:46-54`).
pub fn is_property_editable(element: &Element, property: StatsProperty) -> bool {
    !(property == StatsProperty::Angle && is_frame_like(element))
}

/// `getStepSizedValue(value, stepSize)` (`Stats/utils.ts:56-59`).
pub fn get_step_sized_value(value: f64, step_size: f64) -> f64 {
    let v = value + step_size / 2.0;
    v - (v % step_size)
}

fn is_in_group(element: &Element) -> bool {
    !element.base.group_ids.is_empty()
}

/// `getAtomicUnits(targetElements, appState)` (`Stats/utils.ts:230-254`):
/// each selected group's elements among the targets, then each target in
/// no group.
pub fn get_atomic_units(targets: &[&Element], app_state: &AppState) -> Vec<Vec<String>> {
    let mut units: Vec<Vec<String>> = app_state
        .get("selectedGroupIds")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(_, v)| truthy(v))
        .map(|(gid, _)| {
            let mut ids: Vec<String> = Vec::new();
            for el in targets {
                if el.base.group_ids.iter().any(|g| g == gid) && !ids.contains(&el.base.id) {
                    ids.push(el.base.id.clone());
                }
            }
            ids
        })
        .collect();
    for el in targets.iter().filter(|el| !is_in_group(el)) {
        units.push(vec![el.base.id.clone()]);
    }
    units
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn find<'a>(elements: &'a [Element], id: &str) -> Option<&'a Element> {
    elements.iter().find(|e| e.base.id == id)
}

/// `getElementsInAtomicUnit(atomicUnit, elementsMap, originalElementsMap)`
/// (`Stats/utils.ts:60-76`): `(original, latest)` of each id both have.
fn elements_in_unit(
    unit: &[String],
    scene: &Scene,
    original: &[Element],
) -> Vec<(Element, Element)> {
    unit.iter()
        .filter_map(|id| {
            Some((
                find(original, id)?.clone(),
                scene.get_non_deleted(id)?.clone(),
            ))
        })
        .collect()
}

fn point(x: f64, y: f64) -> GlobalPoint {
    GlobalPoint::new(x, y)
}

fn center(el: &Element) -> [f64; 2] {
    [
        el.base.x + el.base.width / 2.0,
        el.base.y + el.base.height / 2.0,
    ]
}

/// The element's top left corner on the canvas (its `x`, `y` rotated about
/// its centre).
fn top_left(el: &Element) -> [f64; 2] {
    let [cx, cy] = center(el);
    let p = point_rotate_rads(
        point(el.base.x, el.base.y),
        point(cx, cy),
        Radians(el.base.angle.0),
    );
    [p.x, p.y]
}

fn is_arrow(el: &Element) -> bool {
    matches!(el.kind, ElementKind::Arrow(_))
}

fn is_elbow_arrow(el: &Element) -> bool {
    matches!(&el.kind, ElementKind::Arrow(a) if a.elbowed)
}

fn is_text(el: &Element) -> bool {
    matches!(el.kind, ElementKind::Text(_))
}

fn is_sticky_note(el: &Element) -> bool {
    matches!(el.kind, ElementKind::StickyNote(_))
}

fn container_id(el: &Element) -> Option<&str> {
    match &el.kind {
        ElementKind::Text(t) => t.container_id.as_deref(),
        _ => None,
    }
}

fn has_bindings(el: &Element) -> bool {
    match &el.kind {
        ElementKind::Arrow(a) => a.linear.start_binding.is_some() || a.linear.end_binding.is_some(),
        _ => false,
    }
}

fn scene_map(scene: &Scene) -> ElementsMap<'_> {
    scene.elements_map()
}

/// `isStickyNoteBoundText(text, elementsMap)` (`stickyNote.ts:387-395`).
fn is_sticky_note_bound_text(text: &Element, map: &ElementsMap<'_>) -> bool {
    container_id(text)
        .and_then(|c| map.get(c))
        .is_some_and(is_sticky_note)
}

/// `getBaseFontSize(textElement, elementsMap)` (`stickyNote.ts:405-412`).
fn get_base_font_size(text: &Element, map: &ElementsMap<'_>) -> f64 {
    let ElementKind::Text(t) = &text.kind else {
        return f64::NAN;
    };
    if is_sticky_note_bound_text(text, map) {
        t.base_font_size.unwrap_or(t.font_size)
    } else {
        t.font_size
    }
}

/// `getBaseFontSizeUpdate(textElement, fontSize, elementsMap)`
/// (`stickyNote.ts:415-423`).
fn get_base_font_size_update(
    text: &Element,
    font_size: f64,
    map: &ElementsMap<'_>,
) -> ElementUpdate {
    if is_sticky_note_bound_text(text, map) {
        ElementUpdate {
            base_font_size: Some(Some(normalize_sticky_note_font_size(font_size))),
            ..ElementUpdate::default()
        }
    } else {
        ElementUpdate {
            font_size: Some(font_size),
            ..ElementUpdate::default()
        }
    }
}

/// `moveElement(newTopLeftX, newTopLeftY, originalElement, scene, appState,
/// originalElementsMap)` (`Stats/utils.ts:119-228`): the element moved so
/// its top left corner is at the new point; a bound arrow is unbound first
/// (unless it moves less than `DRAGGING_THRESHOLD`), its label and a
/// frame's children follow, and bindings are updated.
#[allow(clippy::too_many_arguments)]
fn move_element<E: StatsEnv>(
    new_top_left_x: f64,
    new_top_left_y: f64,
    original: &Element,
    scene: &mut Scene,
    app_state: &AppState,
    original_elements: &[Element],
    env: &mut E,
) {
    let id = original.base.id.as_str();
    if is_arrow(original) && has_bindings(original) {
        if (new_top_left_x - original.base.x).abs() < DRAGGING_THRESHOLD
            && (new_top_left_y - original.base.y).abs() < DRAGGING_THRESHOLD
        {
            return;
        }
        unbind_binding_element(scene, env, id, BindingEnd::Start);
        unbind_binding_element(scene, env, id, BindingEnd::End);
    }
    if scene.get_non_deleted(id).is_none() {
        return;
    }
    let [cx, cy] = center(original);
    let [tlx, tly] = top_left(original);
    let change_x = new_top_left_x - tlx;
    let change_y = new_top_left_y - tly;
    let p = point_rotate_rads(
        point(new_top_left_x, new_top_left_y),
        point(cx + change_x, cy + change_y),
        Radians(-original.base.angle.0),
    );
    scene.mutate_element(id, ElementUpdate::position(p.x, p.y), env);
    let binding_state = binding_app_state(app_state);
    let _ = update_bindings(scene, env, id, &binding_state, None);

    let original_map = ElementsMap::new(original_elements.iter());
    if let Some(bound_text) = get_bound_text_element(original, &original_map) {
        if scene.get_non_deleted(&bound_text.base.id).is_some() {
            scene.mutate_element(
                &bound_text.base.id,
                ElementUpdate::position(bound_text.base.x + change_x, bound_text.base.y + change_y),
                env,
            );
        }
    }

    if is_frame_like(original) {
        let children: Vec<&Element> = original_elements
            .iter()
            .filter(|e| e.base.frame_id.as_deref() == Some(id))
            .collect();
        let simultaneously: Vec<String> = children
            .iter()
            .filter(|c| !c.base.is_deleted)
            .map(|c| c.base.id.clone())
            .collect();
        for child in &children {
            if scene.get_non_deleted(&child.base.id).is_none() {
                continue;
            }
            let [ccx, ccy] = center(child);
            let [ctlx, ctly] = top_left(child);
            let nx = (ctlx + change_x).round_js();
            let ny = (ctly + change_y).round_js();
            let p = point_rotate_rads(
                point(nx, ny),
                point(ccx + change_x, ccy + change_y),
                Radians(-child.base.angle.0),
            );
            scene.mutate_element(&child.base.id, ElementUpdate::position(p.x, p.y), env);
            let _ = update_bindings(
                scene,
                env,
                &child.base.id,
                &binding_state,
                Some(&simultaneously),
            );
        }
    }
}

/// `Math.round`.
trait RoundJs {
    fn round_js(self) -> f64;
}

impl RoundJs for f64 {
    fn round_js(self) -> f64 {
        excali_math::js::round(self)
    }
}

// -- the gesture ------------------------------------------------------------------

/// What `DragInput` holds while its value is typed or its label dragged.
#[derive(Clone, Debug)]
pub struct StatsGesture {
    pub property: StatsProperty,
    /// Whether the panel shows the Multi* component (more than one element
    /// selected).
    pub multiple: bool,
    /// The component's `elements`, as they were at the start.
    pub elements: Vec<Element>,
    /// `originalElementsMap`: every non-deleted element at the start, in
    /// scene order.
    pub original: Vec<Element>,
    /// `originalAppState`.
    pub app_state: AppState,
}

/// `getApplicableTextElements(elements, elementsMap)`
/// (`MultiFontSize.tsx:40-66`).
fn applicable_text_elements<'a>(
    elements: &[&'a Element],
    map: &ElementsMap<'a>,
) -> Vec<&'a Element> {
    let mut out = Vec::new();
    for el in elements {
        if is_in_group(el) {
            continue;
        }
        if is_text(el) {
            out.push(*el);
            continue;
        }
        if let Some(text) = get_bound_text_element(el, map) {
            out.push(text);
        }
    }
    out
}

impl StatsGesture {
    /// The gesture on the input for `property` that the panel shows for the
    /// selection (`index.tsx:248-415`), `None` when it shows none: nothing
    /// selected, a frame selected with its children, an angle of a frame
    /// (or of elements that are all grouped or frames), a font size with no
    /// text, or the grid step.
    pub fn begin(
        scene: &Scene,
        app_state: &AppState,
        property: StatsProperty,
    ) -> Option<StatsGesture> {
        let selected = get_selected_elements(scene, app_state, false, false);
        if selected.is_empty() || frame_and_children_selected_together(&selected) {
            return None;
        }
        let map = scene_map(scene);
        let multiple = selected.len() > 1;
        let elements: Vec<&Element> = match (property, multiple) {
            (StatsProperty::GridStep, _) => return None,
            (StatsProperty::FontSize, false) => {
                let el = selected[0];
                let text = if is_text(el) {
                    Some(el)
                } else {
                    get_bound_text_element(el, &map)
                };
                vec![text?]
            }
            (StatsProperty::FontSize, true) => {
                let texts = applicable_text_elements(&selected, &map);
                if texts.is_empty() {
                    return None;
                }
                texts
            }
            (StatsProperty::Angle, false) => {
                if !is_property_editable(selected[0], property) {
                    return None;
                }
                selected.clone()
            }
            (StatsProperty::Angle, true) => {
                if !selected
                    .iter()
                    .any(|el| !is_in_group(el) && is_property_editable(el, property))
                {
                    return None;
                }
                selected.clone()
            }
            (StatsProperty::Width | StatsProperty::Height, true) => {
                if get_atomic_units(&selected, app_state).is_empty() {
                    return None;
                }
                selected.clone()
            }
            _ => selected.clone(),
        };
        Some(StatsGesture {
            property,
            multiple,
            elements: elements.into_iter().cloned().collect(),
            original: scene.non_deleted().into_iter().cloned().collect(),
            app_state: app_state.clone(),
        })
    }

    /// The component's `dragInputCallback` with `change`; `app_state` is
    /// the editor's current state (`app.state`). Returns what it passed to
    /// `setAppState` as `elementsToHighlight`, if anything.
    pub fn apply<E: StatsEnv>(
        &self,
        scene: &mut Scene,
        app_state: &AppState,
        change: StatsChange,
        env: &mut E,
    ) -> Option<HighlightPatch> {
        use StatsProperty::*;
        match (self.property, self.multiple) {
            (X | Y, false) => {
                self.position(scene, app_state, change, env);
                None
            }
            (Width | Height, false) => self.dimension(scene, change, env),
            (Angle, false) => {
                self.angle(scene, app_state, change, env);
                None
            }
            (FontSize, false) => {
                self.font_size(scene, change, env);
                None
            }
            (X | Y, true) => {
                self.multi_position(scene, app_state, change, env);
                None
            }
            (Width | Height, true) => self.multi_dimension(scene, change, env),
            (Angle, true) => {
                self.multi_angle(scene, change, env);
                None
            }
            (FontSize, true) => {
                self.multi_font_size(scene, change, env);
                None
            }
            (GridStep, _) => None,
        }
    }

    /// The `dragFinishedCallback` at pointer up (Dimension's and
    /// MultiDimension's `handleDragFinished`): a resized frame (the first
    /// of the component's elements) takes the elements now inside it, and
    /// the highlight is cleared.
    pub fn finish<E: StatsEnv>(&self, scene: &mut Scene, env: &mut E) -> Option<HighlightPatch> {
        if !matches!(self.property, StatsProperty::Width | StatsProperty::Height) {
            return None;
        }
        let orig = self.elements.first()?;
        let latest = scene.get_non_deleted(&orig.base.id)?.clone();
        if !is_frame_like(&latest) {
            return None;
        }
        update_frame_membership(scene, &latest, &self.app_state, env);
        Some(None)
    }

    fn is_cropping(&self, id: &str) -> bool {
        self.app_state
            .get("croppingElementId")
            .and_then(Value::as_str)
            == Some(id)
    }

    // -- Position.tsx ----------------------------------------------------------------

    fn position<E: StatsEnv>(
        &self,
        scene: &mut Scene,
        app_state: &AppState,
        change: StatsChange,
        env: &mut E,
    ) {
        let Some(orig) = self.elements.first() else {
            return;
        };
        let is_x = self.property == StatsProperty::X;
        let [tlx, tly] = top_left(orig);

        if self.is_cropping(&orig.base.id) {
            let Some(element) = scene.get_non_deleted(&orig.base.id).cloned() else {
                return;
            };
            let ElementKind::Image(image) = &element.kind else {
                return;
            };
            let Some(crop) = image.crop else {
                return;
            };
            let flipped_x = image.scale[0] == -1.0;
            let flipped_y = image.scale[1] == -1.0;
            let (uncropped_w, uncropped_h) = get_uncropped_width_and_height(&element);
            let mut next = crop;
            if let Some(v) = change.next_value {
                if is_x {
                    let natural = v * (crop.natural_width / uncropped_w);
                    next.x = if flipped_x {
                        clamp(
                            crop.natural_width - natural - crop.width,
                            0.0,
                            crop.natural_width - crop.width,
                        )
                    } else {
                        clamp(
                            v * (crop.natural_width / uncropped_w),
                            0.0,
                            crop.natural_width - crop.width,
                        )
                    };
                } else {
                    next.y = clamp(
                        v * (crop.natural_height / uncropped_h),
                        0.0,
                        crop.natural_height - crop.height,
                    );
                }
            } else {
                let dx = if is_x { change.instant_change } else { 0.0 }
                    * if flipped_x { -1.0 } else { 1.0 };
                let dy = if is_x { 0.0 } else { change.instant_change }
                    * if flipped_y { -1.0 } else { 1.0 };
                next.x = clamp(crop.x + dx, 0.0, crop.natural_width - crop.width);
                next.y = clamp(crop.y + dy, 0.0, crop.natural_height - crop.height);
            }
            set_crop(scene, &element, next, None, env);
            return;
        }

        let original = &self.original;
        if let Some(v) = change.next_value {
            let (nx, ny) = if is_x { (v, tly) } else { (tlx, v) };
            move_element(nx, ny, orig, scene, app_state, original, env);
            return;
        }
        let acc = change.accumulated_change;
        let nx = if is_x {
            if change.should_change_by_step_size {
                get_step_sized_value(orig.base.x + acc, STEP_SIZE)
            } else {
                tlx + acc
            }
            .round_js()
        } else {
            tlx
        };
        let ny = if !is_x {
            if change.should_change_by_step_size {
                get_step_sized_value(orig.base.y + acc, STEP_SIZE)
            } else {
                tly + acc
            }
            .round_js()
        } else {
            tly
        };
        move_element(nx, ny, orig, scene, app_state, original, env);
    }

    // -- Dimension.tsx ---------------------------------------------------------------

    fn dimension<E: StatsEnv>(
        &self,
        scene: &mut Scene,
        change: StatsChange,
        env: &mut E,
    ) -> Option<HighlightPatch> {
        let orig = self.elements.first()?;
        let latest = scene.get_non_deleted(&orig.base.id)?.clone();
        let is_width = self.property == StatsProperty::Width;
        let keep_aspect_ratio = matches!(orig.kind, ElementKind::Image(_));
        let aspect_ratio = orig.base.width / orig.base.height;

        if self.is_cropping(&orig.base.id) {
            let ElementKind::Image(image) = &latest.kind else {
                return None;
            };
            let crop = image.crop?;
            let flipped_x = image.scale[0] == -1.0;
            let flipped_y = image.scale[1] == -1.0;
            let (uncropped_w, uncropped_h) = get_uncropped_width_and_height(&latest);
            let w_ratio = crop.natural_width / uncropped_w;
            let h_ratio = crop.natural_height / uncropped_h;
            let max_w = if flipped_x {
                crop.width + crop.x
            } else {
                crop.natural_width - crop.x
            };
            let max_h = if flipped_y {
                crop.height + crop.y
            } else {
                crop.natural_height - crop.y
            };
            let min_w = MINIMAL_CROP_SIZE * w_ratio;
            let min_h = MINIMAL_CROP_SIZE * h_ratio;
            let mut next = crop;
            if let Some(v) = change.next_value {
                if is_width {
                    let w = clamp(v * w_ratio, min_w, max_w);
                    next.width = w;
                    next.x = if flipped_x {
                        crop.x + crop.width - w
                    } else {
                        crop.x
                    };
                } else {
                    let h = clamp(v * h_ratio, min_h, max_h);
                    next.height = h;
                    next.y = if flipped_y {
                        crop.y + crop.height - h
                    } else {
                        crop.y
                    };
                }
            } else {
                let dw = if is_width { change.instant_change } else { 0.0 };
                let dh = if is_width { 0.0 } else { change.instant_change };
                let w = clamp(crop.width + dw, min_w, max_w);
                // upstream clamps the height by the minimum width
                let h = clamp(crop.height + dh, min_w, max_h);
                next.x = if flipped_x {
                    crop.x + crop.width - w
                } else {
                    crop.x
                };
                next.y = if flipped_y {
                    crop.y + crop.height - h
                } else {
                    crop.y
                };
                next.width = w;
                next.height = h;
            }
            let size = [
                next.width / (crop.natural_width / uncropped_w),
                next.height / (crop.natural_height / uncropped_h),
            ];
            set_crop(scene, &latest, next, Some(size), env);
            return None;
        }

        let handle = if is_width {
            TransformHandleDirection::E
        } else {
            TransformHandleDirection::S
        };
        let options = ResizeOptions {
            should_maintain_aspect_ratio: keep_aspect_ratio,
            should_resize_from_center: false,
        };

        if let Some(v) = change.next_value {
            let next_width = excali_math::js::max(
                if is_width {
                    v
                } else if keep_aspect_ratio {
                    v * aspect_ratio
                } else {
                    orig.base.width
                },
                MIN_WIDTH_OR_HEIGHT,
            );
            let next_height = excali_math::js::max(
                if !is_width {
                    v
                } else if keep_aspect_ratio {
                    v / aspect_ratio
                } else {
                    orig.base.height
                },
                MIN_WIDTH_OR_HEIGHT,
            );
            resize_single_element(
                next_width,
                next_height,
                &orig.base.id,
                &self.original,
                scene,
                env,
                handle,
                options,
            );
            if let Some(latest) = scene.get_non_deleted(&orig.base.id).cloned() {
                if is_frame_like(&latest) {
                    update_frame_membership(scene, &latest, &self.app_state, env);
                }
            }
            return None;
        }

        let acc = change.accumulated_change;
        let mut next_width =
            excali_math::js::max(0.0, orig.base.width + if is_width { acc } else { 0.0 });
        if is_width {
            next_width = step_or_round(next_width, change.should_change_by_step_size);
        }
        let mut next_height =
            excali_math::js::max(0.0, orig.base.height + if is_width { 0.0 } else { acc });
        if !is_width {
            next_height = step_or_round(next_height, change.should_change_by_step_size);
        }
        if keep_aspect_ratio {
            if is_width {
                next_height = ((next_width / aspect_ratio) * 100.0).round_js() / 100.0;
            } else {
                next_width = (next_height * aspect_ratio * 100.0).round_js() / 100.0;
            }
        }
        next_height = excali_math::js::max(MIN_WIDTH_OR_HEIGHT, next_height);
        next_width = excali_math::js::max(MIN_WIDTH_OR_HEIGHT, next_width);
        resize_single_element(
            next_width,
            next_height,
            &orig.base.id,
            &self.original,
            scene,
            env,
            handle,
            options,
        );
        let latest = scene.get_non_deleted(&orig.base.id)?.clone();
        if is_frame_like(&latest) {
            return Some(Some(elements_in_resizing_frame(
                scene,
                &latest,
                &self.app_state,
            )));
        }
        None
    }

    // -- Angle.tsx -------------------------------------------------------------------

    fn angle<E: StatsEnv>(
        &self,
        scene: &mut Scene,
        app_state: &AppState,
        change: StatsChange,
        env: &mut E,
    ) {
        let Some(orig) = self.elements.first() else {
            return;
        };
        if is_elbow_arrow(orig) || scene.get_non_deleted(&orig.base.id).is_none() {
            return;
        }
        let next_angle = match change.next_value {
            Some(v) => degrees_to_radians(Degrees(v)),
            None => next_angle_by_drag(orig, change),
        };
        let id = orig.base.id.clone();
        scene.mutate_element(&id, angle_update(next_angle), env);
        let _ = update_bindings(scene, env, &id, &binding_app_state(app_state), None);
        rotate_bound_text(scene, &id, next_angle, env);
    }

    // -- FontSize.tsx ----------------------------------------------------------------

    fn font_size<E: StatsEnv>(&self, scene: &mut Scene, change: StatsChange, env: &mut E) {
        let Some(orig) = self.elements.first() else {
            return;
        };
        let Some(latest) = scene.get_non_deleted(&orig.base.id).cloned() else {
            return;
        };
        if !is_text(&latest) {
            return;
        }
        let next = match change.next_value {
            Some(v) => Some(excali_math::js::max(v.round_js(), MIN_FONT_SIZE)),
            None if is_text(orig) => {
                let map = scene_map(scene);
                let original = get_base_font_size(orig, &map).round_js();
                let mut next = excali_math::js::max(
                    original + change.accumulated_change.round_js(),
                    MIN_FONT_SIZE,
                );
                if change.should_change_by_step_size {
                    next = get_step_sized_value(next, FONT_STEP_SIZE);
                }
                Some(next)
            }
            None => None,
        };
        if let Some(next) = next.filter(|n| *n != 0.0 && !n.is_nan()) {
            set_font_size(scene, &latest.base.id, next, env);
        }
    }

    // -- MultiPosition.tsx -----------------------------------------------------------

    fn multi_position<E: StatsEnv>(
        &self,
        scene: &mut Scene,
        app_state: &AppState,
        change: StatsChange,
        env: &mut E,
    ) {
        let is_x = self.property == StatsProperty::X;
        let original = &self.original;
        if let Some(v) = change.next_value {
            let targets: Vec<&Element> = self.elements.iter().collect();
            for unit in get_atomic_units(&targets, &self.app_state) {
                let in_unit = elements_in_unit(&unit, scene, original);
                if in_unit.len() > 1 {
                    let latest: Vec<&Element> = in_unit.iter().map(|(_, l)| l).collect();
                    let [x1, y1, _, _] = get_common_bounds(&latest);
                    let (nx, ny) = if is_x { (v, y1) } else { (x1, v) };
                    let originals: Vec<Element> = in_unit.iter().map(|(o, _)| o.clone()).collect();
                    move_group_to(nx, ny, &originals, original, scene, app_state, env);
                } else if let Some((orig, latest)) = in_unit.first() {
                    if is_property_editable(latest, self.property) {
                        let [tlx, tly] = top_left(orig);
                        let (nx, ny) = if is_x { (v, tly) } else { (tlx, v) };
                        move_element(nx, ny, orig, scene, app_state, original, env);
                    }
                }
            }
            return;
        }
        let change_value = if change.should_change_by_step_size {
            get_step_sized_value(change.accumulated_change, STEP_SIZE)
        } else {
            change.accumulated_change
        };
        for orig in &self.elements {
            let [tlx, tly] = top_left(orig);
            let nx = if is_x {
                (tlx + change_value).round_js()
            } else {
                tlx
            };
            let ny = if is_x {
                tly
            } else {
                (tly + change_value).round_js()
            };
            move_element(nx, ny, orig, scene, app_state, original, env);
        }
    }

    // -- MultiDimension.tsx ----------------------------------------------------------

    fn multi_dimension<E: StatsEnv>(
        &self,
        scene: &mut Scene,
        change: StatsChange,
        env: &mut E,
    ) -> Option<HighlightPatch> {
        let is_width = self.property == StatsProperty::Width;
        let handle = if is_width {
            TransformHandleDirection::E
        } else {
            TransformHandleDirection::S
        };
        let targets: Vec<&Element> = self.elements.iter().collect();
        let units = get_atomic_units(&targets, &self.app_state);
        let original = &self.original;
        let mut highlight: Vec<String> = Vec::new();
        for unit in units {
            let in_unit = elements_in_unit(&unit, scene, original);
            if in_unit.len() > 1 {
                let originals: Vec<Element> = in_unit.iter().map(|(o, _)| o.clone()).collect();
                let refs: Vec<&Element> = originals.iter().collect();
                let [x1, y1, x2, y2] = get_common_bounds(&refs);
                let initial_width = x2 - x1;
                let initial_height = y2 - y1;
                let aspect_ratio = initial_width / initial_height;
                let (next_width, next_height) = match change.next_value {
                    Some(v) => (
                        excali_math::js::max(
                            MIN_WIDTH_OR_HEIGHT,
                            if is_width {
                                excali_math::js::max(0.0, v)
                            } else {
                                initial_width
                            },
                        ),
                        excali_math::js::max(
                            MIN_WIDTH_OR_HEIGHT,
                            if is_width {
                                initial_height
                            } else {
                                excali_math::js::max(0.0, v)
                            },
                        ),
                    ),
                    None => {
                        let acc = change.accumulated_change;
                        let mut w = excali_math::js::max(
                            0.0,
                            initial_width + if is_width { acc } else { 0.0 },
                        );
                        if is_width {
                            w = step_or_round(w, change.should_change_by_step_size);
                        }
                        let mut h = excali_math::js::max(
                            0.0,
                            initial_height + if is_width { 0.0 } else { acc },
                        );
                        if !is_width {
                            h = step_or_round(h, change.should_change_by_step_size);
                        }
                        (
                            excali_math::js::max(MIN_WIDTH_OR_HEIGHT, w),
                            excali_math::js::max(MIN_WIDTH_OR_HEIGHT, h),
                        )
                    }
                };
                resize_group(
                    next_width,
                    next_height,
                    initial_height,
                    aspect_ratio,
                    [x1, y1],
                    is_width,
                    &originals,
                    original,
                    scene,
                    env,
                );
                continue;
            }
            let Some((orig, latest)) = in_unit.first() else {
                continue;
            };
            if !is_property_editable(latest, self.property) {
                continue;
            }
            let (next_width, next_height) = match change.next_value {
                Some(v) => {
                    let mut w = if is_width {
                        excali_math::js::max(0.0, v)
                    } else {
                        latest.base.width
                    };
                    if is_width {
                        w = step_or_round(w, change.should_change_by_step_size);
                    }
                    let mut h = if is_width {
                        latest.base.height
                    } else {
                        excali_math::js::max(0.0, v)
                    };
                    if !is_width {
                        h = step_or_round(h, change.should_change_by_step_size);
                    }
                    (w, h)
                }
                None => {
                    let acc = change.accumulated_change;
                    let mut w = excali_math::js::max(
                        0.0,
                        orig.base.width + if is_width { acc } else { 0.0 },
                    );
                    if is_width {
                        w = step_or_round(w, change.should_change_by_step_size);
                    }
                    let mut h = excali_math::js::max(
                        0.0,
                        orig.base.height + if is_width { 0.0 } else { acc },
                    );
                    if !is_width {
                        h = step_or_round(h, change.should_change_by_step_size);
                    }
                    (w, h)
                }
            };
            resize_single_element(
                excali_math::js::max(MIN_WIDTH_OR_HEIGHT, next_width),
                excali_math::js::max(MIN_WIDTH_OR_HEIGHT, next_height),
                &orig.base.id,
                original,
                scene,
                env,
                handle,
                ResizeOptions::default(),
            );
            let Some(latest) = scene.get_non_deleted(&orig.base.id).cloned() else {
                continue;
            };
            if is_frame_like(&latest) {
                if change.next_value.is_some() {
                    update_frame_membership(scene, &latest, &self.app_state, env);
                } else {
                    highlight.extend(elements_in_resizing_frame(scene, &latest, &self.app_state));
                }
            }
        }
        if change.next_value.is_some() {
            None
        } else {
            Some(Some(highlight))
        }
    }

    // -- MultiAngle.tsx --------------------------------------------------------------

    fn multi_angle<E: StatsEnv>(&self, scene: &mut Scene, change: StatsChange, env: &mut E) {
        let editable: Vec<&Element> = self
            .elements
            .iter()
            .filter(|el| !is_in_group(el) && is_property_editable(el, self.property))
            .collect();
        for orig in editable {
            let id = orig.base.id.clone();
            let Some(latest) = scene.get_non_deleted(&id) else {
                continue;
            };
            if is_in_group(latest) || !is_property_editable(latest, self.property) {
                continue;
            }
            let next_angle = match change.next_value {
                Some(v) => degrees_to_radians(Degrees(v)),
                None => next_angle_by_drag(orig, change),
            };
            scene.mutate_element(&id, angle_update(next_angle), env);
            rotate_bound_text(scene, &id, next_angle, env);
        }
    }

    // -- MultiFontSize.tsx -----------------------------------------------------------

    fn multi_font_size<E: StatsEnv>(&self, scene: &mut Scene, change: StatsChange, env: &mut E) {
        // `if (nextValue)`: a typed 0 is taken as a drag with no change
        let typed = change.next_value.filter(|v| *v != 0.0 && !v.is_nan());
        for orig in &self.elements {
            let id = orig.base.id.clone();
            if scene.get_non_deleted(&id).is_none() {
                continue;
            }
            let next = match typed {
                Some(v) => excali_math::js::max(v.round_js(), MIN_FONT_SIZE),
                None => {
                    let map = scene_map(scene);
                    let original = get_base_font_size(orig, &map).round_js();
                    let mut next = excali_math::js::max(
                        original + change.accumulated_change.round_js(),
                        MIN_FONT_SIZE,
                    );
                    if change.should_change_by_step_size {
                        next = get_step_sized_value(next, FONT_STEP_SIZE);
                    }
                    next
                }
            };
            set_font_size(scene, &id, next, env);
        }
    }
}

fn step_or_round(value: f64, step: bool) -> f64 {
    if step {
        get_step_sized_value(value, DIMENSION_STEP_SIZE)
    } else {
        value.round_js()
    }
}

fn angle_update(angle: Radians) -> ElementUpdate {
    ElementUpdate {
        angle: Some(angle.0),
        ..ElementUpdate::default()
    }
}

/// The angle a drag gives (`Angle.tsx:61-75`): the original angle in
/// hundredths of a degree plus the whole degrees dragged, modulo 360, on
/// 15 degree steps with Shift, made positive.
fn next_angle_by_drag(orig: &Element, change: StatsChange) -> Radians {
    let original = (radians_to_degrees(Radians(orig.base.angle.0)).0 * 100.0).round_js() / 100.0;
    let mut next = (original + change.accumulated_change.round_js()) % 360.0;
    if change.should_change_by_step_size {
        next = get_step_sized_value(next, ANGLE_STEP_SIZE);
    }
    if next < 0.0 {
        next += 360.0;
    }
    degrees_to_radians(Degrees(next))
}

/// The label of a container turned with it (not an arrow's).
fn rotate_bound_text<E: StatsEnv>(scene: &mut Scene, id: &str, angle: Radians, env: &mut E) {
    let Some(latest) = scene.get_non_deleted(id).cloned() else {
        return;
    };
    let text_id = {
        let map = scene_map(scene);
        get_bound_text_element(&latest, &map).map(|t| t.base.id.clone())
    };
    if let Some(text_id) = text_id {
        if !is_arrow(&latest) {
            scene.mutate_element(&text_id, angle_update(angle), env);
        }
    }
}

/// `scene.mutateElement(text, getBaseFontSizeUpdate(...))` then
/// `redrawTextBoundingBox(text, scene.getContainerElement(text), scene)`.
fn set_font_size<E: StatsEnv>(scene: &mut Scene, id: &str, font_size: f64, env: &mut E) {
    let Some(text) = scene.get_non_deleted(id).cloned() else {
        return;
    };
    let update = {
        let map = scene_map(scene);
        get_base_font_size_update(&text, font_size, &map)
    };
    scene.mutate_element(id, update, env);
    let container = container_id(&text)
        .filter(|c| scene.get(c).is_some())
        .map(str::to_owned);
    env.redraw_text_bounding_box(scene, id, container.as_deref());
}

/// `scene.mutateElement(image, { crop, width, height })`: a new crop object
/// always counts as a change.
fn set_crop<E: StatsEnv>(
    scene: &mut Scene,
    element: &Element,
    crop: excali_core::element::ImageCrop,
    size: Option<[f64; 2]>,
    env: &mut E,
) {
    let mut next = element.clone();
    if let ElementKind::Image(image) = &mut next.kind {
        image.crop = Some(crop);
    }
    if let Some([w, h]) = size {
        next.base.width = w;
        next.base.height = h;
    }
    bump_version(&mut next, None, env);
    scene.replace_element(next);
}

/// `getElementsInResizingFrame(...)` of the scene, as ids of the
/// non-deleted elements.
fn elements_in_resizing_frame(scene: &Scene, frame: &Element, app_state: &AppState) -> Vec<String> {
    let map = scene_map(scene);
    get_elements_in_resizing_frame(scene.elements(), frame, app_state, &map)
        .into_iter()
        .map(|i| &scene.elements()[i])
        .filter(|e| !e.base.is_deleted)
        .map(|e| e.base.id.clone())
        .collect()
}

/// `scene.replaceAllElements(replaceAllElementsInFrame(all,
/// getElementsInResizingFrame(all, frame, originalAppState, map), frame))`.
fn update_frame_membership<E: StatsEnv>(
    scene: &mut Scene,
    frame: &Element,
    app_state: &AppState,
    env: &mut E,
) {
    let in_frame = {
        let map = scene_map(scene);
        get_elements_in_resizing_frame(scene.elements(), frame, app_state, &map)
    };
    let mut next = replace_all_elements_in_frame(scene.elements().to_vec(), &in_frame, frame, env);
    let _ = sync_invalid_indices(&mut next, env);
    *scene = Scene::new(next);
}

/// `moveGroupTo(nextX, nextY, originalElements, originalElementsMap, scene,
/// appState)` (`MultiPosition.tsx:79-125`).
fn move_group_to<E: StatsEnv>(
    next_x: f64,
    next_y: f64,
    originals: &[Element],
    original_elements: &[Element],
    scene: &mut Scene,
    app_state: &AppState,
    env: &mut E,
) {
    let refs: Vec<&Element> = originals.iter().collect();
    let [x1, y1, _, _] = get_common_bounds(&refs);
    let offset_x = next_x - x1;
    let offset_y = next_y - y1;
    for orig in originals {
        let Some(latest) = scene.get_non_deleted(&orig.base.id).cloned() else {
            continue;
        };
        // bound texts are moved with their containers
        if is_text(&latest) && container_id(&latest).is_some() {
            continue;
        }
        let [tlx, tly] = top_left(&latest);
        move_element(
            tlx + offset_x,
            tly + offset_y,
            orig,
            scene,
            app_state,
            original_elements,
            env,
        );
    }
}

/// `resizeGroup(...)` (`MultiDimension.tsx:146-181`): the unit scaled from
/// its top left corner with its aspect ratio kept.
#[allow(clippy::too_many_arguments)]
fn resize_group<E: StatsEnv>(
    next_width: f64,
    next_height: f64,
    initial_height: f64,
    aspect_ratio: f64,
    anchor: [f64; 2],
    is_width: bool,
    originals: &[Element],
    original_elements: &[Element],
    scene: &mut Scene,
    env: &mut E,
) {
    // the width follows the height (upstream computes it and scales by the
    // height alone)
    let next_height = if is_width {
        ((next_width / aspect_ratio) * 100.0).round_js() / 100.0
    } else {
        next_height
    };
    let scale = next_height / initial_height;
    for orig in originals {
        resize_element_in_group(anchor, is_width, scale, orig, original_elements, scene, env);
    }
}

/// `resizeElementInGroup(...)` (`MultiDimension.tsx:84-144`).
fn resize_element_in_group<E: StatsEnv>(
    anchor: [f64; 2],
    is_width: bool,
    scale: f64,
    orig: &Element,
    original_elements: &[Element],
    scene: &mut Scene,
    env: &mut E,
) {
    let id = orig.base.id.as_str();
    let Some(latest) = scene.get_non_deleted(id).cloned() else {
        return;
    };
    if is_text(&latest) && is_sticky_note_bound_text(&latest, &scene_map(scene)) {
        // the note's layout owns its label
        return;
    }
    let handle = if is_width {
        TransformHandleDirection::E
    } else {
        TransformHandleDirection::S
    };
    // getResizedUpdates (MultiDimension.tsx:57-82)
    let next_width = orig.base.width * scale;
    let next_height = orig.base.height * scale;
    let update = ElementUpdate {
        width: Some(next_width),
        height: Some(next_height),
        x: Some(anchor[0] + (orig.base.x - anchor[0]) * scale),
        y: Some(anchor[1] + (orig.base.y - anchor[1]) * scale),
        points: rescale_points_in_element(orig, next_width, next_height, false),
        font_size: match &orig.kind {
            ElementKind::Text(t) => Some(t.font_size * scale),
            _ => None,
        },
        ..ElementUpdate::default()
    };
    scene.mutate_element(id, update, env);

    if is_sticky_note(&latest) {
        let Some(latest) = scene.get(id).cloned() else {
            return;
        };
        let mut opts =
            get_sticky_note_resize_intent(&latest, original_elements, handle, true, false, false);
        opts.anchor = Some(StickyNoteLayoutAnchor::Top);
        update_sticky_note_layout(id, scene, env, opts, Some(None));
        return;
    }

    let original_map = ElementsMap::new(original_elements.iter());
    if let Some(bound_text) = get_bound_text_element(orig, &original_map) {
        let ElementKind::Text(t) = &bound_text.kind else {
            return;
        };
        let new_font_size = t.font_size * scale;
        let text_id = bound_text.base.id.clone();
        env.update_bound_elements(scene, id, None);
        if scene.get_non_deleted(&text_id).is_some_and(is_text) {
            scene.mutate_element(
                &text_id,
                ElementUpdate {
                    font_size: Some(new_font_size),
                    ..ElementUpdate::default()
                },
                env,
            );
            handle_bind_text_resize(id, scene, env, Some(handle), true, false, false);
        }
    }
}
