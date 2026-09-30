//! The `perform`s of the styles panel's actions
//! (`packages/excalidraw/actions/actionProperties.tsx`, with
//! `actionLinearEditor.tsx`'s `togglePolygon`, `actionLink.tsx` and
//! `actionCropEditor.tsx`; the aligns and distributes are in
//! [`super::align`]), behind [`perform_style_action`].
//!
//! Upstream's `perform` gets the scene's element objects and `app.scene`
//! over the same objects: `changeProperty` maps them to the element
//! itself or a copy (`newElementWith`), and the layout the text actions
//! run (`redrawTextBoundingBox`, `updateBoundElements`) mutates the copy it
//! is given and the scene's objects in place, so a later mutation of a
//! scene object shows in the result where the result holds the object
//! itself. The port keeps that: the result is a list of [`Obj`]s, each the
//! scene's element at its position (read when the result is built) or an
//! element of its own, and a copy is laid out in the scene in place of the
//! element it copies, which goes back afterwards.

use std::collections::HashSet;

use excali_core::app_state::AppState;
use excali_core::color::is_transparent;
use excali_core::constants::{
    stroke_width_by_key, DEFAULT_ELEMENT_PROPS, DEFAULT_STICKY_NOTE_BG, DEFAULT_STICKY_NOTE_SIZE,
    MIN_FONT_SIZE, STICKY_NOTE_FALLBACK_FONT_SIZE, STICKY_NOTE_MAX_FONT_SIZE, STICKY_NOTE_MIN_SIZE,
};
use excali_core::element::{
    BindMode, Element, ElementKind, FixedPointBinding, FontFamily, StrokeWidthKey, TextAlign,
};
use excali_core::fractional_index::SceneElementsMap;
use excali_core::restore::RestoreEnv;
use excali_math::js;
use excali_scene::bounds::{get_bound_text_element, ElementsMap};
use excali_text::font_metadata::get_line_height;
use serde_json::{json, Map, Value};

use super::{
    align, get_selected_elements, is_elbow_arrow, is_linear, object_key, truthy, ActionResult,
    EditEnv,
};
use crate::actions::{
    has_fill_style, has_stroke_color, resolve_color_target, ActionContext, ActionEnv, ActionName,
    AppProps, ColorProperty, ColorTarget,
};
use crate::binding::{
    bind_binding_element, calculate_fixed_point_for_elbow_arrow_binding, update_bound_elements,
    BindingEnd, BindingEnv,
};
use crate::elbow_arrow::{self, ElbowArrowUpdates};
use crate::js_value::{deep_equal, is_object, num};
use crate::linear_element_editor::{
    get_point_at_index_global_coordinates, point_from_absolute_coords,
};
use crate::mutate::{mutate_element, new_element_with};
use crate::new_element::is_using_adaptive_radius;
use crate::scene::{MutationEnv, Scene};
use excali_core::fractional_index::ChangeStamp;
use excali_text::text_measurements::{CharWidthCache, TextMetricsProvider};

/// What the property actions draw and measure besides [`EditEnv`]: text
/// layout and arrow routing.
///
/// Upstream's `scene.mutateElement` ends in `triggerUpdate()`, which draws
/// the scene's nonce (`randomInteger()`) right after a changed element's
/// version nonce: the actions draw it (from [`RestoreEnv::random_integer`])
/// for each such mutation they make, and an environment's text layout
/// should too, for the draws to follow upstream's sequence.
pub trait StyleEnv: EditEnv + crate::binding::BindingEnv {
    /// `redrawTextBoundingBox(text, container, scene)` over `elements`.
    fn redraw_text_bounding_box(
        &mut self,
        elements: &mut SceneElementsMap,
        text_id: &str,
        container_id: Option<&str>,
    ) -> Result<(), String>;
}

/// `action.perform(elements, appState, value, app)` for the styles panel's
/// actions (the property actions of `actionProperties.tsx`, `togglePolygon`,
/// the six aligns, the two distributes, `hyperlink` and `cropEditor`), on a
/// scene of `elements` (deleted ones included, in order); `value` is
/// upstream's `value` as JSON (`{"color": "#e03131"}`, `"solid"`, `50`,
/// `{"position": "start", "type": "arrow"}`, changeFontFamily's
/// `cachedElements` an object of element JSON by id). `None` for another
/// action or where upstream returns `false` (or throws: a missing value an
/// `invariant` requires, `cropEditor` with nothing selected).
///
/// changeFontFamily redraws synchronously, as upstream does once the font
/// is loaded (`skipFontFaceCheck || fonts.check(...)`); changeArrowType's
/// `app.dismissLinearEditor()` (a deferred `setState`) is the host's.
pub fn perform_style_action<E: StyleEnv>(
    name: ActionName,
    elements: &[Element],
    app_state: &AppState,
    value: &Value,
    env: &mut E,
) -> Option<ActionResult> {
    use ActionName as N;
    let mut w = Work {
        scene: Scene::new(elements.to_vec()),
        env,
    };
    match name {
        N::ChangeStrokeColor => change_stroke_color(&mut w, app_state, value),
        N::ChangeBackgroundColor => change_background_color(&mut w, app_state, value),
        N::ChangeBucketFillBackgroundColor => Some(ActionResult {
            elements: None,
            app_state: value.as_object().cloned().unwrap_or_default(),
            capture: false,
            never: false,
        }),
        N::ChangeFillStyle => {
            let elements = simple_property(&mut w, app_state, false, |el| {
                has_fill_style(el.element_type().as_str()).then(|| one("fillStyle", value.clone()))
            });
            captured(elements, "currentItemFillStyle", value.clone())
        }
        N::ChangeStrokeWidth => {
            let key = stroke_width_key(value.as_str()?)?;
            let elements = simple_property(&mut w, app_state, false, |el| {
                Some(one(
                    "strokeWidth",
                    num(stroke_width_by_key(el.element_type(), key)),
                ))
            });
            captured(elements, "currentItemStrokeWidthKey", value.clone())
        }
        N::ChangeSloppiness => {
            let slots = change_property(&mut w, app_state, false, |w, i| {
                let el = w.scene.elements()[i].clone();
                // seed: randomInteger(), drawn before the version nonce
                let seed = RestoreEnv::random_integer(w.env);
                let mut updates = one("seed", num(seed));
                updates.insert("roughness".into(), value.clone());
                w.with(i, &el, updates)
            });
            let elements = w.materialize(slots);
            captured(Some(elements), "currentItemRoughness", value.clone())
        }
        N::ChangeFreedrawMode => {
            let variability = if truthy(Some(value)) {
                value.clone()
            } else {
                json!("constant")
            };
            let elements = simple_property(&mut w, app_state, false, |el| {
                if !matches!(el.kind, ElementKind::Freedraw(_)) {
                    return None;
                }
                let mut options = el
                    .to_map()
                    .get("strokeOptions")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                options.insert("variability".into(), variability.clone());
                Some(one("strokeOptions", Value::Object(options)))
            });
            captured(elements, "currentItemStrokeVariability", variability)
        }
        N::ChangeStrokeStyle => {
            let elements = simple_property(&mut w, app_state, false, |_| {
                Some(one("strokeStyle", value.clone()))
            });
            captured(elements, "currentItemStrokeStyle", value.clone())
        }
        N::ChangeOpacity => {
            let elements = simple_property(&mut w, app_state, true, |_| {
                Some(one("opacity", value.clone()))
            });
            captured(elements, "currentItemOpacity", value.clone())
        }
        N::ChangeFontSize => {
            // invariant(value, "actionChangeFontSize: Expected a font size value")
            let size = value.as_f64().filter(|&s| s != 0.0 && !s.is_nan())?;
            Some(change_font_size(
                &mut w,
                app_state,
                &|_, _| size,
                Some(size),
            ))
        }
        N::IncreaseFontSize => Some(change_font_size(
            &mut w,
            app_state,
            &|el, scene| js::round(get_base_font_size(el, scene) * (1.0 + FONT_SIZE_STEP)),
            None,
        )),
        N::DecreaseFontSize => Some(change_font_size(
            &mut w,
            app_state,
            &|el, scene| js::round((1.0 / (1.0 + FONT_SIZE_STEP)) * get_base_font_size(el, scene)),
            None,
        )),
        N::ChangeFontFamily => change_font_family(&mut w, app_state, value),
        N::ChangeTextAlign => {
            let elements = text_property(&mut w, app_state, "textAlign", value);
            captured(Some(elements), "currentItemTextAlign", value.clone())
        }
        N::ChangeVerticalAlign => {
            let elements = text_property(&mut w, app_state, "verticalAlign", value);
            Some(ActionResult {
                elements: Some(elements),
                app_state: Map::new(),
                capture: true,
                never: false,
            })
        }
        N::ChangeRoundness => {
            let round = value.as_str() == Some("round");
            let elements = simple_property(&mut w, app_state, false, |el| {
                if is_elbow_arrow(el) {
                    return None;
                }
                let roundness = if round {
                    let kind = if is_using_adaptive_radius(el.element_type().as_str()) {
                        ROUNDNESS_ADAPTIVE_RADIUS
                    } else {
                        ROUNDNESS_PROPORTIONAL_RADIUS
                    };
                    json!({ "type": kind })
                } else {
                    Value::Null
                };
                Some(one("roundness", roundness))
            });
            captured(elements, "currentItemRoundness", value.clone())
        }
        N::ChangeArrowhead => {
            // invariant(value, "actionChangeArrowhead: value must be defined")
            let value = value.as_object()?;
            let position = value.get("position").and_then(Value::as_str);
            let kind = value.get("type").cloned().unwrap_or(Value::Null);
            let elements = simple_property(&mut w, app_state, false, |el| {
                if !is_linear(el) {
                    return None;
                }
                match position {
                    Some("start") => Some(one("startArrowhead", kind.clone())),
                    Some("end") => Some(one("endArrowhead", kind.clone())),
                    _ => None,
                }
            });
            let key = if position == Some("start") {
                "currentItemStartArrowhead"
            } else {
                "currentItemEndArrowhead"
            };
            captured(elements, key, kind)
        }
        N::ChangeArrowType => Some(change_arrow_type(&mut w, app_state, value)),
        N::TogglePolygon => toggle_polygon(&mut w, app_state),
        N::AlignTop => align::align(&mut w, app_state, align::Alignment::TOP),
        N::AlignBottom => align::align(&mut w, app_state, align::Alignment::BOTTOM),
        N::AlignLeft => align::align(&mut w, app_state, align::Alignment::LEFT),
        N::AlignRight => align::align(&mut w, app_state, align::Alignment::RIGHT),
        N::AlignVerticallyCentered => {
            align::align(&mut w, app_state, align::Alignment::VERTICALLY_CENTERED)
        }
        N::AlignHorizontallyCentered => {
            align::align(&mut w, app_state, align::Alignment::HORIZONTALLY_CENTERED)
        }
        N::DistributeHorizontally => align::distribute(&mut w, app_state, align::Axis::X),
        N::DistributeVertically => align::distribute(&mut w, app_state, align::Axis::Y),
        N::Hyperlink => hyperlink(app_state),
        N::CropEditor => crop_editor(&w, app_state),
        _ => None,
    }
}

/// `FONT_SIZE_RELATIVE_INCREASE_STEP` (`actionProperties.tsx:182`).
const FONT_SIZE_STEP: f64 = 0.1;
/// `ROUNDNESS.PROPORTIONAL_RADIUS` (`constants.ts`).
const ROUNDNESS_PROPORTIONAL_RADIUS: u8 = 2;
/// `ROUNDNESS.ADAPTIVE_RADIUS` (`constants.ts`).
const ROUNDNESS_ADAPTIVE_RADIUS: u8 = 3;
/// `LINE_POLYGON_POINT_MERGE_DISTANCE` (`constants.ts:611`).
const LINE_POLYGON_POINT_MERGE_DISTANCE: f64 = 20.0;
/// `PRECISION` (`math/src/utils.ts:1`), `pointsEqual`'s tolerance.
const PRECISION: f64 = 10e-5;

/// A one-key update.
fn one(key: &str, value: Value) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(key.to_owned(), value);
    m
}

/// `{ elements, appState: { ...appState, [key]: value }, captureUpdate:
/// IMMEDIATELY }`.
fn captured(elements: Option<Vec<Element>>, key: &str, value: Value) -> Option<ActionResult> {
    Some(ActionResult {
        elements,
        app_state: one(key, value),
        capture: true,
        never: false,
    })
}

fn stroke_width_key(key: &str) -> Option<StrokeWidthKey> {
    match key {
        "thin" => Some(StrokeWidthKey::Thin),
        "medium" => Some(StrokeWidthKey::Medium),
        "bold" => Some(StrokeWidthKey::Bold),
        _ => None,
    }
}

/// The environment of a `scene.mutateElement`: each version nonce drawn is
/// followed by the scene's (`triggerUpdate()`, `Scene.ts:303-309`).
pub(super) struct Triggering<'a, E>(pub(super) &'a mut E);

impl<E: StyleEnv> Triggering<'_, E> {
    fn scene_nonce(&mut self) {
        let _ = RestoreEnv::random_integer(self.0);
    }
}

impl<E: StyleEnv> ChangeStamp for Triggering<'_, E> {
    fn version_nonce(&mut self) -> f64 {
        let nonce = self.0.version_nonce();
        self.scene_nonce();
        nonce
    }

    fn updated(&mut self) -> f64 {
        self.0.updated()
    }
}

impl<E: StyleEnv> MutationEnv for Triggering<'_, E> {
    fn random_integer(&mut self) -> f64 {
        let nonce = MutationEnv::random_integer(self.0);
        self.scene_nonce();
        nonce
    }

    fn now(&mut self) -> f64 {
        MutationEnv::now(self.0)
    }
}

impl<E: StyleEnv> BindingEnv for Triggering<'_, E> {
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        self.0.text()
    }
}

/// An element of the result: the scene's element at a position (as the
/// scene holds it when the result is read), or an element of its own (a
/// copy upstream made).
pub(super) enum Obj {
    Scene(usize),
    Own(Box<Element>),
}

/// `app.scene` over the elements `perform` was given, and the environment.
pub(super) struct Work<'e, E> {
    pub(super) scene: Scene,
    pub(super) env: &'e mut E,
}

impl<E: StyleEnv> Work<'_, E> {
    pub(super) fn get<'a>(&'a self, obj: &'a Obj) -> &'a Element {
        match obj {
            Obj::Scene(i) => &self.scene.elements()[*i],
            Obj::Own(e) => e,
        }
    }

    /// The result's elements, the scene's as they are now.
    pub(super) fn materialize(&self, slots: Vec<Obj>) -> Vec<Element> {
        slots
            .into_iter()
            .map(|obj| match obj {
                Obj::Scene(i) => self.scene.elements()[i].clone(),
                Obj::Own(e) => *e,
            })
            .collect()
    }

    /// `newElementWith(element, updates)` of the scene's element `i` (or
    /// `element`, a copy of it): the element itself when no value differs
    /// (an object value always does), else a copy with the version bumped.
    fn with(&mut self, i: usize, element: &Element, updates: Map<String, Value>) -> Obj {
        match element_with(element, updates, self.env) {
            Some(next) => Obj::Own(Box::new(next)),
            None => Obj::Scene(i),
        }
    }

    /// The scene's elements as a map, deleted ones included.
    fn scene_map(&self) -> SceneElementsMap {
        self.scene
            .elements()
            .iter()
            .map(|e| (e.base.id.clone(), e.clone()))
            .collect()
    }

    /// The scene's non-deleted elements as a map (`getNonDeletedElementsMap`).
    fn non_deleted_map(&self) -> SceneElementsMap {
        self.scene
            .elements()
            .iter()
            .filter(|e| !e.base.is_deleted)
            .map(|e| (e.base.id.clone(), e.clone()))
            .collect()
    }

    /// The scene takes `map`'s elements that differ.
    fn write_back(&mut self, map: SceneElementsMap) {
        for (id, element) in map {
            if self.scene.get(&id).is_some_and(|e| *e != element) {
                self.scene.replace_element(element);
            }
        }
    }

    /// `scene.getContainerElement(text)`: the scene's element (deleted or
    /// not) the text's `containerId` names.
    fn container_id(&self, text: &Element) -> Option<String> {
        match &text.kind {
            ElementKind::Text(t) => t
                .container_id
                .as_deref()
                .filter(|c| !c.is_empty() && self.scene.get(c).is_some())
                .map(str::to_owned),
            _ => None,
        }
    }

    /// `redrawTextBoundingBox(text, container, scene)`: the text laid out
    /// in place (a copy in the scene in place of the element it copies),
    /// the container and the arrows the layout moves in the scene.
    pub(super) fn redraw(&mut self, obj: &mut Obj, container: Option<&str>) {
        let text = self.get(obj).clone();
        let id = text.base.id.clone();
        let original = self.scene.get(&id).cloned();
        let mut map = self.scene_map();
        map.insert(id.clone(), text);
        if self
            .env
            .redraw_text_bounding_box(&mut map, &id, container)
            .is_err()
        {
            return;
        }
        if let Obj::Own(own) = obj {
            if let Some(laid_out) = map.get(&id) {
                **own = laid_out.clone();
            }
            if let Some(original) = original {
                map.insert(id, original);
            }
        }
        self.write_back(map);
    }

    /// `scene.mutateElement(element, updates)` of an [`Obj`].
    fn mutate(&mut self, obj: &mut Obj, updates: Map<String, Value>) {
        let elements = self.non_deleted_map();
        match obj {
            Obj::Own(element) => {
                let _ = mutate_element(element, &elements, updates, &mut Triggering(self.env));
            }
            Obj::Scene(i) => {
                let mut element = self.scene.elements()[*i].clone();
                if mutate_element(&mut element, &elements, updates, &mut Triggering(self.env))
                    .unwrap_or(false)
                {
                    self.scene.replace_element(element);
                }
            }
        }
    }

    /// `scene.mutateElement(sceneElement, updates)` by id.
    fn mutate_scene(&mut self, id: &str, updates: Map<String, Value>) {
        let elements = self.non_deleted_map();
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        if mutate_element(&mut element, &elements, updates, &mut Triggering(self.env))
            .unwrap_or(false)
        {
            self.scene.replace_element(element);
        }
    }
}

/// `newElementWith(element, updates)` (`mutateElement.ts:149-181`): `None`
/// when upstream returns the element itself.
fn element_with<E: StyleEnv>(
    element: &Element,
    updates: Map<String, Value>,
    env: &mut E,
) -> Option<Element> {
    let map = element.to_map();
    let changed = updates.iter().any(|(key, value)| {
        is_object(value) || !map.get(key).is_some_and(|c| deep_equal(c, value))
    });
    if !changed {
        return None;
    }
    new_element_with(element, updates, true, env).ok()
}

/// `appState.editingTextElement?.id`.
fn editing_text_id(app_state: &AppState) -> Option<String> {
    app_state
        .get("editingTextElement")
        .and_then(|e| e.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The ids `getSelectedElements(elements, appState, {
/// includeBoundTextElement })` gives over the scene.
fn selected_ids(scene: &Scene, app_state: &AppState, include_bound_text: bool) -> Vec<String> {
    let live = scene.non_deleted();
    get_selected_elements(
        &live,
        &object_key(app_state, "selectedElementIds"),
        include_bound_text,
        false,
    )
    .into_iter()
    .map(|e| e.base.id.clone())
    .collect()
}

/// `changeProperty(elements, appState, callback, includeBoundText)`
/// (`actionProperties.tsx:193-227`): the selected elements (with their
/// bound text) and the text being edited through `callback`, a sticky note
/// then normalised (`normalizeStickyNote`); the rest as they are.
fn change_property<E: StyleEnv>(
    w: &mut Work<'_, E>,
    app_state: &AppState,
    include_bound_text: bool,
    mut callback: impl FnMut(&mut Work<'_, E>, usize) -> Obj,
) -> Vec<Obj> {
    let selected: HashSet<String> = selected_ids(&w.scene, app_state, include_bound_text)
        .into_iter()
        .collect();
    let editing = editing_text_id(app_state);
    let mut slots = Vec::with_capacity(w.scene.elements().len());
    for i in 0..w.scene.elements().len() {
        let id = &w.scene.elements()[i].base.id;
        if !(selected.contains(id) || editing.as_deref() == Some(id.as_str())) {
            slots.push(Obj::Scene(i));
            continue;
        }
        let next = callback(w, i);
        let next = if matches!(w.get(&next).kind, ElementKind::StickyNote(_)) {
            let current = w.get(&next).clone();
            match normalize_sticky_note(&current, w.env) {
                Some(normalized) => Obj::Own(Box::new(normalized)),
                None => next,
            }
        } else {
            next
        };
        slots.push(next);
    }
    slots
}

/// `changeProperty` with `newElementWith(el, update(el))` for the elements
/// `update` answers, the result's elements.
fn simple_property<E: StyleEnv>(
    w: &mut Work<'_, E>,
    app_state: &AppState,
    include_bound_text: bool,
    update: impl Fn(&Element) -> Option<Map<String, Value>>,
) -> Option<Vec<Element>> {
    let slots = change_property(w, app_state, include_bound_text, |w, i| {
        let el = w.scene.elements()[i].clone();
        match update(&el) {
            Some(updates) => w.with(i, &el, updates),
            None => Obj::Scene(i),
        }
    });
    Some(w.materialize(slots))
}

// -- sticky notes (packages/element/src/stickyNote.ts, newElement.ts) ----------

/// `normalizeStickyNoteStrokeColor(strokeColor)` (`stickyNote.ts:67-73`).
fn normalize_sticky_note_stroke_color(color: Option<&str>) -> String {
    match color {
        Some(c) if !c.is_empty() && !is_transparent(c) => c.to_owned(),
        _ => DEFAULT_ELEMENT_PROPS.stroke_color.to_owned(),
    }
}

/// `normalizeStickyNoteBackgroundColor(backgroundColor)`
/// (`stickyNote.ts:75-81`).
fn normalize_sticky_note_background_color(color: Option<&str>) -> String {
    match color {
        Some(c) if !c.is_empty() && !is_transparent(c) => c.to_owned(),
        _ => DEFAULT_STICKY_NOTE_BG.to_owned(),
    }
}

/// `normalizeStickyNoteFontSize(fontSize)` (`stickyNote.ts:379-384`).
fn normalize_sticky_note_font_size(size: f64) -> f64 {
    if !size.is_finite() {
        return STICKY_NOTE_FALLBACK_FONT_SIZE;
    }
    js::min(STICKY_NOTE_MAX_FONT_SIZE, js::max(MIN_FONT_SIZE, size))
}

/// `normalizeStickyNote(element)` (`newElement.ts:224-228`): the style
/// (never-transparent colours, solid fill; `normalizeStickyNoteStyle`) then
/// the geometry (the minimum size, `baseHeight <= height`;
/// `normalizeStickyNoteGeometry`), each a `newElementWith`; `None` when
/// both leave the note as it is.
fn normalize_sticky_note<E: StyleEnv>(note: &Element, env: &mut E) -> Option<Element> {
    let mut style = Map::new();
    style.insert(
        "backgroundColor".into(),
        json!(normalize_sticky_note_background_color(Some(
            &note.base.background_color
        ))),
    );
    style.insert(
        "strokeColor".into(),
        json!(normalize_sticky_note_stroke_color(Some(
            &note.base.stroke_color
        ))),
    );
    style.insert("fillStyle".into(), json!("solid"));
    let styled = element_with(note, style, env);
    let current = styled.as_ref().unwrap_or(note);
    let base_height = match &current.kind {
        ElementKind::StickyNote(s) => s.base_height,
        _ => 0.0,
    };
    let or = |v: f64| v != 0.0 && !v.is_nan();
    let base_height = js::max(
        if or(base_height) {
            base_height
        } else if or(current.base.height) {
            current.base.height
        } else {
            DEFAULT_STICKY_NOTE_SIZE
        },
        STICKY_NOTE_MIN_SIZE,
    );
    let mut geometry = Map::new();
    geometry.insert(
        "width".into(),
        num(js::max(current.base.width, STICKY_NOTE_MIN_SIZE)),
    );
    geometry.insert(
        "height".into(),
        num(js::max(current.base.height, base_height)),
    );
    geometry.insert("baseHeight".into(), num(base_height));
    match element_with(current, geometry, env) {
        Some(next) => Some(next),
        None => styled,
    }
}

fn is_sticky_note(element: Option<&Element>) -> bool {
    element.is_some_and(|e| matches!(e.kind, ElementKind::StickyNote(_)))
}

/// A text's `containerId`, when truthy.
fn text_container_id(element: &Element) -> Option<&str> {
    match &element.kind {
        ElementKind::Text(t) => t.container_id.as_deref().filter(|c| !c.is_empty()),
        _ => None,
    }
}

/// `isStickyNoteBoundText(text, elementsMap)` (`stickyNote.ts:387-395`).
fn is_sticky_note_bound_text(element: &Element, map: &ElementsMap<'_>) -> bool {
    text_container_id(element).is_some_and(|c| is_sticky_note(map.get(c)))
}

/// `getColorTargetElement(element, property, elementsMap)`
/// (`stickyNote.ts:95-108`): a background pick on a note's label lands on
/// the note.
fn color_target_element<'a>(
    element: &'a Element,
    property: ColorProperty,
    map: &ElementsMap<'a>,
) -> &'a Element {
    if property == ColorProperty::BackgroundColor
        && matches!(element.kind, ElementKind::Text(_))
        && is_sticky_note_bound_text(element, map)
    {
        return text_container_id(element)
            .and_then(|c| map.get(c))
            .unwrap_or(element);
    }
    element
}

/// `getColorUpdate(element, property, color, elementsMap)`
/// (`stickyNote.ts:110-137`): a note's colours are never transparent, and
/// a note's label keeps the note's ink rather than going transparent.
fn color_update(
    element: &Element,
    property: ColorProperty,
    color: &str,
    map: &ElementsMap<'_>,
) -> Map<String, Value> {
    if matches!(element.kind, ElementKind::StickyNote(_)) {
        return match property {
            ColorProperty::BackgroundColor => one(
                "backgroundColor",
                json!(normalize_sticky_note_background_color(Some(color))),
            ),
            ColorProperty::StrokeColor => one(
                "strokeColor",
                json!(normalize_sticky_note_stroke_color(Some(color))),
            ),
        };
    }
    if property == ColorProperty::StrokeColor
        && matches!(element.kind, ElementKind::Text(_))
        && is_sticky_note_bound_text(element, map)
    {
        let container = text_container_id(element).and_then(|c| map.get(c));
        let ink = if is_transparent(color) {
            normalize_sticky_note_stroke_color(container.map(|c| c.base.stroke_color.as_str()))
        } else {
            color.to_owned()
        };
        return one("strokeColor", json!(ink));
    }
    match property {
        ColorProperty::BackgroundColor => one("backgroundColor", json!(color)),
        ColorProperty::StrokeColor => one("strokeColor", json!(color)),
    }
}

/// `getColorTargetAppStateUpdates(target, color)` (`colorTargets.ts:178-192`).
fn color_target_app_state_updates(target: &ColorTarget, color: &str) -> Map<String, Value> {
    let mut updates = Map::new();
    for key in &target.app_state_keys {
        let value = match *key {
            "currentItemStickynoteStrokeColor" => normalize_sticky_note_stroke_color(Some(color)),
            "currentItemStickynoteBackgroundColor" => {
                normalize_sticky_note_background_color(Some(color))
            }
            _ => color.to_owned(),
        };
        updates.insert((*key).to_owned(), json!(value));
    }
    updates
}

/// `syncStickyNoteInk(elements, prevElementsMap)` (`stickyNote.ts:146-188`):
/// a note and its label share one ink; the side that changed is copied
/// onto the other (the label wins when both or neither did, a transparent
/// label takes the note's).
fn sync_sticky_note_ink<E: StyleEnv>(
    elements: Vec<Element>,
    prev: &ElementsMap<'_>,
    env: &mut E,
) -> Vec<Element> {
    let mut ink: Vec<(String, String)> = Vec::new();
    {
        let map = ElementsMap::new(elements.iter());
        for container in &elements {
            if !matches!(container.kind, ElementKind::StickyNote(_)) || container.base.is_deleted {
                continue;
            }
            let Some(label) = get_bound_text_element(container, &map) else {
                continue;
            };
            if label.base.stroke_color == container.base.stroke_color {
                continue;
            }
            let container_changed = prev.get(&container.base.id).map(|e| &e.base.stroke_color)
                != Some(&container.base.stroke_color);
            let label_changed = prev.get(&label.base.id).map(|e| &e.base.stroke_color)
                != Some(&label.base.stroke_color);
            let entry = if is_transparent(&label.base.stroke_color)
                || (container_changed && !label_changed)
            {
                (
                    label.base.id.clone(),
                    normalize_sticky_note_stroke_color(Some(&container.base.stroke_color)),
                )
            } else {
                (
                    container.base.id.clone(),
                    normalize_sticky_note_stroke_color(Some(&label.base.stroke_color)),
                )
            };
            match ink.iter_mut().find(|(id, _)| *id == entry.0) {
                Some(slot) => slot.1 = entry.1,
                None => ink.push(entry),
            }
        }
    }
    if ink.is_empty() {
        return elements;
    }
    elements
        .into_iter()
        .map(
            |element| match ink.iter().find(|(id, _)| *id == element.base.id) {
                Some((_, color)) => {
                    element_with(&element, one("strokeColor", json!(color)), env).unwrap_or(element)
                }
                None => element,
            },
        )
        .collect()
}

// -- the colour actions -----------------------------------------------------------

/// The app state keys of the value other than `color`, and the colour.
fn split_color(value: &Value) -> (Option<String>, Map<String, Value>) {
    let mut rest = value.as_object().cloned().unwrap_or_default();
    let color = match rest.remove("color") {
        Some(Value::String(c)) => Some(c),
        _ => None,
    };
    (color, rest)
}

fn action_context<'a>(
    elements: &'a [Element],
    app_state: &'a AppState,
    props: &'a AppProps,
    env: &'a ActionEnv,
) -> ActionContext<'a> {
    ActionContext {
        elements,
        app_state,
        props,
        env,
    }
}

/// `actionChangeStrokeColor.perform` (`actionProperties.tsx:364-401`):
/// without a colour the value's app state keys only (not captured); else
/// the selection with its bound text and the text being edited take the
/// colour under the sticky note policy ([`color_update`]), notes and their
/// labels share their ink ([`sync_sticky_note_ink`]), and the colour
/// target's current item keys take the colour.
fn change_stroke_color<E: StyleEnv>(
    w: &mut Work<'_, E>,
    app_state: &AppState,
    value: &Value,
) -> Option<ActionResult> {
    let (color, mut patch) = split_color(value);
    let Some(color) = color else {
        return Some(ActionResult {
            elements: None,
            app_state: patch,
            capture: false,
            never: false,
        });
    };
    let original = w.scene.elements().to_vec();
    let (props, action_env) = (AppProps::default(), ActionEnv::default());
    let target = resolve_color_target(
        &action_context(&original, app_state, &props, &action_env),
        ColorProperty::StrokeColor,
    );
    let map = ElementsMap::new(original.iter());
    let slots = change_property(w, app_state, true, |w, i| {
        let el = w.scene.elements()[i].clone();
        if !has_stroke_color(el.element_type().as_str()) {
            return Obj::Scene(i);
        }
        let updates = color_update(&el, ColorProperty::StrokeColor, &color, &map);
        w.with(i, &el, updates)
    });
    let elements = w.materialize(slots);
    let elements = sync_sticky_note_ink(elements, &map, w.env);
    patch.extend(color_target_app_state_updates(&target, &color));
    Some(ActionResult {
        elements: Some(elements),
        app_state: patch,
        capture: true,
        never: false,
    })
}

/// `canBecomePolygon(points)` (`typeChecks.ts:403-411`).
fn can_become_polygon(points: &[[f64; 2]]) -> bool {
    let n = points.len();
    n > 3
        || (n == 3
            && !((points[0][0] - points[n - 1][0]).abs() < PRECISION
                && (points[0][1] - points[n - 1][1]).abs() < PRECISION))
}

fn line_points(element: &Element) -> Option<&[[f64; 2]]> {
    match &element.kind {
        ElementKind::Line(l) => Some(&l.linear.points),
        _ => None,
    }
}

/// `toggleLinePolygonState(element, nextPolygonState)` (`shape.ts:1138-1180`):
/// `{ polygon, points }`, the last point on the first (or the first added
/// after the last) when closing; `None` for a line that cannot close.
fn toggle_line_polygon_state(points: &[[f64; 2]], next: bool) -> Option<Map<String, Value>> {
    let mut updated = points.to_vec();
    if next {
        if !can_become_polygon(points) {
            return None;
        }
        let first = updated[0];
        let last = updated[updated.len() - 1];
        let distance = js::hypot(first[0] - last[0], first[1] - last[1]);
        if distance > LINE_POLYGON_POINT_MERGE_DISTANCE || updated.len() < 4 {
            updated.push(first);
        } else {
            let n = updated.len();
            updated[n - 1] = first;
        }
    }
    let mut ret = one("polygon", json!(next));
    ret.insert(
        "points".into(),
        Value::Array(
            updated
                .iter()
                .map(|p| Value::Array(vec![num(p[0]), num(p[1])]))
                .collect(),
        ),
    );
    Some(ret)
}

/// `actionChangeBackgroundColor.perform` (`actionProperties.tsx:443-517`):
/// without a colour the value's app state keys only; a visible colour on
/// lines that can all close closes them into polygons; else the selection
/// (and the text being edited) takes the colour under the sticky note
/// policy, a note's label passing it on to the note.
fn change_background_color<E: StyleEnv>(
    w: &mut Work<'_, E>,
    app_state: &AppState,
    value: &Value,
) -> Option<ActionResult> {
    let (color, mut patch) = split_color(value);
    let Some(color) = color else {
        return Some(ActionResult {
            elements: None,
            app_state: patch,
            capture: false,
            never: false,
        });
    };
    let original = w.scene.elements().to_vec();
    let (props, action_env) = (AppProps::default(), ActionEnv::default());
    let target = resolve_color_target(
        &action_context(&original, app_state, &props, &action_env),
        ColorProperty::BackgroundColor,
    );
    let map = ElementsMap::new(original.iter());
    let selected: Vec<String> = selected_ids(&w.scene, app_state, false);
    let should_enable_polygon = !is_transparent(&color)
        && !selected.is_empty()
        && selected.iter().all(|id| {
            w.scene
                .get(id)
                .and_then(line_points)
                .is_some_and(can_become_polygon)
        });
    let elements = if should_enable_polygon {
        let mut out = Vec::with_capacity(original.len());
        for el in &original {
            let next = match line_points(el) {
                Some(points) if selected.contains(&el.base.id) => {
                    let mut updates = one("backgroundColor", json!(color));
                    if let Some(polygon) = toggle_line_polygon_state(points, true) {
                        updates.extend(polygon);
                    }
                    element_with(el, updates, w.env)
                }
                _ => None,
            };
            out.push(next.unwrap_or_else(|| el.clone()));
        }
        out
    } else {
        let slots = change_property(w, app_state, false, |w, i| {
            let el = w.scene.elements()[i].clone();
            if color_target_element(&el, ColorProperty::BackgroundColor, &map)
                .base
                .id
                != el.base.id
            {
                return Obj::Scene(i);
            }
            let updates = color_update(&el, ColorProperty::BackgroundColor, &color, &map);
            w.with(i, &el, updates)
        });
        let mut elements = w.materialize(slots);
        // editing a note's label (no selection): the pick colours the note
        let editing = editing_text_id(app_state).and_then(|id| map.get(&id));
        if let Some(editing) = editing {
            let target = color_target_element(editing, ColorProperty::BackgroundColor, &map);
            if target.base.id != editing.base.id {
                let target_id = target.base.id.clone();
                for el in elements.iter_mut() {
                    if el.base.id == target_id {
                        let updates =
                            color_update(el, ColorProperty::BackgroundColor, &color, &map);
                        if let Some(next) = element_with(el, updates, w.env) {
                            *el = next;
                        }
                    }
                }
            }
        }
        elements
    };
    patch.extend(color_target_app_state_updates(&target, &color));
    Some(ActionResult {
        elements: Some(elements),
        app_state: patch,
        capture: true,
        never: false,
    })
}

// -- text -----------------------------------------------------------------------------

/// `getBaseFontSize(text, elementsMap)` (`stickyNote.ts:405-413`) over the
/// scene's non-deleted elements: a note's label's `baseFontSize` (else its
/// `fontSize`), any other text's `fontSize`.
fn get_base_font_size(element: &Element, scene: &Scene) -> f64 {
    let ElementKind::Text(t) = &element.kind else {
        return 0.0;
    };
    if is_sticky_note(text_container_id(element).and_then(|c| scene.get_non_deleted(c))) {
        t.base_font_size.unwrap_or(t.font_size)
    } else {
        t.font_size
    }
}

/// `getBaseFontSizeUpdate(text, fontSize, elementsMap)`
/// (`stickyNote.ts:415-423`).
fn base_font_size_update(element: &Element, size: f64, scene: &Scene) -> Map<String, Value> {
    if is_sticky_note(text_container_id(element).and_then(|c| scene.get_non_deleted(c))) {
        one("baseFontSize", num(normalize_sticky_note_font_size(size)))
    } else {
        one("fontSize", num(size))
    }
}

type FontSizeFn<'a> = &'a dyn Fn(&Element, &Scene) -> f64;

/// `changeFontSize(elements, appState, app, getNewFontSize, fallbackValue)`
/// (`actionProperties.tsx:294-354`): each selected text (bound ones with
/// their container) takes its new size (a note's label as its ceiling),
/// is laid out again and, when it grows with its content and is free,
/// keeps its alignment's edge and its vertical centre; the arrows bound
/// to the texts follow. The current item's size is the one size set, else
/// the fallback.
fn change_font_size<E: StyleEnv>(
    w: &mut Work<'_, E>,
    app_state: &AppState,
    get_new_font_size: FontSizeFn<'_>,
    fallback: Option<f64>,
) -> ActionResult {
    let mut sizes: Vec<f64> = Vec::new();
    let selected = selected_ids(&w.scene, app_state, true);
    let slots = change_property(w, app_state, true, |w, i| {
        let old = w.scene.elements()[i].clone();
        if !matches!(old.kind, ElementKind::Text(_)) {
            return Obj::Scene(i);
        }
        let size = get_new_font_size(&old, &w.scene);
        if !sizes.contains(&size) {
            sizes.push(size);
        }
        let container = w.container_id(&old);
        let updates = base_font_size_update(&old, size, &w.scene);
        let mut next = w.with(i, &old, updates);
        w.redraw(&mut next, container.as_deref());
        // offsetElementAfterFontResize(prevElement, nextElement, scene)
        let prev = match &next {
            Obj::Scene(_) => w.get(&next).clone(),
            Obj::Own(_) => old,
        };
        let (auto_resize, bound) = match &w.get(&next).kind {
            ElementKind::Text(t) => (t.auto_resize, t.container_id.is_some()),
            _ => (false, true),
        };
        if bound || !auto_resize {
            return next;
        }
        let align = match &prev.kind {
            ElementKind::Text(t) => t.text_align,
            _ => TextAlign::Left,
        };
        let (next_width, next_height) = {
            let n = w.get(&next);
            (n.base.width, n.base.height)
        };
        let p = &prev.base;
        let x = if align == TextAlign::Left {
            p.x
        } else {
            let divisor = if align == TextAlign::Center { 2.0 } else { 1.0 };
            p.x + (p.width - next_width) / divisor
        };
        let y = p.y + (p.height - next_height) / 2.0;
        let mut updates = one("x", num(x));
        updates.insert("y".into(), num(y));
        w.mutate(&mut next, updates);
        next
    });
    // the arrows bound to the texts follow
    for id in &selected {
        if w.scene
            .get(id)
            .is_some_and(|e| matches!(e.kind, ElementKind::Text(_)))
        {
            update_bound_elements(&mut w.scene, &mut Triggering(w.env), id, None, None);
        }
    }
    let elements = w.materialize(slots);
    let mut patch = Map::new();
    match (sizes.as_slice(), fallback) {
        ([size], _) => {
            patch.insert("currentItemFontSize".into(), num(*size));
        }
        (_, Some(fallback)) => {
            patch.insert("currentItemFontSize".into(), num(fallback));
        }
        _ => {}
    }
    ActionResult {
        elements: Some(elements),
        app_state: patch,
        capture: true,
        never: false,
    }
}

/// `actionChangeTextAlign.perform` and `actionChangeVerticalAlign.perform`
/// (`actionProperties.tsx:1551-1580`, :1652-1681): each selected text
/// (bound ones with their container) takes the alignment and is laid out
/// again.
fn text_property<E: StyleEnv>(
    w: &mut Work<'_, E>,
    app_state: &AppState,
    key: &str,
    value: &Value,
) -> Vec<Element> {
    let slots = change_property(w, app_state, true, |w, i| {
        let old = w.scene.elements()[i].clone();
        if !matches!(old.kind, ElementKind::Text(_)) {
            return Obj::Scene(i);
        }
        let mut next = w.with(i, &old, one(key, value.clone()));
        let container = w.container_id(&old);
        w.redraw(&mut next, container.as_deref());
        next
    });
    w.materialize(slots)
}

/// `actionChangeFontFamily.perform` (`actionProperties.tsx:1167-1545`).
///
/// - `resetAll`: the selection (with bound text) and the text being
///   edited back to the cached elements; never captured.
/// - A picked family (`currentItemFontFamily`, captured) or a hovered one
///   (`currentHoveredFontFamily`, captured eventually; nothing is laid out
///   for over 200 selected texts or over 5000 characters of them): each
///   selected text of another family (every one when picked) takes the
///   family and its line height, its container back to the cached one on
///   `resetContainers`, then each is laid out again.
///
/// The value's other keys are set on the app state.
fn change_font_family<E: StyleEnv>(
    w: &mut Work<'_, E>,
    app_state: &AppState,
    value: &Value,
) -> Option<ActionResult> {
    let mut rest = value.as_object().cloned().unwrap_or_default();
    let cached = rest
        .remove("cachedElements")
        .and_then(|c| c.as_object().cloned());
    let reset_all = truthy(rest.remove("resetAll").as_ref());
    let reset_containers = truthy(rest.remove("resetContainers").as_ref());
    let cached_element = |id: &str| cached.as_ref().and_then(|c| c.get(id)).cloned();
    if reset_all {
        let slots = change_property(w, app_state, true, |w, i| {
            let el = w.scene.elements()[i].clone();
            match cached_element(&el.base.id).and_then(|c| c.as_object().cloned()) {
                Some(cached) => w.with(i, &el, cached),
                None => Obj::Scene(i),
            }
        });
        return Some(ActionResult {
            elements: Some(w.materialize(slots)),
            app_state: rest,
            capture: false,
            never: true,
        });
    }
    let current_item = value
        .get("currentItemFontFamily")
        .filter(|v| truthy(Some(v)));
    let hovered = value
        .get("currentHoveredFontFamily")
        .filter(|v| truthy(Some(v)));
    let (next_family, capture) = match (current_item, hovered) {
        (Some(family), _) => (Some(family.clone()), true),
        (None, Some(family)) => (Some(family.clone()), false),
        (None, None) => (None, false),
    };
    let mut skip_on_hover_render = false;
    if current_item.is_none() && hovered.is_some() {
        let live = w.scene.non_deleted();
        let texts: Vec<&Element> = get_selected_elements(
            &live,
            &object_key(app_state, "selectedElementIds"),
            true,
            false,
        )
        .into_iter()
        .filter(|e| matches!(e.kind, ElementKind::Text(_)))
        .collect();
        if texts.len() > 200 {
            skip_on_hover_render = true;
        } else {
            let mut length = 0;
            for text in &texts {
                if length >= 5000 {
                    break;
                }
                if let ElementKind::Text(t) = &text.kind {
                    length += t.original_text.encode_utf16().count();
                }
            }
            if length > 5000 {
                skip_on_hover_render = true;
            }
        }
    }
    let mut result = ActionResult {
        elements: None,
        app_state: rest,
        capture,
        never: false,
    };
    let Some(family) = next_family.filter(|_| !skip_on_hover_render) else {
        return Some(result);
    };
    let family_number = family.as_f64().unwrap_or(f64::NAN);
    let line_height = get_line_height(FontFamily(family_number as u32));
    let force = current_item.is_some();
    let mut mapping: Vec<(usize, Option<String>)> = Vec::new();
    let mut slots = change_property(w, app_state, true, |w, i| {
        let old = w.scene.elements()[i].clone();
        let ElementKind::Text(t) = &old.kind else {
            return Obj::Scene(i);
        };
        if f64::from(t.font_family.0) == family_number && !force {
            return Obj::Scene(i);
        }
        let mut updates = one("fontFamily", family.clone());
        updates.insert("lineHeight".into(), num(line_height));
        let next = w.with(i, &old, updates);
        let container = w.container_id(&old);
        if reset_containers {
            if let Some(container) = &container {
                // cachedElements?.get(containerId || "") || {}
                let cached = cached_element(container)
                    .and_then(|c| c.as_object().cloned())
                    .unwrap_or_default();
                w.mutate_scene(container, cached);
            }
        }
        mapping.push((i, container));
        next
    });
    // the font is loaded: laid out at once
    for (i, container) in mapping {
        let mut obj = std::mem::replace(&mut slots[i], Obj::Scene(i));
        w.redraw(&mut obj, container.as_deref());
        slots[i] = obj;
    }
    result.elements = Some(w.materialize(slots));
    Some(result)
}

// -- arrows and lines ---------------------------------------------------------------

fn binding_of(element: &Element, end: BindingEnd) -> Option<FixedPointBinding> {
    let linear = element.kind.linear()?;
    match end {
        BindingEnd::Start => linear.start_binding.clone(),
        BindingEnd::End => linear.end_binding.clone(),
    }
}

fn binding_value(binding: &Option<FixedPointBinding>) -> Value {
    match binding {
        None => Value::Null,
        Some(b) => serde_json::to_value(b).unwrap_or(Value::Null),
    }
}

/// `actionChangeArrowType.perform` (`actionProperties.tsx:2061-2226`):
/// each selected arrow becomes sharp, round or elbowed. An elbow arrow is
/// rebuilt from its first and last points (its bindings' fixed points
/// computed for an elbow arrow, `calculateFixedPointForElbowArrowBinding`,
/// and routed, `updateElbowArrowPoints`); any other arrow binds its bound
/// ends again (`bindBindingElement`, orbiting unless the bind mode is
/// inside). The linear element editor of the selected arrow is rebuilt.
fn change_arrow_type<E: StyleEnv>(
    w: &mut Work<'_, E>,
    app_state: &AppState,
    value: &Value,
) -> ActionResult {
    let arrow_type = value.as_str().unwrap_or("");
    let elbow = arrow_type == "elbow";
    let zoom = app_state.zoom().unwrap_or(1.0);
    let binding_enabled = app_state
        .get("isBindingEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let bind_mode = if app_state.get("bindMode").and_then(Value::as_str) == Some("inside") {
        BindMode::Inside
    } else {
        BindMode::Orbit
    };
    let slots = change_property(w, app_state, false, |w, i| {
        let el = w.scene.elements()[i].clone();
        if !matches!(el.kind, ElementKind::Arrow(_)) {
            return Obj::Scene(i);
        }
        let (start_point, points) = {
            let live = w.scene.non_deleted();
            let map = ElementsMap::new(live.iter().copied());
            let start = get_point_at_index_global_coordinates(&el, 0, &map);
            let end = get_point_at_index_global_coordinates(&el, -1, &map);
            let points = if elbow || is_elbow_arrow(&el) {
                let mut moved = el.clone();
                moved.base.x = start[0];
                moved.base.y = start[1];
                moved.base.angle.0 = 0.0;
                let a = point_from_absolute_coords(&moved, start, &map);
                let b = point_from_absolute_coords(&moved, end, &map);
                json!([[num(a[0]), num(a[1])], [num(b[0]), num(b[1])]])
            } else {
                el.to_map().get("points").cloned().unwrap_or(Value::Null)
            };
            (start, points)
        };
        let mut updates = Map::new();
        updates.insert(
            "x".into(),
            num(if elbow { start_point[0] } else { el.base.x }),
        );
        updates.insert(
            "y".into(),
            num(if elbow { start_point[1] } else { el.base.y }),
        );
        updates.insert(
            "roundness".into(),
            if arrow_type == "round" {
                json!({ "type": ROUNDNESS_PROPORTIONAL_RADIUS })
            } else {
                Value::Null
            },
        );
        updates.insert("elbowed".into(), json!(elbow));
        updates.insert(
            "angle".into(),
            num(if elbow { 0.0 } else { el.base.angle.0 }),
        );
        updates.insert("points".into(), points);
        let mut next = match element_with(&el, updates, w.env) {
            Some(next) => next,
            None => return Obj::Scene(i),
        };
        if is_elbow_arrow(&next) {
            if let ElementKind::Arrow(a) = &mut next.kind {
                a.fixed_segments = Some(None);
            }
            let rebuilt = {
                let live = w.scene.non_deleted();
                let map = ElementsMap::new(live.iter().copied());
                let start_global = get_point_at_index_global_coordinates(&next, 0, &map);
                let end_global = get_point_at_index_global_coordinates(&next, -1, &map);
                let fixed = |end: BindingEnd| -> Option<FixedPointBinding> {
                    let binding = binding_of(&next, end)?;
                    let bindable = map.get(&binding.element_id)?;
                    let fixed_point = calculate_fixed_point_for_elbow_arrow_binding(
                        &next,
                        bindable,
                        end,
                        &map,
                        zoom,
                        binding_enabled,
                        true,
                    );
                    Some(FixedPointBinding {
                        fixed_point,
                        ..binding
                    })
                };
                let start_binding = fixed(BindingEnd::Start);
                let end_binding = fixed(BindingEnd::End);
                let routing_map = elbow_arrow::ElementsMap::new(live.iter().copied());
                let routed = elbow_arrow::update_elbow_arrow_points(
                    &next,
                    &routing_map,
                    &ElbowArrowUpdates {
                        points: Some(vec![
                            [start_global[0] - next.base.x, start_global[1] - next.base.y],
                            [end_global[0] - next.base.x, end_global[1] - next.base.y],
                        ]),
                        fixed_segments: Some(None),
                        start_binding: Some(start_binding.clone()),
                        end_binding: Some(end_binding.clone()),
                        other_keys: false,
                    },
                );
                // { ...newElement, startBinding, endBinding, ...routed }
                let mut map = next.to_map();
                map.insert("startBinding".into(), binding_value(&start_binding));
                map.insert("endBinding".into(), binding_value(&end_binding));
                if let Ok(routed) = routed {
                    for (key, value) in routed.to_map() {
                        map.insert(key, value);
                    }
                }
                Element::from_map(map).ok()
            };
            if let Some(rebuilt) = rebuilt {
                next = rebuilt;
            }
        } else {
            // bindBindingElement mutates the copy and the bound element: the
            // copy stands in the scene for the arrow meanwhile
            let id = next.base.id.clone();
            let original = w.scene.get(&id).cloned();
            w.scene.replace_element(next);
            for end in [BindingEnd::Start, BindingEnd::End] {
                let Some(binding) = w.scene.get(&id).and_then(|a| binding_of(a, end)) else {
                    continue;
                };
                if w.scene.get_non_deleted(&binding.element_id).is_some() {
                    bind_binding_element(
                        &mut w.scene,
                        &mut Triggering(w.env),
                        &id,
                        &binding.element_id,
                        bind_mode,
                        end,
                        zoom,
                        None,
                        true,
                        true,
                    );
                }
            }
            next = w
                .scene
                .get(&id)
                .cloned()
                .expect("the arrow stands in the scene");
            if let Some(original) = original {
                w.scene.replace_element(original);
            }
        }
        Obj::Own(Box::new(next))
    });
    let elements = w.materialize(slots);
    let mut patch = one("currentItemArrowType", value.clone());
    let selected_id = app_state
        .get("selectedLinearElement")
        .and_then(|l| l.get("elementId"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());
    if let Some(id) = selected_id {
        if elements.iter().any(|e| e.base.id == id) {
            patch.insert(
                "selectedLinearElement".into(),
                json!({ "elementId": id, "isEditing": false }),
            );
        }
    }
    ActionResult {
        elements: Some(elements),
        app_state: patch,
        capture: true,
        never: false,
    }
}

/// `actionTogglePolygon.perform` (`actionLinearEditor.tsx:139-173`): the
/// selected lines closed into polygons when one is not, else opened (and
/// unfilled); `None` when something else is selected.
fn toggle_polygon<E: StyleEnv>(w: &mut Work<'_, E>, app_state: &AppState) -> Option<ActionResult> {
    let selected = selected_ids(&w.scene, app_state, false);
    let lines: Vec<&Element> = selected.iter().filter_map(|id| w.scene.get(id)).collect();
    if lines
        .iter()
        .any(|e| !matches!(e.kind, ElementKind::Line(_)))
    {
        return None;
    }
    let next_state = lines
        .iter()
        .any(|e| matches!(&e.kind, ElementKind::Line(l) if !l.polygon));
    let original = w.scene.elements().to_vec();
    let mut elements = Vec::with_capacity(original.len());
    for el in original {
        let next = match line_points(&el) {
            Some(points) if selected.contains(&el.base.id) => {
                let background = if next_state {
                    json!(el.base.background_color)
                } else {
                    json!("transparent")
                };
                let mut updates = one("backgroundColor", background);
                if let Some(polygon) = toggle_line_polygon_state(points, next_state) {
                    updates.extend(polygon);
                }
                element_with(&el, updates, w.env)
            }
            _ => None,
        };
        elements.push(next.unwrap_or(el));
    }
    Some(ActionResult {
        elements: Some(elements),
        app_state: Map::new(),
        capture: true,
        never: false,
    })
}

/// `actionLink.perform` (`actionLink.tsx:24-38`): the hyperlink editor
/// opens (the menu closes); `None` while it is open.
fn hyperlink(app_state: &AppState) -> Option<ActionResult> {
    if app_state.get("showHyperlinkPopup").and_then(Value::as_str) == Some("editor") {
        return None;
    }
    let mut patch = one("showHyperlinkPopup", json!("editor"));
    patch.insert("openMenu".into(), Value::Null);
    Some(ActionResult {
        elements: None,
        app_state: patch,
        capture: true,
        never: false,
    })
}

/// `actionToggleCropEditor.perform` (`actionCropEditor.tsx:22-34`): the
/// first selected element (bound text included) is the one to crop; `None`
/// with nothing selected (upstream throws).
fn crop_editor<E: StyleEnv>(w: &Work<'_, E>, app_state: &AppState) -> Option<ActionResult> {
    let id = selected_ids(&w.scene, app_state, true).into_iter().next()?;
    let mut patch = one("isCropping", json!(false));
    patch.insert("croppingElementId".into(), json!(id));
    Some(ActionResult {
        elements: None,
        app_state: patch,
        capture: true,
        never: false,
    })
}

/// The eye dropper's live preview, LayerUI's `EyeDropper` `onChange`
/// (`components/LayerUI.tsx:522-567`): while the pointer is held the
/// sampled `color` goes to `property` of the selection (`mutateElement`
/// with `getColorUpdate`; a stroke pick includes the bound labels, a
/// note's visible text), or, with nothing selected, to the colour
/// target's defaults (`getColorTargetAppStateUpdates`). Nothing is
/// captured: the pick on release records the change (`onSelect`, the
/// colour action's perform).
pub fn eye_dropper_preview<E: StyleEnv>(
    elements: &[Element],
    app_state: &AppState,
    property: ColorProperty,
    color: &str,
    env: &mut E,
) -> ActionResult {
    let selected_ids = object_key(app_state, "selectedElementIds");
    let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
    let stroke = property == ColorProperty::StrokeColor;
    let targets: HashSet<String> = get_selected_elements(&live, &selected_ids, stroke, false)
        .into_iter()
        .map(|e| e.base.id.clone())
        .collect();
    if get_selected_elements(&live, &selected_ids, false, false).is_empty() {
        let (props, action_env) = (AppProps::default(), ActionEnv::default());
        let target = resolve_color_target(
            &action_context(elements, app_state, &props, &action_env),
            property,
        );
        return ActionResult {
            elements: None,
            app_state: color_target_app_state_updates(&target, color),
            capture: false,
            never: false,
        };
    }
    let map = ElementsMap::new(elements.iter());
    let next = elements
        .iter()
        .map(|el| {
            if !targets.contains(&el.base.id) {
                return el.clone();
            }
            element_with(el, color_update(el, property, color, &map), env)
                .unwrap_or_else(|| el.clone())
        })
        .collect();
    ActionResult {
        elements: Some(next),
        app_state: Map::new(),
        capture: false,
        never: false,
    }
}
