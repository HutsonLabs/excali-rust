//! Converting the selected elements to another type: the Tab / Shift+Tab
//! cycle and the popup under the selection that lists the types.
//!
//! Upstream, at the pinned commit,
//! `packages/excalidraw/components/ConvertElementTypePopup.tsx`:
//!
//! - the popup's effects (`:157-176`: closing when the selection changes
//!   kind, `:178-183`: the caches cleared when it closes) and the Panel's
//!   (`:229-255`: the caches primed from the selection);
//! - the Panel's position and shapes (`:186-331`);
//! - `adjustBoundTextSize` (`:333-376`), `convertElementTypes`
//!   (`:380-639`), `getConversionTypeFromElements` (`:641-664`),
//!   `convertLineToElbow` (`:698-801`), `sanitizePoints` (`:803-821`),
//!   `convertElementType` (`:823-930`).
//!
//! The Tab keys (`App.tsx:5636-5672`) are [`crate::keyboard`]'s; the host
//! answers [`crate::keyboard::KeyEffect::ConvertElementType`] with
//! [`ConvertElementTypePopup::convert`].

use std::collections::HashMap;

use excali_core::app_state::AppState;
use excali_core::element::{BoundElementType, Element, ElementKind, FixedSegment, LocalPoint};
use excali_core::fractional_index::SceneElementsMap;
use excali_math::{point_rotate_rads, GlobalPoint, Point, Radians};
use excali_scene::bounds::{get_bound_text_element, get_element_absolute_coords};
use excali_text::font_metadata::get_font_string;
use excali_text::text_element::{get_bound_text_max_height, get_bound_text_max_width};
use excali_text::text_measurements::measure_text;
use excali_text::text_wrapping::wrap_text;
use serde_json::{json, Map, Value};

use crate::binding::{reanchor_bindings_to_outline, update_bindings, BindingEnv};
use crate::elbow_arrow::{update_elbow_arrow_points, ElbowArrowUpdates, ElementsMap};
use crate::js_value::num;
use crate::keyboard::{binding_app_state, get_selected_elements, ConversionType, ConvertDirection};
use crate::mutate::{bump_version, mutate_element};
use crate::resize_elements::get_common_bounding_box;
use crate::scene::{ElementUpdate, Scene};
use crate::store::HistoryEnv;
use crate::tools::{update_active_tool, ActiveTool, ActiveToolUpdate, Tool, ToolType};
use crate::viewport::{scene_coords_to_viewport_coords, ViewportState};

/// `ConvertibleTypes`: the generic shapes and the linear sub-types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConvertibleType {
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    SharpArrow,
    CurvedArrow,
    ElbowArrow,
}

/// `GENERIC_TYPES`, in the order Tab cycles them.
pub const GENERIC_TYPES: [ConvertibleType; 3] = [
    ConvertibleType::Rectangle,
    ConvertibleType::Diamond,
    ConvertibleType::Ellipse,
];

/// `LINEAR_TYPES`, in the order Tab cycles them.
pub const LINEAR_TYPES: [ConvertibleType; 4] = [
    ConvertibleType::Line,
    ConvertibleType::SharpArrow,
    ConvertibleType::CurvedArrow,
    ConvertibleType::ElbowArrow,
];

impl ConvertibleType {
    /// The name upstream gives the type (the panel's `title` and
    /// `data-testid` suffix).
    pub fn name(self) -> &'static str {
        match self {
            ConvertibleType::Rectangle => "rectangle",
            ConvertibleType::Diamond => "diamond",
            ConvertibleType::Ellipse => "ellipse",
            ConvertibleType::Line => "line",
            ConvertibleType::SharpArrow => "sharpArrow",
            ConvertibleType::CurvedArrow => "curvedArrow",
            ConvertibleType::ElbowArrow => "elbowArrow",
        }
    }

    pub fn from_name(name: &str) -> Option<ConvertibleType> {
        GENERIC_TYPES
            .into_iter()
            .chain(LINEAR_TYPES)
            .find(|t| t.name() == name)
    }

    /// `isConvertibleGenericType`.
    pub fn is_generic(self) -> bool {
        GENERIC_TYPES.contains(&self)
    }
}

/// A generic element's type, `None` for any other element.
fn generic_type(e: &Element) -> Option<ConvertibleType> {
    match e.kind {
        ElementKind::Rectangle => Some(ConvertibleType::Rectangle),
        ElementKind::Diamond => Some(ConvertibleType::Diamond),
        ElementKind::Ellipse => Some(ConvertibleType::Ellipse),
        _ => None,
    }
}

/// `getLinearElementSubType(element)` (`typeChecks.ts:375-388`), `None`
/// for an element that is not a line or an arrow.
pub fn linear_sub_type(e: &Element) -> Option<ConvertibleType> {
    match &e.kind {
        ElementKind::Line(_) => Some(ConvertibleType::Line),
        ElementKind::Arrow(a) if a.elbowed => Some(ConvertibleType::ElbowArrow),
        ElementKind::Arrow(_) if e.base.roundness.is_none() => Some(ConvertibleType::SharpArrow),
        ElementKind::Arrow(_) => Some(ConvertibleType::CurvedArrow),
        _ => None,
    }
}

/// `getConvertibleType(element)`.
fn convertible_type(e: &Element) -> Option<ConvertibleType> {
    linear_sub_type(e).or_else(|| generic_type(e))
}

fn has_bound_text_element(e: &Element) -> bool {
    e.base
        .bound_elements
        .iter()
        .flatten()
        .any(|b| b.kind == BoundElementType::Text)
}

/// `isEligibleLinearElement(element)` (`:666-672`): a line, or an arrow
/// bound to nothing and without a label.
fn is_eligible_linear(e: &Element) -> bool {
    match &e.kind {
        ElementKind::Line(_) => true,
        ElementKind::Arrow(a) => {
            a.linear.start_binding.is_none()
                && a.linear.end_binding.is_none()
                && !has_bound_text_element(e)
        }
        _ => false,
    }
}

/// `getConversionTypeFromElements(elements)` (`:641-664`): generic when
/// any element is a rectangle, diamond or ellipse, else linear when any is
/// an eligible line or arrow.
pub fn get_conversion_type(elements: &[&Element]) -> Option<ConversionType> {
    let mut can_be_linear = false;
    for e in elements {
        if generic_type(e).is_some() {
            return Some(ConversionType::Generic);
        }
        if is_eligible_linear(e) {
            can_be_linear = true;
        }
    }
    can_be_linear.then_some(ConversionType::Linear)
}

/// The elements of `elements` a conversion of `conversion` changes
/// (`filterGenericConvetibleElements`, `filterLinearConvertibleElements`).
fn convertible<'a>(elements: &[&'a Element], conversion: ConversionType) -> Vec<&'a Element> {
    elements
        .iter()
        .copied()
        .filter(|e| match conversion {
            ConversionType::Generic => generic_type(e).is_some(),
            ConversionType::Linear => is_eligible_linear(e),
        })
        .collect()
}

// -- convertLineToElbow -----------------------------------------------------------

/// `THRESHOLD`.
const THRESHOLD: f64 = 20.0;

fn is_vert(a: LocalPoint, b: LocalPoint) -> bool {
    a[0] == b[0]
}

fn is_horz(a: LocalPoint, b: LocalPoint) -> bool {
    a[1] == b[1]
}

fn dist(a: LocalPoint, b: LocalPoint) -> f64 {
    if is_vert(a, b) {
        (a[1] - b[1]).abs()
    } else {
        (a[0] - b[0]).abs()
    }
}

/// `sanitizePoints(points)` (`:803-821`): consecutive duplicates dropped.
fn sanitize_points(points: &[LocalPoint]) -> Vec<LocalPoint> {
    let mut out: Vec<LocalPoint> = Vec::with_capacity(points.len());
    for &p in points {
        if out.last() != Some(&p) {
            out.push(p);
        }
    }
    out
}

/// `convertLineToElbow(line)` (`:698-801`): an orthogonal route through the
/// line's points, offsets under 20 snapped onto the axis, colinear middle
/// points dropped and short jogs collapsed.
pub fn convert_line_to_elbow(points: &[LocalPoint]) -> Vec<LocalPoint> {
    // upstream reads line.points[0] (undefined for an empty line)
    let Some(&first) = points.first() else {
        return Vec::new();
    };
    // 1. an orthogonal route
    let mut ortho = vec![first];
    let src = sanitize_points(points);
    for &p in src.iter().skip(1) {
        let start = *ortho.last().unwrap();
        let mut end = p;
        if (end[0] - start[0]).abs() < THRESHOLD {
            end[0] = start[0];
        } else if (end[1] - start[1]).abs() < THRESHOLD {
            end[1] = start[1];
        }
        if is_vert(start, end) || is_horz(start, end) {
            ortho.push(end);
        } else {
            ortho.push([start[0], end[1]]);
            ortho.push(end);
        }
    }

    // 2. colinear middle points dropped
    let mut trimmed = vec![ortho[0]];
    for i in 1..ortho.len().saturating_sub(1) {
        let colinear = (is_vert(ortho[i - 1], ortho[i]) && is_vert(ortho[i], ortho[i + 1]))
            || (is_horz(ortho[i - 1], ortho[i]) && is_horz(ortho[i], ortho[i + 1]));
        if !colinear {
            trimmed.push(ortho[i]);
        }
    }
    trimmed.push(ortho[ortho.len() - 1]);

    // 3. micro jogs collapsed; upstream moves the points in place, so a
    // point pulled onto an axis is the one later steps read
    let mut clean = vec![trimmed[0]];
    for i in 1..trimmed.len().saturating_sub(1) {
        let a = *clean.last().unwrap();
        let b = trimmed[i];
        let c = trimmed[i + 1];
        let v1 = is_vert(a, b);
        let v2 = is_vert(b, c);
        if v1 != v2 {
            let d1 = dist(a, b);
            let d2 = dist(b, c);
            if d1 < THRESHOLD || d2 < THRESHOLD {
                if d2 < d1 {
                    // absorb leg 2: pull c onto the axis of a-b
                    if v1 {
                        trimmed[i + 1][0] = a[0];
                    } else {
                        trimmed[i + 1][1] = a[1];
                    }
                } else {
                    // absorb leg 1: slide the first leg onto the b-c axis
                    let axis = if v2 { 0 } else { 1 };
                    let mut k = clean.len();
                    while k > 0 && clean[k - 1][axis] == a[axis] {
                        clean[k - 1][axis] = b[axis];
                        k -= 1;
                    }
                }
                continue;
            }
        }
        clean.push(b);
    }
    clean.push(trimmed[trimmed.len() - 1]);
    clean
}

// -- convertElementType -----------------------------------------------------------

/// `ROUNDNESS.PROPORTIONAL_RADIUS` and `ROUNDNESS.ADAPTIVE_RADIUS`.
const PROPORTIONAL_RADIUS: u8 = 2;
const ADAPTIVE_RADIUS: u8 = 3;

/// `_newElementBase(type, { ...element })` (`newElement.ts:87-172`): the
/// keys every element has, taken from `element`, not deleted, stamped now.
fn new_element_base(ty: &str, e: &Map<String, Value>, now: f64) -> Map<String, Value> {
    let get = |key: &str| e.get(key).cloned().unwrap_or(Value::Null);
    let mut m = Map::new();
    m.insert("id".into(), get("id"));
    m.insert("type".into(), json!(ty));
    for key in [
        "x",
        "y",
        "width",
        "height",
        "angle",
        "strokeColor",
        "backgroundColor",
        "fillStyle",
        "strokeWidth",
        "strokeStyle",
        "roughness",
        "opacity",
        "groupIds",
        "frameId",
        "index",
        "roundness",
        "seed",
    ] {
        m.insert(key.into(), get(key));
    }
    // version: rest.version || 1
    let version = e
        .get("version")
        .and_then(Value::as_f64)
        .filter(|v| *v != 0.0 && !v.is_nan())
        .unwrap_or(1.0);
    m.insert("version".into(), json!(version));
    m.insert("versionNonce".into(), get("versionNonce"));
    m.insert("isDeleted".into(), json!(false));
    m.insert("boundElements".into(), get("boundElements"));
    m.insert("updated".into(), json!(now));
    m.insert(
        "created".into(),
        e.get("created").cloned().unwrap_or(json!(now)),
    );
    m.insert("link".into(), get("link"));
    m.insert("locked".into(), get("locked"));
    if let Some(data) = e.get("customData") {
        m.insert("customData".into(), data.clone());
    }
    m
}

fn arrowhead(app_state: &AppState, key: &str) -> Value {
    match app_state.get(key) {
        Some(Value::String(s)) if !s.is_empty() => Value::String(s.clone()),
        _ => Value::Null,
    }
}

/// `x || null` for an arrowhead read from the element.
fn truthy_or_null(v: Option<&Value>) -> Value {
    match v {
        Some(Value::String(s)) if !s.is_empty() => Value::String(s.clone()),
        _ => Value::Null,
    }
}

/// `convertElementType(element, targetType, app)` (`:823-930`): the element
/// rebuilt as `target` with its version bumped, or the element itself when
/// it already is one or the conversion is not valid (upstream throws there
/// outside production).
fn convert_element_type<E: HistoryEnv>(
    element: &Element,
    target: ConvertibleType,
    app_state: &AppState,
    env: &mut E,
) -> Element {
    let generic_from = generic_type(element).is_some();
    let linear_from = matches!(element.kind, ElementKind::Line(_) | ElementKind::Arrow(_));
    let valid = if target.is_generic() {
        generic_from
    } else {
        linear_from
    };
    if !valid {
        return element.clone();
    }
    // element.type === targetType ("line" is the only linear target that
    // is also an element type)
    let same = match target {
        ConvertibleType::Rectangle => matches!(element.kind, ElementKind::Rectangle),
        ConvertibleType::Diamond => matches!(element.kind, ElementKind::Diamond),
        ConvertibleType::Ellipse => matches!(element.kind, ElementKind::Ellipse),
        ConvertibleType::Line => matches!(element.kind, ElementKind::Line(_)),
        _ => false,
    };
    if same {
        return element.clone();
    }
    let e = element.to_map();
    let now = env.updated();
    let null = Value::Null;
    let map = match target {
        ConvertibleType::Rectangle | ConvertibleType::Diamond | ConvertibleType::Ellipse => {
            let mut m = new_element_base(target.name(), &e, now);
            if element.base.roundness.is_some() {
                let kind = if target == ConvertibleType::Rectangle {
                    ADAPTIVE_RADIUS
                } else {
                    PROPORTIONAL_RADIUS
                };
                m.insert("roundness".into(), json!({ "type": kind }));
            }
            m
        }
        ConvertibleType::Line => {
            let mut m = new_element_base("line", &e, now);
            m.insert(
                "points".into(),
                e.get("points").cloned().unwrap_or(json!([])),
            );
            m.insert("startBinding".into(), null.clone());
            m.insert("endBinding".into(), null.clone());
            m.insert("startArrowhead".into(), null.clone());
            m.insert("endArrowhead".into(), null.clone());
            let polygon = e.get("polygon").cloned().unwrap_or(json!(false));
            m.insert("polygon".into(), polygon);
            m
        }
        ConvertibleType::SharpArrow | ConvertibleType::CurvedArrow => {
            let mut m = new_element_base("arrow", &e, now);
            let roundness = if target == ConvertibleType::CurvedArrow {
                json!({ "type": PROPORTIONAL_RADIUS })
            } else {
                null.clone()
            };
            m.insert("roundness".into(), roundness);
            m.insert(
                "points".into(),
                e.get("points").cloned().unwrap_or(json!([])),
            );
            m.insert("startBinding".into(), null.clone());
            m.insert("endBinding".into(), null.clone());
            m.insert(
                "startArrowhead".into(),
                arrowhead(app_state, "currentItemStartArrowhead"),
            );
            m.insert(
                "endArrowhead".into(),
                arrowhead(app_state, "currentItemEndArrowhead"),
            );
            m.insert("elbowed".into(), json!(false));
            m
        }
        ConvertibleType::ElbowArrow => {
            let mut m = new_element_base("arrow", &e, now);
            m.insert("roundness".into(), null.clone());
            m.insert(
                "points".into(),
                e.get("points").cloned().unwrap_or(json!([])),
            );
            m.insert("startBinding".into(), null.clone());
            m.insert("endBinding".into(), null.clone());
            m.insert(
                "startArrowhead".into(),
                truthy_or_null(e.get("startArrowhead")),
            );
            m.insert("endArrowhead".into(), truthy_or_null(e.get("endArrowhead")));
            m.insert("elbowed".into(), json!(true));
            m.insert("fixedSegments".into(), json!([]));
            m.insert("startIsSpecial".into(), json!(false));
            m.insert("endIsSpecial".into(), json!(false));
            m
        }
    };
    let Ok(mut next) = Element::from_map(map) else {
        return element.clone();
    };
    bump_version(&mut next, None, env);
    next
}

// -- the popup --------------------------------------------------------------------

/// The open popup's state: the kind of selection it opened for
/// (`elementsCategoryRef`) and the two conversion caches, which live as
/// long as it is open.
#[derive(Debug, Clone, Default)]
pub struct ConvertElementTypePopup {
    category: Option<ConversionType>,
    /// `FONT_SIZE_CONVERSION_CACHE`: a container's label font size when the
    /// popup first saw it, restored before each conversion shrinks it again.
    font_sizes: HashMap<String, f64>,
    /// `LINEAR_ELEMENT_CONVERSION_CACHE`: each line or arrow in each
    /// sub-type it has been, so cycling back restores its points and
    /// properties.
    linear: HashMap<(String, ConvertibleType), Element>,
}

/// One button of the panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelShape {
    pub kind: ConvertibleType,
    /// Every convertible selected element is this type (`isSelected`).
    pub checked: bool,
}

/// The panel as upstream renders it (`:257-331`): its CSS position in the
/// editor container and its buttons.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvertPanel {
    pub conversion: ConversionType,
    pub left: f64,
    pub top: f64,
    pub shapes: Vec<PanelShape>,
}

/// `GAP_HORIZONTAL` and `GAP_VERTICAL`.
const GAP_HORIZONTAL: f64 = 8.0;
const GAP_VERTICAL: f64 = 10.0;

impl ConvertElementTypePopup {
    /// The kind of selection the popup opened for.
    pub fn category(&self) -> Option<ConversionType> {
        self.category
    }

    /// The popup unmounted: its caches cleared (`:178-183`).
    pub fn reset(&mut self) {
        *self = ConvertElementTypePopup::default();
    }

    /// The popup's effects after a render, `open` being the
    /// `convertElementTypePopupAtom`: closed on an empty selection, or when
    /// the selection no longer converts the way it did when the popup
    /// opened (`:157-176`); while it stays open, the Panel primes the
    /// caches ([`Self::prime`]). A closed popup forgets its state.
    pub fn sync(&mut self, open: &mut bool, scene: &Scene, app_state: &AppState) {
        if !*open {
            self.reset();
            return;
        }
        let selected = get_selected_elements(scene, app_state, false, false);
        if selected.is_empty() {
            *open = false;
            self.reset();
            return;
        }
        let conversion = get_conversion_type(&selected);
        if conversion.is_some() && self.category.is_none() {
            self.category = conversion;
        } else if self.category.is_some() && conversion != self.category {
            *open = false;
            self.reset();
            return;
        }
        self.prime(scene, app_state);
    }

    /// The Panel's effects (`:229-255`): each convertible selected line or
    /// arrow is remembered in its current sub-type, and each selected
    /// container's label font size, the first time the popup sees them.
    pub fn prime(&mut self, scene: &Scene, app_state: &AppState) {
        let selected = get_selected_elements(scene, app_state, false, false);
        let Some(conversion) = get_conversion_type(&selected) else {
            return;
        };
        let map = scene.elements_map();
        for e in convertible(&selected, conversion) {
            match conversion {
                ConversionType::Linear => {
                    if let Some(kind) = convertible_type(e) {
                        self.linear
                            .entry((e.base.id.clone(), kind))
                            .or_insert_with(|| e.clone());
                    }
                }
                ConversionType::Generic => {
                    if self.font_sizes.contains_key(&e.base.id) {
                        continue;
                    }
                    let size = get_bound_text_element(e, &map).and_then(|t| match &t.kind {
                        ElementKind::Text(t) => Some(t.font_size),
                        _ => None,
                    });
                    if let Some(size) = size {
                        self.font_sizes.insert(e.base.id.clone(), size);
                    }
                }
            }
        }
    }

    /// The panel for the current selection, `None` when nothing selected
    /// converts.
    pub fn panel(scene: &Scene, app_state: &AppState) -> Option<ConvertPanel> {
        let selected = get_selected_elements(scene, app_state, false, false);
        let conversion = get_conversion_type(&selected)?;
        let elements = convertible(&selected, conversion);
        let first = *elements.first()?;
        let kind_of = |e: &Element| match conversion {
            ConversionType::Generic => generic_type(e),
            ConversionType::Linear => linear_sub_type(e),
        };
        let first_kind = kind_of(first);
        let same_type = elements.iter().all(|e| kind_of(e) == first_kind);

        let map = scene.elements_map();
        let (bx, by) = if elements.len() == 1 {
            let [x1, _, _, y2, cx, cy] = get_element_absolute_coords(first, &map, false);
            let p: GlobalPoint = point_rotate_rads(
                Point::new(x1, y2),
                Point::new(cx, cy),
                Radians(first.base.angle.0),
            );
            (p.x, p.y)
        } else {
            let b = get_common_bounding_box(&elements);
            (b.min_x, b.max_y)
        };
        let viewport = ViewportState::from_app_state(app_state);
        let (x, y) = scene_coords_to_viewport_coords(bx, by, &viewport);
        let types: &[ConvertibleType] = match conversion {
            ConversionType::Generic => &GENERIC_TYPES,
            ConversionType::Linear => &LINEAR_TYPES,
        };
        Some(ConvertPanel {
            conversion,
            left: x - viewport.offset_left - GAP_HORIZONTAL,
            top: y + (GAP_VERTICAL + 8.0) * viewport.zoom - viewport.offset_top,
            shapes: types
                .iter()
                .map(|&kind| PanelShape {
                    kind,
                    checked: same_type && first_kind == Some(kind),
                })
                .collect(),
        })
    }

    /// A click on the panel's button for `kind` (`onSelect`, `:307-324`):
    /// the checked one does nothing, any other converts the selection to
    /// it. Returns whether it converted.
    pub fn select<E: HistoryEnv + BindingEnv>(
        &mut self,
        scene: &mut Scene,
        app_state: &mut AppState,
        tool: &mut ActiveTool,
        env: &mut E,
        kind: ConvertibleType,
    ) -> bool {
        let Some(panel) = Self::panel(scene, app_state) else {
            return false;
        };
        if panel.shapes.iter().any(|s| s.kind == kind && s.checked) {
            return false;
        }
        self.prime(scene, app_state);
        self.convert(
            scene,
            app_state,
            tool,
            env,
            Some(panel.conversion),
            Some(kind),
            ConvertDirection::Right,
        )
    }

    /// `convertElementTypes(app, { conversionType, nextType, direction })`
    /// (`:380-639`): the selected elements of the conversion's kind become
    /// `next_type`, or the type after (before, with `Left`) the one they all
    /// are, the first when they differ. The selection is kept, a lone line
    /// or arrow gets its editor, and the tool becomes the selection tool;
    /// `tool` is the app's active tool, written to `activeTool` too. Returns
    /// whether it converted (`false` only without a conversion type).
    #[allow(clippy::too_many_arguments)]
    pub fn convert<E: HistoryEnv + BindingEnv>(
        &mut self,
        scene: &mut Scene,
        app_state: &mut AppState,
        tool: &mut ActiveTool,
        env: &mut E,
        conversion: Option<ConversionType>,
        next_type: Option<ConvertibleType>,
        direction: ConvertDirection,
    ) -> bool {
        let Some(conversion) = conversion else {
            return false;
        };
        let selected: Vec<Element> = get_selected_elements(scene, app_state, false, false)
            .into_iter()
            .cloned()
            .collect();
        let selected_refs: Vec<&Element> = selected.iter().collect();
        let selected_ids: Map<String, Value> = selected
            .iter()
            .map(|e| (e.base.id.clone(), Value::Bool(true)))
            .collect();
        let advancement: isize = match direction {
            ConvertDirection::Right => 1,
            ConvertDirection::Left => -1,
        };
        let elements = convertible(&selected_refs, conversion);
        let step = |types: &[ConvertibleType], index: Option<usize>| {
            let n = types.len() as isize;
            let index = index.map_or(-1, |i| i as isize);
            types[((index + n + advancement) % n) as usize]
        };

        match conversion {
            ConversionType::Generic => {
                let first = elements.first().and_then(|e| generic_type(e));
                let same_type = elements.iter().all(|e| generic_type(e) == first);
                let index = if same_type {
                    first.and_then(|t| GENERIC_TYPES.iter().position(|g| *g == t))
                } else {
                    None
                };
                let next = next_type.unwrap_or_else(|| step(&GENERIC_TYPES, index));
                if next.is_generic() {
                    self.convert_generic(scene, app_state, env, &elements, next);
                    app_state.insert("selectedElementIds", Value::Object(selected_ids));
                    select_tool(app_state, tool);
                }
            }
            ConversionType::Linear => {
                let next = next_type.unwrap_or_else(|| {
                    // reduceToCommonValue(elements, getLinearElementSubType)
                    let first = elements.first().and_then(|e| linear_sub_type(e));
                    let common =
                        first.filter(|_| elements.iter().all(|e| linear_sub_type(e) == first));
                    step(
                        &LINEAR_TYPES,
                        common.and_then(|t| LINEAR_TYPES.iter().position(|l| *l == t)),
                    )
                });
                if !next.is_generic() {
                    self.convert_linear(scene, app_state, env, &elements, next);
                }
                let after = get_selected_elements(scene, app_state, false, false);
                let linear: Vec<&Element> = convertible(&after, ConversionType::Linear);
                let editor = match linear.as_slice() {
                    [one] => json!({ "elementId": one.base.id, "isEditing": false }),
                    _ => Value::Null,
                };
                app_state.insert("selectedElementIds", Value::Object(selected_ids));
                app_state.insert("selectedLinearElement", editor);
                select_tool(app_state, tool);
            }
        }
        true
    }

    fn convert_generic<E: HistoryEnv + BindingEnv>(
        &mut self,
        scene: &mut Scene,
        app_state: &AppState,
        env: &mut E,
        elements: &[&Element],
        next: ConvertibleType,
    ) {
        let mut ids = Vec::with_capacity(elements.len());
        for e in elements {
            let converted = convert_element_type(e, next, app_state, env);
            ids.push(converted.base.id.clone());
            scene.replace_element(converted);
        }
        let zoom = app_state.zoom().unwrap_or(1.0);
        let binding_state = binding_app_state(app_state);
        for id in &ids {
            // every generic element is bindable
            reanchor_bindings_to_outline(scene, env, id, zoom);
            // a bindable element's arrows are laid out again; that fails
            // only for an arrow
            let _ = update_bindings(scene, env, id, &binding_state, None);

            let Some(container) = scene.get(id) else {
                continue;
            };
            let Some(text_id) =
                get_bound_text_element(container, &scene.elements_map()).map(|t| t.base.id.clone())
            else {
                continue;
            };
            if let Some(&size) = self.font_sizes.get(id) {
                mutate_in_scene(scene, env, &text_id, json_update([("fontSize", num(size))]));
            }
            adjust_bound_text_size(scene, env, id, &text_id);
        }
    }

    fn convert_linear<E: HistoryEnv + BindingEnv>(
        &mut self,
        scene: &mut Scene,
        app_state: &AppState,
        env: &mut E,
        elements: &[&Element],
        next: ConvertibleType,
    ) {
        // (element id, reused from the cache)
        let mut converted: Vec<(String, bool)> = Vec::with_capacity(elements.len());
        for e in elements {
            let cached = self
                .linear
                .get(&(e.base.id.clone(), next))
                .filter(|c| linear_sub_type(c) == Some(next));
            if let Some(cached) = cached {
                converted.push((cached.base.id.clone(), true));
                scene.replace_element(cached.clone());
            } else {
                let c = convert_element_type(e, next, app_state, env);
                converted.push((c.base.id.clone(), false));
                scene.replace_element(c);
            }
        }

        // post normalization
        for (id, reused) in &converted {
            let Some(element) = scene.get(id).cloned() else {
                continue;
            };
            match &element.kind {
                ElementKind::Arrow(a) if a.elbowed => {
                    let next_points = convert_line_to_elbow(&a.linear.points);
                    if next_points.len() < 2 {
                        continue;
                    }
                    let fixed_segments: Vec<FixedSegment> = (1..next_points.len() - 2)
                        .map(|i| FixedSegment {
                            start: next_points[i],
                            end: next_points[i + 1],
                            index: (i + 1) as f64,
                        })
                        .collect();
                    let updates = {
                        let map = ElementsMap::new(
                            scene.elements().iter().filter(|e| !e.base.is_deleted),
                        );
                        update_elbow_arrow_points(
                            &element,
                            &map,
                            &ElbowArrowUpdates {
                                points: Some(next_points),
                                fixed_segments: Some(Some(fixed_segments)),
                                ..ElbowArrowUpdates::default()
                            },
                        )
                    };
                    let Ok(updates) = updates else {
                        continue;
                    };
                    let mut update = updates.to_map();
                    update.insert("endArrowhead".into(), json!("arrow"));
                    mutate_in_scene(scene, env, id, update);
                }
                ElementKind::Line(_) | ElementKind::Arrow(_) => {
                    let similar = [
                        ConvertibleType::Line,
                        ConvertibleType::SharpArrow,
                        ConvertibleType::CurvedArrow,
                    ]
                    .into_iter()
                    .find_map(|t| self.linear.get(&(id.clone(), t)));
                    if let Some(points) = similar.and_then(points_of) {
                        scene.mutate_element(
                            id,
                            ElementUpdate {
                                points: Some(points),
                                ..ElementUpdate::default()
                            },
                            env,
                        );
                    }
                }
                _ => {}
            }
            // a cached element is the object the scene holds: what the
            // scene did to it, the cache sees
            if *reused {
                if let Some(e) = scene.get(id) {
                    if let Some(kind) = linear_sub_type(e) {
                        self.linear.insert((id.clone(), kind), e.clone());
                    }
                }
            }
        }
    }
}

fn points_of(e: &Element) -> Option<Vec<LocalPoint>> {
    match &e.kind {
        ElementKind::Line(l) => Some(l.linear.points.clone()),
        ElementKind::Arrow(a) => Some(a.linear.points.clone()),
        _ => None,
    }
}

/// `updateActiveTool(prevState, { type: "selection" })`.
fn select_tool(app_state: &mut AppState, tool: &mut ActiveTool) {
    *tool = update_active_tool(
        tool,
        ActiveToolUpdate::to(Tool::Builtin(ToolType::Selection)),
    );
    app_state.insert("activeTool", tool.to_json());
}

fn json_update<const N: usize>(pairs: [(&str, Value); N]) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

/// `mutateElement(element, elementsMap, updates)` on the scene's element
/// `id`, reading the other elements from the scene.
fn mutate_in_scene<E: HistoryEnv>(
    scene: &mut Scene,
    env: &mut E,
    id: &str,
    update: Map<String, Value>,
) {
    let map: SceneElementsMap = scene
        .elements()
        .iter()
        .map(|e| (e.base.id.clone(), e.clone()))
        .collect();
    let Some(mut element) = scene.get(id).cloned() else {
        return;
    };
    if let Ok(true) = mutate_element(&mut element, &map, update, env) {
        scene.replace_element(element);
    }
}

/// `adjustBoundTextSize(container, boundText, scene)` (`:333-376`): the
/// label's font size stepped down until its text fits the container, then
/// laid out in it again (`redrawTextBoundingBox`).
fn adjust_bound_text_size<E: HistoryEnv + BindingEnv>(
    scene: &mut Scene,
    env: &mut E,
    container_id: &str,
    text_id: &str,
) {
    let (Some(container), Some(text)) = (scene.get(container_id), scene.get(text_id)) else {
        return;
    };
    let ElementKind::Text(fields) = &text.kind else {
        return;
    };
    let max_width = get_bound_text_max_width(container, Some(text));
    let max_height = get_bound_text_max_height(container, text);
    let (next_font_size, width, height) = {
        let (provider, char_widths) = BindingEnv::text(env);
        let font = get_font_string(fields.font_size, fields.font_family);
        let wrapped = wrap_text(&fields.text, &font, max_width, provider, char_widths);
        let mut metrics = measure_text(&wrapped, &font, fields.line_height, provider);
        let mut next_font_size = fields.font_size;
        while (metrics.width > max_width || metrics.height > max_height) && next_font_size > 0.0 {
            next_font_size -= 1.0;
            let font = get_font_string(next_font_size, fields.font_family);
            metrics = measure_text(&fields.text, &font, fields.line_height, provider);
        }
        (next_font_size, metrics.width, metrics.height)
    };
    mutate_in_scene(
        scene,
        env,
        text_id,
        json_update([
            ("fontSize", num(next_font_size)),
            ("width", num(width)),
            ("height", num(height)),
        ]),
    );
    let mut map: SceneElementsMap = scene
        .elements()
        .iter()
        .map(|e| (e.base.id.clone(), e.clone()))
        .collect();
    if HistoryEnv::redraw_text_bounding_box(env, &mut map, text_id, container_id).is_ok() {
        for (_, e) in map {
            if scene.get(&e.base.id) != Some(&e) {
                scene.replace_element(e);
            }
        }
    }
}
