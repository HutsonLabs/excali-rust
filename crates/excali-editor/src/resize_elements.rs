//! Resizing and rotating selected elements
//! (`packages/element/src/resizeElements.ts`).
//!
//! [`transform_elements`] is what the editor calls on every pointer move of
//! a transform: one element is rotated about its centre or resized from a
//! handle ([`resize_single_element`]); several are rotated about the
//! selection's centre or scaled about the opposite side of the selection
//! box. The modifiers upstream reads from the keyboard are flags: Shift
//! locks the aspect ratio (and rotates in 15 degree steps), Alt resizes from
//! the centre.
//!
//! Text is measured and wrapped with the environment's metrics
//! ([`TransformEnv::text`]). Two steps belong to other parts of the editor
//! and are asked of the environment where upstream calls them: moving the
//! arrows bound to a changed element (`updateBoundElements`, `binding.ts`)
//! and laying out a sticky note's label (`getStickyNoteLayout`,
//! `stickyNote.ts`); the sticky note's resize intents, minimum size and
//! the application of its layout are here.

use excali_core::constants::{
    MIN_FONT_SIZE, STICKY_NOTE_BODY_INSET_Y, STICKY_NOTE_FALLBACK_FONT_SIZE,
    STICKY_NOTE_MAX_FONT_SIZE, STICKY_NOTE_MIN_SIZE, STICKY_NOTE_PADDING,
};
use excali_core::element::{
    Element, ElementKind, FixedPointBinding, FixedSegment, FontFamily, LocalPoint,
};
use excali_math::{
    js, normalize_radians, point_center, point_from, point_rotate_rads, GlobalPoint, Radians,
};
use excali_rough::RoughGenerator;
use excali_scene::bounds::{
    get_bound_text_element, get_bound_text_element_id, get_bounds_from_points, get_common_bounds,
    get_container_element, get_curve_path_ops, get_element_absolute_coords, get_element_bounds,
    get_min_max_xy_from_curve_path_ops, Bounds, ElementsMap,
};
use excali_scene::linear_element::get_bound_text_element_position;
use excali_scene::rough_options::generate_rough_options;
use excali_text::font_metadata::{get_font_string, get_line_height};
use excali_text::text_element::{
    compute_bound_text_position, compute_container_dimension_for_bound_text,
    get_bound_text_max_height, get_bound_text_max_width, ArrowLabelGeometry,
};
use excali_text::text_measurements::{
    get_approx_min_line_height, get_approx_min_line_width, get_min_text_element_width,
    measure_text, CharWidthCache, TextMetricsProvider,
};
use excali_text::text_wrapping::wrap_text;

use crate::geometry::get_global_fixed_point_for_bindable_element;
use crate::scene::{ElementUpdate, MutationEnv, Scene};
use crate::transform_handles::{
    is_elbow_arrow, is_frame_like, TransformHandleDirection, TransformHandleType,
};

/// `SHIFT_LOCKING_ANGLE` (`common/src/constants.ts:34`): rotation steps of
/// 15 degrees with Shift held.
pub const SHIFT_LOCKING_ANGLE: f64 = std::f64::consts::PI / 12.0;

// -- the environment ---------------------------------------------------------------

/// The edge of a sticky note that stays put when its layout changes its
/// height (`VerticalResizeAnchor`, `sizeHelpers.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickyNoteLayoutAnchor {
    Top,
    Center,
    Bottom,
}

/// `StickyNoteLayoutOpts` without `originalText` (`stickyNote.ts:529-546`),
/// as a resize passes them: the base height and font ceiling to keep and
/// the anchor. `None` leaves each to the layout.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StickyNoteLayoutOpts {
    pub base_height: Option<f64>,
    pub base_font_size: Option<f64>,
    pub anchor: Option<StickyNoteLayoutAnchor>,
}

/// The label half of a [`StickyNoteLayout`].
#[derive(Debug, Clone, PartialEq)]
pub struct StickyNoteTextLayout {
    pub text: String,
    pub font_size: f64,
    pub base_font_size: f64,
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    pub angle: f64,
}

/// `StickyNoteLayout` (`stickyNote.ts:548-564`): the note's geometry and
/// its label's, as `getStickyNoteLayout` computes them.
#[derive(Debug, Clone, PartialEq)]
pub struct StickyNoteLayout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub base_height: f64,
    pub text: Option<StickyNoteTextLayout>,
}

/// What a transform needs from the rest of the editor.
pub trait TransformEnv: MutationEnv {
    /// The text metrics and the per-font character width cache
    /// (`textMeasurements.ts`'s provider and `charWidth`).
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache);

    /// `updateBoundElements(changedElement, scene, { simultaneouslyUpdated })`
    /// (`binding.ts:1321-1429`): moves the arrows bound to `changed`, except
    /// those in `simultaneously_updated` (the elements transformed with it).
    /// Called wherever upstream calls it, for every element, bindable or
    /// not.
    fn update_bound_elements(
        &mut self,
        scene: &mut Scene,
        changed: &str,
        simultaneously_updated: Option<&[String]>,
    );

    /// `getStickyNoteLayout(container, textElement, opts)`
    /// (`stickyNote.ts:669-762`): the note's and its label's geometry.
    fn sticky_note_layout(
        &mut self,
        container: &Element,
        text: Option<&Element>,
        opts: &StickyNoteLayoutOpts,
    ) -> StickyNoteLayout;
}

/// Arrow labels placed by the arrow's geometry
/// (`LinearElementEditor.getBoundTextElementPosition`).
struct ArrowLabels;

impl ArrowLabelGeometry for ArrowLabels {
    fn bound_text_element_position(
        &mut self,
        arrow: &Element,
        text: &Element,
        elements: &[Element],
    ) -> Option<[f64; 2]> {
        Some(get_bound_text_element_position(
            arrow,
            text,
            &ElementsMap::new(elements.iter().filter(|e| !e.base.is_deleted)),
        ))
    }
}

// -- element kinds ----------------------------------------------------------------

fn is_text(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Text(_))
}

fn is_linear(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Line(_) | ElementKind::Arrow(_))
}

fn is_freedraw(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Freedraw(_))
}

fn is_arrow(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Arrow(_))
}

fn is_sticky_note(e: &Element) -> bool {
    matches!(e.kind, ElementKind::StickyNote(_))
}

/// `isBoundToContainer(element)`: a text whose `containerId` is not null.
fn is_bound_to_container(e: &Element) -> bool {
    matches!(&e.kind, ElementKind::Text(t) if t.container_id.is_some())
}

fn text_fields(e: &Element) -> Option<&excali_core::element::TextFields> {
    match &e.kind {
        ElementKind::Text(t) => Some(t),
        _ => None,
    }
}

fn bindings(e: &Element) -> (Option<&FixedPointBinding>, Option<&FixedPointBinding>) {
    match e.kind.linear() {
        Some(l) => (l.start_binding.as_ref(), l.end_binding.as_ref()),
        None => (None, None),
    }
}

fn find<'a>(elements: &'a [Element], id: &str) -> Option<&'a Element> {
    elements.iter().find(|e| e.base.id == id)
}

/// `Math.sign(x)`.
fn sign(x: f64) -> f64 {
    if x.is_nan() {
        f64::NAN
    } else if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

fn truthy(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}

/// `a || 0` for a number.
fn or_zero(x: f64) -> f64 {
    if truthy(x) {
        x
    } else {
        0.0
    }
}

fn rotate(p: [f64; 2], center: [f64; 2], angle: f64) -> [f64; 2] {
    let r: GlobalPoint = point_rotate_rads(
        point_from(p[0], p[1]),
        point_from(center[0], center[1]),
        Radians(angle),
    );
    [r.x, r.y]
}

fn normalize(angle: f64) -> f64 {
    normalize_radians(Radians(angle)).0
}

fn font_string(e: &Element) -> String {
    let t = text_fields(e).expect("a text element");
    get_font_string(t.font_size, t.font_family)
}

// -- points -----------------------------------------------------------------------

/// `rescalePoints(dimension, newSize, points, normalize)`
/// (`common/src/points.ts:22-66`): scales one coordinate of the points to
/// span `new_size`; with `normalize`, moves them back so the smallest
/// coordinate stays where it was (not for two points).
pub fn rescale_points(
    dimension: usize,
    new_size: f64,
    points: &[LocalPoint],
    normalize: bool,
) -> Vec<LocalPoint> {
    let max_coordinate = points
        .iter()
        .fold(f64::NEG_INFINITY, |m, p| js::max(m, p[dimension]));
    let min_coordinate = points
        .iter()
        .fold(f64::INFINITY, |m, p| js::min(m, p[dimension]));
    let size = max_coordinate - min_coordinate;
    let scale = if size == 0.0 { 1.0 } else { new_size / size };

    let mut next_min_coordinate = f64::INFINITY;
    let scaled: Vec<LocalPoint> = points
        .iter()
        .map(|p| {
            let new_coordinate = p[dimension] * scale;
            let mut new_point = *p;
            new_point[dimension] = new_coordinate;
            if new_coordinate < next_min_coordinate {
                next_min_coordinate = new_coordinate;
            }
            new_point
        })
        .collect();

    if !normalize || scaled.len() == 2 {
        // we don't translate two-point lines
        return scaled;
    }

    let translation = min_coordinate - next_min_coordinate;
    scaled
        .into_iter()
        .map(|mut p| {
            p[dimension] += translation;
            p
        })
        .collect()
}

/// `rescalePointsInElement(element, width, height, normalizePoints)`
/// (`resizeElements.ts:275-290`): a line's, arrow's or freedraw's points
/// scaled to `width` by `height`; `None` for other elements.
pub fn rescale_points_in_element(
    element: &Element,
    width: f64,
    height: f64,
    normalize: bool,
) -> Option<Vec<LocalPoint>> {
    if !(is_linear(element) || is_freedraw(element)) {
        return None;
    }
    let points = element.kind.points().unwrap_or(&[]);
    Some(rescale_points(
        0,
        width,
        &rescale_points(1, height, points, normalize),
        normalize,
    ))
}

/// `getResizedElementAbsoluteCoords(element, nextWidth, nextHeight,
/// normalizePoints)` (`bounds.ts:1044-1095`): the element's box at the
/// next size. Lines and arrows are measured along the rough.js curve of
/// their rescaled points, freedraw along its points.
pub fn get_resized_element_absolute_coords(
    element: &Element,
    next_width: f64,
    next_height: f64,
    normalize_points: bool,
) -> Bounds {
    let b = &element.base;
    let Some(points) =
        rescale_points_in_element(element, next_width, next_height, normalize_points)
    else {
        return [b.x, b.y, b.x + next_width, b.y + next_height];
    };
    let [min_x, min_y, max_x, max_y] = if is_freedraw(element) {
        get_bounds_from_points(&points, 0.0)
    } else {
        let generator = RoughGenerator::new();
        let ops = generate_rough_options(element, false, false)
            .ok()
            .and_then(|options| {
                let options = options.to_rough(generator.default_options());
                if element.base.roundness.is_none() {
                    Some(generator.linear_path(&points, &options))
                } else {
                    generator.curve(&points, &options).ok()
                }
            })
            .map(|shape| get_curve_path_ops(&shape).to_vec())
            .unwrap_or_default();
        get_min_max_xy_from_curve_path_ops(&ops, None)
    };
    [min_x + b.x, min_y + b.y, max_x + b.x, max_y + b.y]
}

/// `getCommonBoundingBox(elements)` (`bounds.ts:1128-1144`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub width: f64,
    pub height: f64,
    pub mid_x: f64,
    pub mid_y: f64,
}

pub fn get_common_bounding_box(elements: &[&Element]) -> BoundingBox {
    let [min_x, min_y, max_x, max_y] = get_common_bounds(elements);
    BoundingBox {
        min_x,
        min_y,
        max_x,
        max_y,
        width: max_x - min_x,
        height: max_y - min_y,
        mid_x: (min_x + max_x) / 2.0,
        mid_y: (min_y + max_y) / 2.0,
    }
}

// -- small helpers of the element modules -----------------------------------------

/// `getPositionAfterHeightChange(element, nextHeight, anchor)`
/// (`sizeHelpers.ts:28-54`): where the element goes so that the anchored
/// edge stays put when its height becomes `next_height`.
fn get_position_after_height_change(
    element: &Element,
    next_height: f64,
    anchor: StickyNoteLayoutAnchor,
) -> (f64, f64) {
    let b = &element.base;
    let delta = (b.height - next_height) / 2.0;
    if anchor == StickyNoteLayoutAnchor::Center {
        // Rotation is about the center, so holding it is angle-independent.
        return (b.x, b.y + delta);
    }
    let sin = js::sin(b.angle.0);
    let cos = js::cos(b.angle.0);
    if anchor == StickyNoteLayoutAnchor::Bottom {
        return (b.x - delta * sin, b.y + delta * (1.0 + cos));
    }
    (b.x + delta * sin, b.y + delta * (1.0 - cos))
}

/// `normalizeStickyNoteFontSize(fontSize)` (`stickyNote.ts:379-384`).
fn normalize_sticky_note_font_size(size: f64) -> f64 {
    if !size.is_finite() {
        return STICKY_NOTE_FALLBACK_FONT_SIZE;
    }
    js::min(STICKY_NOTE_MAX_FONT_SIZE, js::max(MIN_FONT_SIZE, size))
}

/// `getStickyNoteMinSize({ fontSize, fontFamily })` (`stickyNote.ts:504-521`):
/// a note must fit one line of its label at the font ceiling.
pub fn get_sticky_note_min_size(font_size: f64, font_family: FontFamily) -> (f64, f64) {
    let line_height_px =
        (normalize_sticky_note_font_size(font_size) * get_line_height(font_family)).ceil();
    (
        js::max(
            STICKY_NOTE_MIN_SIZE,
            line_height_px + STICKY_NOTE_PADDING * 2.0,
        ),
        js::max(
            STICKY_NOTE_MIN_SIZE,
            line_height_px + STICKY_NOTE_BODY_INSET_Y,
        ),
    )
}

/// `getStickyNoteResizeIntent(container, originalElementsMap,
/// handleDirection, { proportional, fromCenter, flip })`
/// (`stickyNote.ts:777-823`): the base height and font ceiling a resize
/// asks the layout to keep, from the note at pointer-down, and the edge
/// that stays put.
pub fn get_sticky_note_resize_intent(
    container: &Element,
    original_elements: &[Element],
    handle: TransformHandleDirection,
    proportional: bool,
    from_center: bool,
    flip: bool,
) -> StickyNoteLayoutOpts {
    let orig_sticky = find(original_elements, &container.base.id)
        .filter(|e| is_sticky_note(e))
        .unwrap_or(container);
    let base_height_of = |e: &Element| match &e.kind {
        ElementKind::StickyNote(n) => n.base_height,
        _ => f64::NAN,
    };
    let originals_map = ElementsMap::new(original_elements);
    let ceiling = get_bound_text_element(orig_sticky, &originals_map)
        .and_then(text_fields)
        .map(|t| t.base_font_size.unwrap_or(t.font_size));

    if flip {
        return StickyNoteLayoutOpts {
            base_height: Some(base_height_of(orig_sticky)),
            base_font_size: ceiling,
            anchor: None,
        };
    }

    let changes_height = proportional || handle.includes('n') || handle.includes('s');
    let scale = if proportional && truthy(orig_sticky.base.width) {
        container.base.width / orig_sticky.base.width
    } else {
        1.0
    };
    let anchor = if from_center {
        StickyNoteLayoutAnchor::Center
    } else if handle.includes('n') {
        StickyNoteLayoutAnchor::Bottom
    } else if proportional
        && handle.is_side()
        && matches!(
            handle,
            TransformHandleDirection::E | TransformHandleDirection::W
        )
    {
        // Shift+E/W holds the opposite side's midpoint, i.e. the vertical center
        StickyNoteLayoutAnchor::Center
    } else {
        StickyNoteLayoutAnchor::Top
    };
    StickyNoteLayoutOpts {
        base_height: Some(if changes_height {
            container.base.height
        } else {
            base_height_of(orig_sticky)
        }),
        base_font_size: ceiling.map(|c| c * scale),
        anchor: Some(anchor),
    }
}

/// `updateStickyNoteLayout(container, scene, { ...opts, bindings })`
/// (`stickyNote.ts:832-865`): lays the note and its label out
/// ([`TransformEnv::sticky_note_layout`]) and applies the layout; then,
/// unless `bindings` is `None` (upstream's `bindings: false`), moves the
/// arrows bound to the note.
fn update_sticky_note_layout(
    container_id: &str,
    scene: &mut Scene,
    env: &mut dyn TransformEnv,
    opts: StickyNoteLayoutOpts,
    bindings: Option<Option<&[String]>>,
) {
    let Some(container) = scene.get(container_id).cloned() else {
        return;
    };
    let text = get_bound_text_element(&container, &scene.elements_map()).cloned();
    let layout = env.sticky_note_layout(&container, text.as_ref(), &opts);
    scene.mutate_element(
        container_id,
        ElementUpdate {
            x: Some(layout.x),
            y: Some(layout.y),
            width: Some(layout.width),
            height: Some(layout.height),
            base_height: Some(layout.base_height),
            ..ElementUpdate::default()
        },
        env,
    );
    if let (Some(text), Some(t)) = (&text, layout.text) {
        scene.mutate_element(
            &text.base.id,
            ElementUpdate {
                text: Some(t.text),
                font_size: Some(t.font_size),
                base_font_size: Some(Some(t.base_font_size)),
                width: Some(t.width),
                height: Some(t.height),
                x: Some(t.x),
                y: Some(t.y),
                angle: Some(t.angle),
                ..ElementUpdate::default()
            },
            env,
        );
    }
    if let Some(simultaneously_updated) = bindings {
        if !container.base.is_deleted {
            env.update_bound_elements(scene, container_id, simultaneously_updated);
        }
    }
}

/// `unbindBindingElement(arrow, startOrEnd, scene)` (`binding.ts:1187-1217`):
/// drops the arrow's binding at that end and, unless the other end is
/// bound to the same element, the arrow from that element's
/// `boundElements`.
pub fn unbind_binding_element(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    arrow_id: &str,
    start: bool,
) -> Option<String> {
    let arrow = scene.get(arrow_id)?;
    let (start_binding, end_binding) = bindings(arrow);
    let (binding, opposite) = if start {
        (start_binding, end_binding)
    } else {
        (end_binding, start_binding)
    };
    let binding = binding?.element_id.clone();
    if opposite.is_none_or(|o| o.element_id != binding) {
        // Only remove the record on the bound element if the other end is
        // not bound to the same element
        if let Some(bound) = scene.get_non_deleted(&binding) {
            let remaining = bound.base.bound_elements.as_ref().map(|list| {
                list.iter()
                    .filter(|b| b.id != arrow_id)
                    .cloned()
                    .collect::<Vec<_>>()
            });
            scene.mutate_element(
                &binding,
                ElementUpdate {
                    // `boundElements?.filter(...)`: undefined for no list
                    bound_elements: remaining.map(Some),
                    ..ElementUpdate::default()
                },
                env,
            );
        }
    }
    let update = if start {
        ElementUpdate {
            start_binding: Some(None),
            ..ElementUpdate::default()
        }
    } else {
        ElementUpdate {
            end_binding: Some(None),
            ..ElementUpdate::default()
        }
    };
    scene.mutate_element(arrow_id, update, env);
    Some(binding)
}

/// `handleBindTextResize(container, scene, transformHandleType,
/// shouldMaintainAspectRatio, shouldResizeFromCenter, flipByY)`
/// (`textElement.ts:155-247`): rewraps a container's label to the
/// container's new width (not for a free `n` or `s` resize), grows the
/// container when the label no longer fits its height (from the edge the
/// gesture holds still), and centres the label again.
pub fn handle_bind_text_resize(
    container_id: &str,
    scene: &mut Scene,
    env: &mut dyn TransformEnv,
    handle: Option<TransformHandleDirection>,
    should_maintain_aspect_ratio: bool,
    should_resize_from_center: bool,
    flip_by_y: bool,
) {
    let Some(container) = scene.get(container_id).cloned() else {
        return;
    };
    if is_sticky_note(&container) {
        // resize callers pass their intents to `updateStickyNoteLayout`
        // directly and own the bound-arrow pass; this is the fallback for
        // generic callers
        update_sticky_note_layout(
            container_id,
            scene,
            env,
            StickyNoteLayoutOpts::default(),
            None,
        );
        return;
    }
    if get_bound_text_element_id(&container).is_none() {
        return;
    }
    let Some(text_element) = get_bound_text_element(&container, &scene.elements_map()).cloned()
    else {
        return;
    };
    let Some(t) = text_fields(&text_element) else {
        return;
    };
    if t.text.is_empty() {
        return;
    }
    let mut text = t.text.clone();
    let mut next_height = text_element.base.height;
    let mut next_width = text_element.base.width;
    let max_width = get_bound_text_max_width(&container, Some(&text_element));
    let max_height = get_bound_text_max_height(&container, &text_element);
    let is_side_ns = matches!(
        handle,
        Some(TransformHandleDirection::N | TransformHandleDirection::S)
    );
    if should_maintain_aspect_ratio || !is_side_ns {
        let font = font_string(&text_element);
        let (provider, char_widths) = env.text();
        text = wrap_text(&t.original_text, &font, max_width, provider, char_widths);
        let metrics = measure_text(&text, &font, t.line_height, provider);
        next_height = metrics.height;
        next_width = metrics.width;
    }
    // increase height in case text element height exceeds
    if next_height > max_height {
        let container_height =
            compute_container_dimension_for_bound_text(next_height, container.kind.element_type());
        // Crossing the opposite edge swaps the anchor for text-driven growth.
        let from_top = matches!(
            handle,
            Some(
                TransformHandleDirection::N
                    | TransformHandleDirection::Ne
                    | TransformHandleDirection::Nw
            )
        ) != flip_by_y;
        let mut update = ElementUpdate {
            height: Some(container_height),
            ..ElementUpdate::default()
        };
        if !is_arrow(&container) {
            let anchor = if should_resize_from_center {
                StickyNoteLayoutAnchor::Center
            } else if from_top {
                StickyNoteLayoutAnchor::Bottom
            } else {
                StickyNoteLayoutAnchor::Top
            };
            let (x, y) = get_position_after_height_change(&container, container_height, anchor);
            update.x = Some(x);
            update.y = Some(y);
        }
        scene.mutate_element(container_id, update, env);
    }

    let text_id = text_element.base.id.clone();
    scene.mutate_element(
        &text_id,
        ElementUpdate {
            text: Some(text),
            width: Some(next_width),
            height: Some(next_height),
            ..ElementUpdate::default()
        },
        env,
    );

    if !is_arrow(&container) {
        let (Some(container), Some(label)) = (scene.get(container_id), scene.get(&text_id)) else {
            return;
        };
        if let Some([x, y]) =
            compute_bound_text_position(container, label, scene.elements(), &mut ArrowLabels)
        {
            scene.mutate_element(&text_id, ElementUpdate::position(x, y), env);
        }
    }
}

// -- transformElements ----------------------------------------------------------------

/// The modifiers of a transform (`shouldRotateWithDiscreteAngle`,
/// `shouldResizeFromCenter`, `shouldMaintainAspectRatio`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransformFlags {
    pub rotate_with_discrete_angle: bool,
    pub resize_from_center: bool,
    pub maintain_aspect_ratio: bool,
}

/// `transformElements(originalElements, transformHandleType,
/// selectedElements, scene, shouldRotateWithDiscreteAngle,
/// shouldResizeFromCenter, shouldMaintainAspectRatio, pointerX, pointerY,
/// centerX, centerY)` (`resizeElements.ts:94-208`). `original_elements`
/// are the elements as they were at pointer-down; `selected_ids` names the
/// selection, read from the scene as it is now. Returns whether a
/// transform happened (always, for one element).
#[allow(clippy::too_many_arguments)]
pub fn transform_elements(
    original_elements: &[Element],
    transform_handle_type: Option<TransformHandleType>,
    selected_ids: &[String],
    scene: &mut Scene,
    env: &mut dyn TransformEnv,
    flags: TransformFlags,
    [pointer_x, pointer_y]: [f64; 2],
    [center_x, center_y]: [f64; 2],
) -> bool {
    let selected: Vec<String> = scene
        .selected(selected_ids)
        .iter()
        .map(|e| e.base.id.clone())
        .collect();
    if selected.len() == 1 {
        let id = &selected[0];
        match transform_handle_type {
            Some(TransformHandleType::Rotation) => {
                if !scene.get(id).is_some_and(is_elbow_arrow) {
                    rotate_single_element(
                        id,
                        scene,
                        env,
                        pointer_x,
                        pointer_y,
                        flags.rotate_with_discrete_angle,
                    );
                    env.update_bound_elements(scene, id, None);
                }
            }
            Some(TransformHandleType::Resize(direction)) => {
                if let (Some(latest), Some(orig)) =
                    (scene.get_non_deleted(id), find(original_elements, id))
                {
                    let (next_width, next_height) = get_next_single_width_and_height_from_pointer(
                        latest,
                        orig,
                        direction,
                        pointer_x,
                        pointer_y,
                        flags.maintain_aspect_ratio,
                        flags.resize_from_center,
                    );
                    resize_single_element(
                        next_width,
                        next_height,
                        id,
                        original_elements,
                        scene,
                        env,
                        direction,
                        ResizeOptions {
                            should_maintain_aspect_ratio: flags.maintain_aspect_ratio,
                            should_resize_from_center: flags.resize_from_center,
                        },
                    );
                }
            }
            None => {}
        }
        if scene.get(id).is_some_and(is_text) {
            env.update_bound_elements(scene, id, None);
        }
        return true;
    }
    if selected.len() > 1 {
        match transform_handle_type {
            Some(TransformHandleType::Rotation) => {
                rotate_multiple_elements(
                    original_elements,
                    &selected,
                    scene,
                    env,
                    pointer_x,
                    pointer_y,
                    flags.rotate_with_discrete_angle,
                    center_x,
                    center_y,
                );
                return true;
            }
            Some(TransformHandleType::Resize(direction)) => {
                let next = get_next_multiple_width_and_height_from_pointer(
                    scene,
                    &selected,
                    original_elements,
                    direction,
                    pointer_x,
                    pointer_y,
                    flags.maintain_aspect_ratio,
                    flags.resize_from_center,
                );
                resize_multiple_elements(
                    &selected,
                    direction,
                    scene,
                    env,
                    original_elements,
                    MultipleResize {
                        should_maintain_aspect_ratio: flags.maintain_aspect_ratio,
                        should_resize_from_center: flags.resize_from_center,
                        flip_by_x: next.flip_by_x,
                        flip_by_y: next.flip_by_y,
                        next_width: next.next_width,
                        next_height: next.next_height,
                        original_bounding_box: next.original_bounding_box,
                    },
                );
                return true;
            }
            None => {}
        }
    }
    false
}

/// `rotateSingleElement` (`resizeElements.ts:210-273`): turns the element
/// to face the pointer from its centre (frames never turn), in 15 degree
/// steps with Shift; an arrow drops its bindings; a container's label
/// turns and moves with it.
fn rotate_single_element(
    id: &str,
    scene: &mut Scene,
    env: &mut dyn TransformEnv,
    pointer_x: f64,
    pointer_y: f64,
    should_rotate_with_discrete_angle: bool,
) {
    let Some(element) = scene.get(id).cloned() else {
        return;
    };
    let [x1, y1, x2, y2, _, _] =
        get_element_absolute_coords(&element, &scene.elements_map(), false);
    let cx = (x1 + x2) / 2.0;
    let cy = (y1 + y2) / 2.0;
    let angle = if is_frame_like(&element) {
        0.0
    } else {
        let mut angle =
            (5.0 * std::f64::consts::PI) / 2.0 + js::atan2(pointer_y - cy, pointer_x - cx);
        if should_rotate_with_discrete_angle {
            angle += SHIFT_LOCKING_ANGLE / 2.0;
            angle -= angle % SHIFT_LOCKING_ANGLE;
        }
        normalize(angle)
    };
    let bound_text_id = get_bound_text_element_id(&element).map(str::to_string);

    if is_arrow(&element) {
        let (start, end) = bindings(&element);
        let (start, end) = (start.is_some(), end.is_some());
        if start {
            unbind_binding_element(scene, env, id, true);
        }
        if end {
            unbind_binding_element(scene, env, id, false);
        }
    }

    scene.mutate_element(
        id,
        ElementUpdate {
            angle: Some(angle),
            ..ElementUpdate::default()
        },
        env,
    );

    if let Some(text_id) = bound_text_id {
        if !is_arrow(&element) {
            // `scene.getElement`: the label is read deleted or not.
            let (Some(container), Some(text)) = (scene.get(id), scene.get(&text_id)) else {
                return;
            };
            if let Some([x, y]) =
                compute_bound_text_position(container, text, scene.elements(), &mut ArrowLabels)
            {
                scene.mutate_element(
                    &text_id,
                    ElementUpdate {
                        angle: Some(angle),
                        x: Some(x),
                        y: Some(y),
                        ..ElementUpdate::default()
                    },
                    env,
                );
            }
        }
    }
}

/// `measureFontSizeFromWidth(element, elementsMap, nextWidth)`
/// (`resizeElements.ts:292-315`): the font size that makes the text (or the
/// room its container gives it) `next_width` wide; `None` below
/// `MIN_FONT_SIZE`.
pub fn measure_font_size_from_width(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    next_width: f64,
) -> Option<f64> {
    let t = text_fields(element)?;
    // We only use width to scale font on resize
    let mut width = element.base.width;
    if is_bound_to_container(element) {
        if let Some(container) = get_container_element(element, elements_map) {
            width = get_bound_text_max_width(container, Some(element));
        }
    }
    let next_font_size = t.font_size * (next_width / width);
    if next_font_size < MIN_FONT_SIZE {
        return None;
    }
    Some(next_font_size)
}

/// `resizeSingleTextElement` (`resizeElements.ts:317-409`): corner and
/// `n`/`s` handles scale the font with the height; `e`/`w` rewrap the text
/// to the new width (never narrower than an empty line) and stop
/// auto-resizing.
#[allow(clippy::too_many_arguments)]
fn resize_single_text_element(
    orig: &Element,
    id: &str,
    scene: &mut Scene,
    env: &mut dyn TransformEnv,
    handle: TransformHandleDirection,
    should_resize_from_center: bool,
    next_width: f64,
    next_height: f64,
) {
    let Some(element) = scene.get(id).cloned() else {
        return;
    };
    let Some(t) = text_fields(&element) else {
        return;
    };
    let metrics_width = element.base.width * (next_height / element.base.height);
    let Some(font_size) =
        measure_font_size_from_width(&element, &scene.elements_map(), metrics_width)
    else {
        return;
    };
    let previous_origin = [orig.base.x, orig.base.y];

    if handle.includes('n') || handle.includes('s') {
        let (x, y) = get_resized_origin(
            previous_origin,
            orig.base.width,
            orig.base.height,
            metrics_width,
            next_height,
            orig.base.angle.0,
            handle,
            false,
            should_resize_from_center,
        );
        scene.mutate_element(
            id,
            ElementUpdate {
                font_size: Some(font_size),
                width: Some(metrics_width),
                height: Some(next_height),
                x: Some(x),
                y: Some(y),
                ..ElementUpdate::default()
            },
            env,
        );
        return;
    }

    if matches!(
        handle,
        TransformHandleDirection::E | TransformHandleDirection::W
    ) {
        let (provider, char_widths) = env.text();
        let min_width = get_min_text_element_width(
            &get_font_string(t.font_size, t.font_family),
            t.line_height,
            provider,
        );
        let new_width = js::max(min_width, next_width);
        let font = font_string(&element);
        let text = wrap_text(
            &t.original_text,
            &font,
            new_width.abs(),
            provider,
            char_widths,
        );
        let metrics = measure_text(&text, &font, t.line_height, provider);
        let new_height = metrics.height;
        let (x, y) = get_resized_origin(
            previous_origin,
            orig.base.width,
            orig.base.height,
            new_width,
            new_height,
            element.base.angle.0,
            handle,
            false,
            should_resize_from_center,
        );
        scene.mutate_element(
            id,
            ElementUpdate {
                width: Some(new_width.abs()),
                height: Some(metrics.height.abs()),
                x: Some(x),
                y: Some(y),
                text: Some(text),
                auto_resize: Some(false),
                ..ElementUpdate::default()
            },
            env,
        );
    }
}

/// `rotateMultipleElements` (`resizeElements.ts:411-495`): turns every
/// element but frames about the selection's centre; elbow arrows are
/// re-routed from their fixed points instead, arrows drop bindings to
/// elements outside the selection, and container labels follow.
#[allow(clippy::too_many_arguments)]
fn rotate_multiple_elements(
    original_elements: &[Element],
    ids: &[String],
    scene: &mut Scene,
    env: &mut dyn TransformEnv,
    pointer_x: f64,
    pointer_y: f64,
    should_rotate_with_discrete_angle: bool,
    center_x: f64,
    center_y: f64,
) {
    let mut center_angle =
        (5.0 * std::f64::consts::PI) / 2.0 + js::atan2(pointer_y - center_y, pointer_x - center_x);
    if should_rotate_with_discrete_angle {
        center_angle += SHIFT_LOCKING_ANGLE / 2.0;
        center_angle -= center_angle % SHIFT_LOCKING_ANGLE;
    }

    for id in ids {
        let Some(element) = scene.get(id).cloned() else {
            continue;
        };
        if is_frame_like(&element) {
            continue;
        }
        let map = scene.elements_map();
        let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(&element, &map, false);
        let cx = (x1 + x2) / 2.0;
        let cy = (y1 + y2) / 2.0;
        let orig_angle =
            find(original_elements, id).map_or(element.base.angle.0, |o| o.base.angle.0);
        let [rotated_cx, rotated_cy] = rotate(
            [cx, cy],
            [center_x, center_y],
            center_angle + orig_angle - element.base.angle.0,
        );
        let update = if is_elbow_arrow(&element) {
            // Needed to re-route the arrow
            ElementUpdate {
                points: Some(get_arrow_local_fixed_points(&element, &map).to_vec()),
                ..ElementUpdate::default()
            }
        } else {
            ElementUpdate {
                x: Some(element.base.x + (rotated_cx - cx)),
                y: Some(element.base.y + (rotated_cy - cy)),
                angle: Some(normalize(center_angle + orig_angle)),
                ..ElementUpdate::default()
            }
        };
        scene.mutate_element(id, update, env);

        env.update_bound_elements(scene, id, Some(ids));

        if let Some(element) = scene.get(id).cloned() {
            if is_arrow(&element) {
                let start = bindings(&element)
                    .0
                    .is_some_and(|b| !ids.contains(&b.element_id));
                if start {
                    unbind_binding_element(scene, env, id, true);
                }
                let end = scene
                    .get(id)
                    .is_some_and(|e| bindings(e).1.is_some_and(|b| !ids.contains(&b.element_id)));
                if end {
                    unbind_binding_element(scene, env, id, false);
                }
            }
        }

        let Some(element) = scene.get(id).cloned() else {
            continue;
        };
        let bound_text = get_bound_text_element(&element, &scene.elements_map()).cloned();
        if let Some(bound_text) = bound_text {
            if !is_arrow(&element) {
                if let Some([x, y]) = compute_bound_text_position(
                    &element,
                    &bound_text,
                    scene.elements(),
                    &mut ArrowLabels,
                ) {
                    scene.mutate_element(
                        &bound_text.base.id,
                        ElementUpdate {
                            x: Some(x),
                            y: Some(y),
                            angle: Some(normalize(center_angle + orig_angle)),
                            ..ElementUpdate::default()
                        },
                        env,
                    );
                }
            }
        }
    }
}

/// `getArrowLocalFixedPoints(arrow, elementsMap)` (`binding.ts:2727-2737`):
/// an elbow arrow's two ends where its bindings put them (or where they
/// are, unbound), relative to the arrow.
fn get_arrow_local_fixed_points(
    arrow: &Element,
    elements_map: &ElementsMap<'_>,
) -> [LocalPoint; 2] {
    let points = arrow.kind.points().unwrap_or(&[]);
    let (start, end) = bindings(arrow);
    let global = |binding: Option<&FixedPointBinding>, point: Option<&LocalPoint>| -> [f64; 2] {
        match binding.and_then(|b| elements_map.get(&b.element_id).map(|e| (b, e))) {
            Some((b, target)) => get_global_fixed_point_for_bindable_element(b.fixed_point, target),
            None => {
                let p = point.copied().unwrap_or([f64::NAN, f64::NAN]);
                [arrow.base.x + p[0], arrow.base.y + p[1]]
            }
        }
    };
    let start = global(start, points.first());
    let end = global(end, points.last());
    // LinearElementEditor.pointFromAbsoluteCoords: no rotation for elbow
    // arrows
    [
        [start[0] - arrow.base.x, start[1] - arrow.base.y],
        [end[0] - arrow.base.x, end[1] - arrow.base.y],
    ]
}

/// `getResizeOffsetXY(transformHandleType, selectedElements, elementsMap,
/// x, y)` (`resizeElements.ts:497-554`): how far the pointer is from the
/// edge or corner it grabbed, in the element's rotated frame turned back to
/// the scene; the resize then follows the pointer less this offset.
pub fn get_resize_offset_xy(
    transform_handle_type: Option<TransformHandleType>,
    selected_elements: &[&Element],
    elements_map: &ElementsMap<'_>,
    x: f64,
    y: f64,
) -> [f64; 2] {
    let single = selected_elements.len() == 1;
    let [x1, y1, x2, y2] = if single {
        let [x1, y1, x2, y2, _, _] =
            get_element_absolute_coords(selected_elements[0], elements_map, false);
        [x1, y1, x2, y2]
    } else {
        get_common_bounds(selected_elements)
    };
    let cx = (x1 + x2) / 2.0;
    let cy = (y1 + y2) / 2.0;
    let angle = if single {
        selected_elements[0].base.angle.0
    } else {
        0.0
    };
    let [x, y] = rotate([x, y], [cx, cy], -angle);
    let turn = |dx: f64, dy: f64| rotate([dx, dy], [0.0, 0.0], angle);
    use TransformHandleDirection as D;
    match transform_handle_type {
        Some(TransformHandleType::Resize(d)) => match d {
            D::N => turn(x - (x1 + x2) / 2.0, y - y1),
            D::S => turn(x - (x1 + x2) / 2.0, y - y2),
            D::W => turn(x - x1, y - (y1 + y2) / 2.0),
            D::E => turn(x - x2, y - (y1 + y2) / 2.0),
            D::Nw => turn(x - x1, y - y1),
            D::Ne => turn(x - x2, y - y1),
            D::Sw => turn(x - x1, y - y2),
            D::Se => turn(x - x2, y - y2),
        },
        _ => [0.0, 0.0],
    }
}

/// Which end of a two-point line a corner handle drags
/// (`getResizeArrowDirection`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowDirection {
    Origin,
    End,
}

/// `getResizeArrowDirection(transformHandleType, element)`
/// (`resizeElements.ts:556-567`).
pub fn get_resize_arrow_direction(
    transform_handle_type: Option<TransformHandleType>,
    element: &Element,
) -> ArrowDirection {
    let [px, py] = element
        .kind
        .points()
        .and_then(|p| p.get(1).copied())
        .unwrap_or([f64::NAN, f64::NAN]);
    use TransformHandleDirection as D;
    let d = transform_handle_type.and_then(TransformHandleType::direction);
    let is_resize_end = (d == Some(D::Nw) && (px < 0.0 || py < 0.0))
        || (d == Some(D::Ne) && px >= 0.0)
        || (d == Some(D::Sw) && px <= 0.0)
        || (d == Some(D::Se) && (px > 0.0 || py > 0.0));
    if is_resize_end {
        ArrowDirection::End
    } else {
        ArrowDirection::Origin
    }
}

/// `ResizeAnchor` (`resizeElements.ts:569-578`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResizeAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    WestSide,
    NorthSide,
    EastSide,
    SouthSide,
    Center,
}

/// `getResizeAnchor` (`resizeElements.ts:580-619`): the point that stays
/// put.
fn get_resize_anchor(
    handle: TransformHandleDirection,
    should_maintain_aspect_ratio: bool,
    should_resize_from_center: bool,
) -> ResizeAnchor {
    use TransformHandleDirection as D;
    if should_resize_from_center {
        return ResizeAnchor::Center;
    }
    if should_maintain_aspect_ratio {
        return match handle {
            D::N => ResizeAnchor::SouthSide,
            D::E => ResizeAnchor::WestSide,
            D::S => ResizeAnchor::NorthSide,
            D::W => ResizeAnchor::EastSide,
            D::Ne => ResizeAnchor::BottomLeft,
            D::Nw => ResizeAnchor::BottomRight,
            D::Se => ResizeAnchor::TopLeft,
            D::Sw => ResizeAnchor::TopRight,
        };
    }
    match handle {
        D::E | D::Se | D::S => ResizeAnchor::TopLeft,
        D::N | D::Nw | D::W => ResizeAnchor::BottomRight,
        D::Ne => ResizeAnchor::BottomLeft,
        D::Sw => ResizeAnchor::TopRight,
    }
}

/// `getResizedOrigin` (`resizeElements.ts:621-727`): the new `x, y` of an
/// element turned by `angle` so that its anchor stays in place when it goes
/// from `prev` to `new` size.
#[allow(clippy::too_many_arguments)]
fn get_resized_origin(
    [x, y]: [f64; 2],
    prev_width: f64,
    prev_height: f64,
    new_width: f64,
    new_height: f64,
    angle: f64,
    handle: TransformHandleDirection,
    should_maintain_aspect_ratio: bool,
    should_resize_from_center: bool,
) -> (f64, f64) {
    let anchor = get_resize_anchor(
        handle,
        should_maintain_aspect_ratio,
        should_resize_from_center,
    );
    let cos = js::cos(angle);
    let sin = js::sin(angle);
    match anchor {
        ResizeAnchor::TopLeft => (
            x + (prev_width - new_width) / 2.0
                + ((new_width - prev_width) / 2.0) * cos
                + ((prev_height - new_height) / 2.0) * sin,
            y + (prev_height - new_height) / 2.0
                + ((new_width - prev_width) / 2.0) * sin
                + ((new_height - prev_height) / 2.0) * cos,
        ),
        ResizeAnchor::TopRight => (
            x + ((prev_width - new_width) / 2.0) * (cos + 1.0)
                + ((prev_height - new_height) / 2.0) * sin,
            y + (prev_height - new_height) / 2.0
                + ((prev_width - new_width) / 2.0) * sin
                + ((new_height - prev_height) / 2.0) * cos,
        ),
        ResizeAnchor::BottomLeft => (
            x + ((prev_width - new_width) / 2.0) * (1.0 - cos)
                + ((new_height - prev_height) / 2.0) * sin,
            y + ((prev_height - new_height) / 2.0) * (cos + 1.0)
                + ((new_width - prev_width) / 2.0) * sin,
        ),
        ResizeAnchor::BottomRight => (
            x + ((prev_width - new_width) / 2.0) * (cos + 1.0)
                + ((new_height - prev_height) / 2.0) * sin,
            y + ((prev_height - new_height) / 2.0) * (cos + 1.0)
                + ((prev_width - new_width) / 2.0) * sin,
        ),
        ResizeAnchor::Center => (
            x - (new_width - prev_width) / 2.0,
            y - (new_height - prev_height) / 2.0,
        ),
        ResizeAnchor::EastSide => (
            x + ((prev_width - new_width) / 2.0) * (cos + 1.0),
            y + ((prev_width - new_width) / 2.0) * sin + (prev_height - new_height) / 2.0,
        ),
        ResizeAnchor::WestSide => (
            x + ((prev_width - new_width) / 2.0) * (1.0 - cos),
            y + ((new_width - prev_width) / 2.0) * sin + (prev_height - new_height) / 2.0,
        ),
        ResizeAnchor::NorthSide => (
            x + (prev_width - new_width) / 2.0 + ((prev_height - new_height) / 2.0) * sin,
            y + ((new_height - prev_height) / 2.0) * (cos - 1.0),
        ),
        ResizeAnchor::SouthSide => (
            x + (prev_width - new_width) / 2.0 + ((new_height - prev_height) / 2.0) * sin,
            y + ((prev_height - new_height) / 2.0) * (cos + 1.0),
        ),
    }
}

/// The options of [`resize_single_element`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResizeOptions {
    pub should_maintain_aspect_ratio: bool,
    pub should_resize_from_center: bool,
}

/// `resizeSingleElement(nextWidth, nextHeight, latestElement, origElement,
/// originalElementsMap, scene, handleDirection, options)`
/// (`resizeElements.ts:729-986`): resizes the element `id` to `next_width`
/// by `next_height` (negative flips it across the anchor) from
/// `original_elements`' copy of it. Texts scale or rewrap; containers keep
/// room for their label (and scale its font with the aspect ratio locked);
/// sticky notes keep their minimum size and are laid out again; lines and
/// freedraw rescale their points; images flip their `scale`; an arrow
/// drops its bindings.
#[allow(clippy::too_many_arguments)]
pub fn resize_single_element(
    next_width: f64,
    next_height: f64,
    id: &str,
    original_elements: &[Element],
    scene: &mut Scene,
    env: &mut dyn TransformEnv,
    handle: TransformHandleDirection,
    options: ResizeOptions,
) {
    let ResizeOptions {
        should_maintain_aspect_ratio,
        should_resize_from_center,
    } = options;
    let (Some(latest), Some(orig)) = (scene.get(id).cloned(), find(original_elements, id)) else {
        return;
    };
    if is_text(&latest) && is_text(orig) {
        return resize_single_text_element(
            orig,
            id,
            scene,
            env,
            handle,
            should_resize_from_center,
            next_width,
            next_height,
        );
    }

    // Constraints and font scaling use magnitudes. Keep the signs for the
    // geometry below, where crossing the opposite edge flips the element.
    let flip_factor_x = if next_width < 0.0 { -1.0 } else { 1.0 };
    let flip_factor_y = if next_height < 0.0 { -1.0 } else { 1.0 };
    let mut next_width = next_width.abs();
    let mut next_height = next_height.abs();

    let bound_text = get_bound_text_element(&latest, &scene.elements_map()).cloned();
    let is_resizing_sticky_note = is_sticky_note(&latest);
    let mut min_size: Option<(f64, f64)> = None;
    if is_resizing_sticky_note {
        // A note must fit one line at its label's font ceiling.
        min_size = Some(match bound_text.as_ref().and_then(text_fields) {
            Some(t) => {
                get_sticky_note_min_size(t.base_font_size.unwrap_or(t.font_size), t.font_family)
            }
            None => (STICKY_NOTE_MIN_SIZE, STICKY_NOTE_MIN_SIZE),
        });
    } else if let Some(t) = bound_text.as_ref().and_then(text_fields) {
        if !should_maintain_aspect_ratio {
            let font = get_font_string(t.font_size, t.font_family);
            let (provider, char_widths) = env.text();
            min_size = Some((
                get_approx_min_line_width(&font, t.line_height, provider, char_widths),
                get_approx_min_line_height(t.font_size, t.line_height),
            ));
        }
    }

    if let Some((min_width, min_height)) = min_size {
        next_width = js::max(next_width, min_width);
        next_height = js::max(next_height, min_height);
        if should_maintain_aspect_ratio {
            // Both dimensions must use the same scale even at the minimum size.
            let scale = js::max(next_width / orig.base.width, next_height / orig.base.height);
            next_width = orig.base.width * scale;
            next_height = orig.base.height * scale;
        }
    }

    let mut bound_text_font_size: Option<f64> = None;
    if let Some(bound_text) = bound_text.as_ref().filter(|_| !is_resizing_sticky_note) {
        if let Some(t) = find(original_elements, &bound_text.base.id).and_then(text_fields) {
            bound_text_font_size = Some(t.font_size);
        }
        if should_maintain_aspect_ratio {
            let mut updated = latest.clone();
            updated.base.width = next_width;
            updated.base.height = next_height;
            let max_width = get_bound_text_max_width(&updated, Some(bound_text));
            match measure_font_size_from_width(bound_text, &scene.elements_map(), max_width) {
                Some(size) => bound_text_font_size = Some(size),
                None => return,
            }
        }
    }

    next_width *= flip_factor_x;
    next_height *= flip_factor_y;

    let mut rescaled_points = rescale_points_in_element(orig, next_width, next_height, true);

    let mut previous_origin = [orig.base.x, orig.base.y];
    if is_linear(orig) {
        let [x1, y1, _, _] = get_element_bounds(orig, &ElementsMap::new(original_elements));
        previous_origin = [x1, y1];
    }

    let (mut new_x, mut new_y) = get_resized_origin(
        previous_origin,
        orig.base.width,
        orig.base.height,
        next_width,
        next_height,
        orig.base.angle.0,
        handle,
        should_maintain_aspect_ratio,
        should_resize_from_center,
    );

    if is_linear(orig) {
        if let Some(points) = rescaled_points.as_mut() {
            let offset_x = orig.base.x - previous_origin[0];
            let offset_y = orig.base.y - previous_origin[1];
            new_x += offset_x;
            new_y += offset_y;
            let [scaled_x, scaled_y] = points.first().copied().unwrap_or([f64::NAN, f64::NAN]);
            new_x += scaled_x;
            new_y += scaled_y;
            for p in points.iter_mut() {
                *p = [p[0] - scaled_x, p[1] - scaled_y];
            }
        }
    }

    // flipping
    if next_width < 0.0 {
        new_x += next_width;
    }
    if next_height < 0.0 {
        new_y += next_height;
    }

    if let (ElementKind::Image(_), ElementKind::Image(orig_image)) = (&latest.kind, &orig.kind) {
        let [sx, sy] = orig_image.scale;
        // defaulting because scaleX/Y can be 0/-0
        let factor = |s: f64, fallback: f64| if truthy(s) { s } else { fallback };
        scene.mutate_element(
            id,
            ElementUpdate {
                scale: Some([
                    factor(sign(next_width), sx) * sx,
                    factor(sign(next_height), sy) * sy,
                ]),
                ..ElementUpdate::default()
            },
            env,
        );
    }

    if is_arrow(&latest) && should_maintain_aspect_ratio {
        if let Some(t) = bound_text.as_ref().and_then(text_fields) {
            let font_size = (next_width.abs() / latest.base.width) * t.font_size;
            if font_size < MIN_FONT_SIZE {
                return;
            }
            bound_text_font_size = Some(font_size);
        }
    }

    if next_width != 0.0 && next_height != 0.0 && new_x.is_finite() && new_y.is_finite() {
        let mut update = ElementUpdate {
            x: Some(new_x),
            y: Some(new_y),
            width: Some(next_width.abs()),
            height: Some(next_height.abs()),
            points: rescaled_points,
            ..ElementUpdate::default()
        };

        if is_arrow(&latest) {
            let (start, end) = bindings(&latest);
            let (start, end) = (start.is_some(), end.is_some());
            if start {
                unbind_binding_element(scene, env, id, true);
            }
            if end {
                update.end_binding = Some(None);
            }
        }

        scene.mutate_element(id, update, env);

        if is_resizing_sticky_note {
            let Some(latest) = scene.get(id).cloned() else {
                return;
            };
            let intent = get_sticky_note_resize_intent(
                &latest,
                original_elements,
                handle,
                should_maintain_aspect_ratio,
                should_resize_from_center,
                false,
            );
            // the arrow pass below is this function's — keep it single
            update_sticky_note_layout(id, scene, env, intent, None);
        } else {
            if let Some(bound_text) = &bound_text {
                scene.mutate_element(
                    &bound_text.base.id,
                    ElementUpdate {
                        font_size: bound_text_font_size,
                        ..ElementUpdate::default()
                    },
                    env,
                );
            }
            handle_bind_text_resize(
                id,
                scene,
                env,
                Some(handle),
                should_maintain_aspect_ratio,
                should_resize_from_center,
                flip_factor_y < 0.0,
            );
        }

        env.update_bound_elements(scene, id, None);
    }
}

/// `getNextSingleWidthAndHeightFromPointer` (`resizeElements.ts:988-1080`):
/// the size the pointer asks for, measured in the element's frame at
/// pointer-down, doubled from the centre with Alt and with the aspect
/// ratio kept with Shift.
fn get_next_single_width_and_height_from_pointer(
    latest: &Element,
    orig: &Element,
    handle: TransformHandleDirection,
    pointer_x: f64,
    pointer_y: f64,
    should_maintain_aspect_ratio: bool,
    should_resize_from_center: bool,
) -> (f64, f64) {
    // Gets bounds corners
    let [x1, y1, x2, y2] =
        get_resized_element_absolute_coords(orig, orig.base.width, orig.base.height, true);
    let start_top_left: GlobalPoint = point_from(x1, y1);
    let start_bottom_right: GlobalPoint = point_from(x2, y2);
    let start_center = point_center(start_top_left, start_bottom_right);

    // Calculate new dimensions based on cursor position
    let rotated_pointer = point_rotate_rads(
        point_from(pointer_x, pointer_y),
        start_center,
        Radians(-orig.base.angle.0),
    );

    // Get bounds corners rendered on screen
    let [esx1, esy1, esx2, esy2] =
        get_resized_element_absolute_coords(latest, latest.base.width, latest.base.height, true);

    let bounds_current_width = esx2 - esx1;
    let bounds_current_height = esy2 - esy1;

    // It's important we set the initial scale value based on the width and
    // height at resize start, otherwise previous dimensions affected by
    // modifiers will be taken into account.
    let at_start_bounds_width = start_bottom_right.x - start_top_left.x;
    let at_start_bounds_height = start_bottom_right.y - start_top_left.y;
    let mut scale_x = at_start_bounds_width / bounds_current_width;
    let mut scale_y = at_start_bounds_height / bounds_current_height;

    if handle.includes('e') {
        scale_x = (rotated_pointer.x - start_top_left.x) / bounds_current_width;
    }
    if handle.includes('s') {
        scale_y = (rotated_pointer.y - start_top_left.y) / bounds_current_height;
    }
    if handle.includes('w') {
        scale_x = (start_bottom_right.x - rotated_pointer.x) / bounds_current_width;
    }
    if handle.includes('n') {
        scale_y = (start_bottom_right.y - rotated_pointer.y) / bounds_current_height;
    }

    // We have to use dimensions of element on screen, otherwise the scaling
    // of the dimensions won't match the cursor for linear elements.
    let mut next_width = latest.base.width * scale_x;
    let mut next_height = latest.base.height * scale_y;

    if should_resize_from_center {
        next_width = 2.0 * next_width - orig.base.width;
        next_height = 2.0 * next_height - orig.base.height;
    }

    // adjust dimensions to keep sides ratio
    if should_maintain_aspect_ratio {
        let width_ratio = next_width.abs() / orig.base.width;
        let height_ratio = next_height.abs() / orig.base.height;
        if handle.is_side() {
            next_height *= width_ratio;
            next_width *= height_ratio;
        } else {
            let ratio = js::max(width_ratio, height_ratio);
            next_width = orig.base.width * ratio * sign(next_width);
            next_height = orig.base.height * ratio * sign(next_height);
        }
    }
    (next_width, next_height)
}

/// What [`get_next_multiple_width_and_height_from_pointer`] gives.
struct NextMultiple {
    original_bounding_box: BoundingBox,
    next_width: f64,
    next_height: f64,
    flip_by_x: bool,
    flip_by_y: bool,
}

/// The anchor of a multi-element resize: the opposite corner or side of
/// the selection box (`anchorsMap`).
fn multiple_anchor(bb: &BoundingBox, handle: TransformHandleDirection) -> [f64; 2] {
    use TransformHandleDirection as D;
    let BoundingBox {
        min_x,
        min_y,
        max_x,
        max_y,
        ..
    } = *bb;
    let width = max_x - min_x;
    let height = max_y - min_y;
    match handle {
        D::Ne => [min_x, max_y],
        D::Se => [min_x, min_y],
        D::Sw => [max_x, min_y],
        D::Nw => [max_x, max_y],
        D::E => [min_x, min_y + height / 2.0],
        D::W => [max_x, min_y + height / 2.0],
        D::N => [min_x + width / 2.0, max_y],
        D::S => [min_x + width / 2.0, min_y],
    }
}

/// The copies of the selection at pointer-down and, for their arrows'
/// labels, the labels placed where the arrows put them
/// (`resizeElements.ts:1097-1132`).
fn original_selection_with_labels(
    scene: &Scene,
    ids: &[String],
    original_elements: &[Element],
) -> Vec<Element> {
    let originals: Vec<Element> = ids
        .iter()
        .filter_map(|id| find(original_elements, id).cloned())
        .collect();
    let scene_map = scene.elements_map();
    let mut labels = Vec::new();
    for orig in &originals {
        if !is_linear(orig) {
            continue;
        }
        let Some(text_id) = get_bound_text_element_id(orig) else {
            continue;
        };
        let Some(text) = find(original_elements, text_id) else {
            continue;
        };
        if !is_bound_to_container(text) {
            continue;
        }
        let [x, y] = get_bound_text_element_position(orig, text, &scene_map);
        let mut label = text.clone();
        label.base.x = x;
        label.base.y = y;
        labels.push(label);
    }
    originals.into_iter().chain(labels).collect()
}

/// `getNextMultipleWidthAndHeightFromPointer`
/// (`resizeElements.ts:1082-1207`): the selection box at pointer-down, the
/// size the pointer asks for from the opposite side (or the centre), and
/// whether the pointer crossed the anchor.
#[allow(clippy::too_many_arguments)]
fn get_next_multiple_width_and_height_from_pointer(
    scene: &Scene,
    ids: &[String],
    original_elements: &[Element],
    handle: TransformHandleDirection,
    pointer_x: f64,
    pointer_y: f64,
    should_maintain_aspect_ratio: bool,
    should_resize_from_center: bool,
) -> NextMultiple {
    let with_labels = original_selection_with_labels(scene, ids, original_elements);
    let refs: Vec<&Element> = with_labels.iter().collect();
    let original_bounding_box = get_common_bounding_box(&refs);
    let bb = original_bounding_box;
    let width = bb.max_x - bb.min_x;
    let height = bb.max_y - bb.min_y;

    // anchor point must be on the opposite side of the dragged selection
    // handle or be the center of the selection if shouldResizeFromCenter
    let [anchor_x, anchor_y] = if should_resize_from_center {
        [bb.mid_x, bb.mid_y]
    } else {
        multiple_anchor(&bb, handle)
    };

    let resize_from_center_scale = if should_resize_from_center { 2.0 } else { 1.0 };

    let scale = js::max(
        or_zero((pointer_x - anchor_x).abs() / width),
        or_zero((pointer_y - anchor_y).abs() / height),
    ) * resize_from_center_scale;

    let mut next_width = if handle.includes('e') || handle.includes('w') {
        (pointer_x - anchor_x).abs() * resize_from_center_scale
    } else {
        width
    };
    let mut next_height = if handle.includes('n') || handle.includes('s') {
        (pointer_y - anchor_y).abs() * resize_from_center_scale
    } else {
        height
    };

    if should_maintain_aspect_ratio {
        next_width = width * scale * sign(pointer_x - anchor_x);
        next_height = height * scale * sign(pointer_y - anchor_y);
    }

    use TransformHandleDirection as D;
    let (flip_by_x, flip_by_y) = match handle {
        D::Ne => (pointer_x < anchor_x, pointer_y > anchor_y),
        D::Se => (pointer_x < anchor_x, pointer_y < anchor_y),
        D::Sw => (pointer_x > anchor_x, pointer_y < anchor_y),
        D::Nw => (pointer_x > anchor_x, pointer_y > anchor_y),
        // e.g. when resizing from the "e" side, we do not need to consider
        // changes in the `y` direction and therefore, we do not need to flip
        // in the `y` direction at all
        D::E => (pointer_x < anchor_x, false),
        D::W => (pointer_x > anchor_x, false),
        D::N => (false, pointer_y > anchor_y),
        D::S => (false, pointer_y < anchor_y),
    };

    NextMultiple {
        original_bounding_box,
        next_width,
        next_height,
        flip_by_x,
        flip_by_y,
    }
}

/// The options of [`resize_multiple_elements`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MultipleResize {
    pub should_maintain_aspect_ratio: bool,
    pub should_resize_from_center: bool,
    pub flip_by_x: bool,
    pub flip_by_y: bool,
    pub next_width: f64,
    pub next_height: f64,
    pub original_bounding_box: BoundingBox,
}

/// One element's part of a multi-element resize.
struct PendingResize {
    id: String,
    update: ElementUpdate,
    bound_text_font_size: Option<f64>,
}

/// `resizeMultipleElements(selectedElements, elementsMap, handleDirection,
/// scene, originalElementsMap, options)` (`resizeElements.ts:1209-1594`):
/// scales every selected element about the anchor, flipping across it; the
/// aspect ratio is kept with Shift or when an element is rotated, a text or
/// in a group. Nothing changes when a text or label would go below
/// `MIN_FONT_SIZE`.
pub fn resize_multiple_elements(
    ids: &[String],
    handle: TransformHandleDirection,
    scene: &mut Scene,
    env: &mut dyn TransformEnv,
    original_elements: &[Element],
    options: MultipleResize,
) {
    let MultipleResize {
        should_maintain_aspect_ratio,
        should_resize_from_center,
        flip_by_x,
        flip_by_y,
        mut next_width,
        next_height,
        original_bounding_box,
    } = options;

    // do not allow next width or height to be 0
    if next_height == 0.0 || next_width == 0.0 {
        return;
    }

    // originalElementsMap holds snapshots of the (non-deleted) selection
    let targets: Vec<(Element, Element)> = ids
        .iter()
        .filter_map(|id| {
            let latest = scene.get_non_deleted(id)?.clone();
            let orig = find(original_elements, id)?.clone();
            Some((orig, latest))
        })
        .collect();

    let bb = original_bounding_box;
    let width = bb.max_x - bb.min_x;
    let height = bb.max_y - bb.min_y;

    if should_maintain_aspect_ratio && (next_width / next_height - width / height).abs() > 0.001 {
        next_width = next_height * (width / height);
    }

    if !(truthy(next_width) && truthy(next_height)) {
        return;
    }

    let mut scale_x = if handle.includes('e') || handle.includes('w') {
        next_width.abs() / width
    } else {
        1.0
    };
    let mut scale_y = if handle.includes('n') || handle.includes('s') {
        next_height.abs() / height
    } else {
        1.0
    };

    let scale = if handle.is_side() {
        if handle.includes('e') || handle.includes('w') {
            scale_x
        } else {
            scale_y
        }
    } else {
        js::max(
            or_zero(next_width.abs() / width),
            or_zero(next_height.abs() / height),
        )
    };

    let [anchor_x, anchor_y] = if should_resize_from_center {
        [bb.mid_x, bb.mid_y]
    } else {
        multiple_anchor(&bb, handle)
    };

    let keep_aspect_ratio = should_maintain_aspect_ratio
        || targets.iter().any(|(_, latest)| {
            latest.base.angle.0 != 0.0 || is_text(latest) || !latest.base.group_ids.is_empty()
        });

    if keep_aspect_ratio {
        scale_x = scale;
        scale_y = scale;
    }

    // to flip an element:
    // 1. determine over which axis is the element being flipped (could be x,
    //    y, or both) indicated by `flipFactorX` & `flipFactorY`
    // 2. shift element's position by the amount of width or height (or both)
    //    or mirror points in the case of linear & freedraw elements
    // 3. adjust element angle
    let flip_factor_x = if flip_by_x { -1.0 } else { 1.0 };
    let flip_factor_y = if flip_by_y { -1.0 } else { 1.0 };

    let scene_map_owned: Vec<Element> = scene.non_deleted().into_iter().cloned().collect();
    let scene_map = ElementsMap::new(&scene_map_owned);
    let originals_map = ElementsMap::new(original_elements);

    let mut pending: Vec<PendingResize> = Vec::new();
    for (orig, latest) in &targets {
        // bounded text elements are updated along with their container elements
        if is_text(orig) && is_bound_to_container(orig) {
            continue;
        }

        let width = orig.base.width * scale_x;
        let height = orig.base.height * scale_y;
        let angle = normalize(orig.base.angle.0 * flip_factor_x * flip_factor_y);

        let is_linear_or_freedraw = is_linear(orig) || is_freedraw(orig);
        let offset_x = orig.base.x - anchor_x;
        let offset_y = orig.base.y - anchor_y;
        let shift_x = if flip_by_x && !is_linear_or_freedraw {
            width
        } else {
            0.0
        };
        let shift_y = if flip_by_y && !is_linear_or_freedraw {
            height
        } else {
            0.0
        };
        let x = anchor_x + flip_factor_x * (offset_x * scale_x + shift_x);
        let y = anchor_y + flip_factor_y * (offset_y * scale_y + shift_y);

        let rescaled_points =
            rescale_points_in_element(orig, width * flip_factor_x, height * flip_factor_y, false);

        let mut update = ElementUpdate {
            x: Some(x),
            y: Some(y),
            width: Some(width),
            height: Some(height),
            angle: Some(angle),
            points: rescaled_points.clone(),
            ..ElementUpdate::default()
        };

        if is_elbow_arrow(orig) {
            // Mirror fixed point binding for elbow arrows when resize goes
            // into the negative direction
            let mirror = |b: &FixedPointBinding| FixedPointBinding {
                fixed_point: [
                    if flip_by_x {
                        -b.fixed_point[0] + 1.0
                    } else {
                        b.fixed_point[0]
                    },
                    if flip_by_y {
                        -b.fixed_point[1] + 1.0
                    } else {
                        b.fixed_point[1]
                    },
                ],
                ..b.clone()
            };
            let (start, end) = bindings(orig);
            if let Some(start) = start {
                update.start_binding = Some(Some(mirror(start)));
            }
            if let Some(end) = end {
                update.end_binding = Some(Some(mirror(end)));
            }
            if let (ElementKind::Arrow(arrow), Some(points)) = (&orig.kind, &rescaled_points) {
                if let Some(Some(segments)) = &arrow.fixed_segments {
                    let at = |i: f64| -> LocalPoint {
                        if i >= 0.0 && i.fract() == 0.0 {
                            points.get(i as usize).copied()
                        } else {
                            None
                        }
                        .unwrap_or([f64::NAN, f64::NAN])
                    };
                    update.fixed_segments = Some(Some(
                        segments
                            .iter()
                            .map(|segment| FixedSegment {
                                start: at(segment.index - 1.0),
                                end: at(segment.index),
                                index: segment.index,
                            })
                            .collect(),
                    ));
                }
            }
        }

        if let ElementKind::Image(image) = &orig.kind {
            update.scale = Some([
                image.scale[0] * flip_factor_x,
                image.scale[1] * flip_factor_y,
            ]);
        }

        if is_text(orig) {
            let Some(size) = measure_font_size_from_width(orig, &scene_map, width) else {
                return;
            };
            update.font_size = Some(size);
        }

        let bound_text = get_bound_text_element_id(orig)
            .and_then(|id| originals_map.get(id))
            .and_then(text_fields);

        let mut bound_text_font_size = None;
        // sticky notes derive their label's size from the layout below
        if let Some(t) = bound_text.filter(|_| !is_sticky_note(orig)) {
            if keep_aspect_ratio {
                let new_font_size = t.font_size * scale;
                if new_font_size < MIN_FONT_SIZE {
                    return;
                }
                bound_text_font_size = Some(new_font_size);
            } else {
                bound_text_font_size = Some(t.font_size);
            }
        }

        pending.push(PendingResize {
            id: latest.base.id.clone(),
            update,
            bound_text_font_size,
        });
    }

    let elements_to_update: Vec<String> = pending.iter().map(|p| p.id.clone()).collect();

    for PendingResize {
        id,
        update,
        bound_text_font_size,
    } in pending
    {
        let angle = update.angle;
        scene.mutate_element(&id, update, env);

        let Some(element) = scene.get(&id).cloned() else {
            continue;
        };
        if is_sticky_note(&element) {
            // the content correction runs before the (single) arrow pass,
            // which must still skip arrows resized in the same gesture
            let intent = get_sticky_note_resize_intent(
                &element,
                original_elements,
                handle,
                keep_aspect_ratio,
                should_resize_from_center,
                flip_by_x || flip_by_y,
            );
            update_sticky_note_layout(&id, scene, env, intent, Some(Some(&elements_to_update)));
        } else {
            env.update_bound_elements(scene, &id, Some(&elements_to_update));
        }

        if scene.get(&id).is_some_and(is_arrow) {
            let start = scene
                .get(&id)
                .and_then(|e| bindings(e).0)
                .is_some_and(|b| !elements_to_update.contains(&b.element_id));
            if start {
                unbind_binding_element(scene, env, &id, true);
            }
            let end = scene
                .get(&id)
                .and_then(|e| bindings(e).1)
                .is_some_and(|b| !elements_to_update.contains(&b.element_id));
            if end {
                unbind_binding_element(scene, env, &id, false);
            }
        }

        let Some(element) = scene.get(&id).cloned() else {
            continue;
        };
        let bound_text = get_bound_text_element(&element, &scene.elements_map()).cloned();
        if let (Some(bound_text), Some(font_size)) = (bound_text, bound_text_font_size) {
            if truthy(font_size) && !is_sticky_note(&element) {
                scene.mutate_element(
                    &bound_text.base.id,
                    ElementUpdate {
                        font_size: Some(font_size),
                        angle: if is_linear(&element) { None } else { angle },
                        ..ElementUpdate::default()
                    },
                    env,
                );
                handle_bind_text_resize(
                    &id,
                    scene,
                    env,
                    Some(handle),
                    true,
                    should_resize_from_center,
                    flip_by_y,
                );
            }
        }
    }
}
