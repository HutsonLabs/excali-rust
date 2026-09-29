//! Text bound to an arrow endpoint: a text element that reads as a label
//! for the tip of an arrow, placed and anchored so the arrow itself never
//! moves.
//!
//! Upstream: `packages/element/src/arrowEndpointText.ts` (the whole file),
//! `dragNewTextElement` (`packages/element/src/dragElements.ts:227-292`) and
//! `App.arrowText.bindText` (`components/App.arrowText.ts:114-130`), at the
//! pinned commit. [`crate::text_editing::start_text_editing`] takes the
//! endpoint as its `arrow_endpoint` (`App.startTextEditing`'s
//! `arrowEndpoint`).

use excali_core::element::{Element, ElementKind, TextAlign, VerticalAlign};
use excali_math::{
    point_distance, point_from, point_from_vector, points_equal, vector, vector_dot,
    vector_from_point, vector_normalize, vector_scale, GlobalPoint,
};
use excali_scene::bounds::ElementsMap;
use excali_text::font_metadata::get_font_string;
use excali_text::new_element::get_text_anchor_ratios;
use excali_text::text_measurements::{get_min_text_element_width, TextMetricsProvider};

use crate::binding::{bind_binding_element_to_fixed_point, BindingEnd};
use crate::geometry::{normalize_fixed_point, BASE_BINDING_GAP};
use crate::linear_element_editor::{get_point_at_index_global_coordinates, POINT_HANDLE_SIZE};
use crate::scene::{MutationEnv, Scene};

/// `TEXT_AUTOWRAP_THRESHOLD` (`common/src/constants.ts:24`): how far (screen
/// px) a new text must be dragged out before it stops auto-growing and
/// wraps at the dragged width.
pub const TEXT_AUTOWRAP_THRESHOLD: f64 = 36.0;

/// `ArrowEndpoint` (`arrowEndpointText.ts:51-54`): an arrow and one of its
/// ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrowEndpoint {
    pub arrow_id: String,
    pub start_or_end: BindingEnd,
}

/// What [`get_text_binding_for_arrow_endpoint`] resolves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrowEndpointTextBinding {
    /// The text-local ratio the arrow binds to (a side midpoint).
    pub fixed_point: [f64; 2],
    pub text_align: TextAlign,
    pub vertical_align: VerticalAlign,
    /// Scene position the text's bound side midpoint should sit at.
    pub anchor: [f64; 2],
}

/// [`get_endpoint_bound_text_drag_anchor`]'s answer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragAnchor {
    /// 0 = anchored by its left edge, 1 = by its right, 0.5 = by its centre.
    pub anchor_ratio: f64,
    pub anchor_x: f64,
}

/// The update [`drag_new_text_element`] makes (`scene.mutateElement`'s
/// argument): `y` only when given `next_y`, `auto_resize` (always `false`)
/// only once the drag is past the autowrap threshold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewTextDrag {
    pub x: f64,
    pub y: Option<f64>,
    pub width: f64,
    pub auto_resize: Option<bool>,
}

fn is_elbow_arrow(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Arrow(a) if a.elbowed)
}

fn index_of(end: BindingEnd, from_start: isize, from_end: isize) -> isize {
    match end {
        BindingEnd::Start => from_start,
        BindingEnd::End => from_end,
    }
}

fn global(p: [f64; 2]) -> GlobalPoint {
    point_from(p[0], p[1])
}

/// `getUnboundArrowEndpointAtPoint(scenePointer, elements, elementsMap,
/// zoom)` (`arrowEndpointText.ts:61-120`): the free (unbound) arrow
/// endpoint under the pointer, front-most first, the end preferred over the
/// start; within the reach of a point handle (`POINT_HANDLE_SIZE + 1`
/// screen px). Locked arrows, arrows with fewer than two points and
/// endpoints on a zero-length tail are skipped. `elements` are the
/// non-deleted elements in scene order.
pub fn get_unbound_arrow_endpoint_at_point(
    scene_pointer: [f64; 2],
    elements: &[Element],
    elements_map: &ElementsMap<'_>,
    zoom: f64,
) -> Option<ArrowEndpoint> {
    // front to back, so the top-most arrow wins when endpoints overlap
    for element in elements.iter().rev() {
        let ElementKind::Arrow(arrow) = &element.kind else {
            continue;
        };
        let points = &arrow.linear.points;
        if element.base.locked || points.len() < 2 {
            continue;
        }

        // prefer the end (arrowhead) over the start when both are within range
        for end in [BindingEnd::End, BindingEnd::Start] {
            let bound = match end {
                BindingEnd::Start => &arrow.linear.start_binding,
                BindingEnd::End => &arrow.linear.end_binding,
            };
            if bound.is_some() {
                continue;
            }

            // a zero-length tail has no direction to place a text along
            let (tip, neighbor) = match end {
                BindingEnd::Start => (points[0], points[1]),
                BindingEnd::End => (points[points.len() - 1], points[points.len() - 2]),
            };
            if points_equal(global(tip), global(neighbor)) {
                continue;
            }

            let point =
                get_point_at_index_global_coordinates(element, index_of(end, 0, -1), elements_map);

            // same reach as grabbing a point handle in the linear editor
            if point_distance(global(scene_pointer), global(point)) * zoom < POINT_HANDLE_SIZE + 1.0
            {
                return Some(ArrowEndpoint {
                    arrow_id: element.base.id.clone(),
                    start_or_end: end,
                });
            }
        }
    }
    None
}

/// `getTextBindingForArrowEndpoint(arrow, startOrEnd, elementsMap,
/// targetStrokeWidth)` (`arrowEndpointText.ts:131-254`): how a text is
/// created so that it reads as a label for the endpoint. The arrow stays
/// fixed: the text's side the arrow already points at binds (its midpoint),
/// placed back along the arrow by the binding gap of the text's stroke
/// width (none for an elbow arrow), and the alignment pins that midpoint
/// while the text is typed. `None` when the endpoint's segment has no
/// length.
pub fn get_text_binding_for_arrow_endpoint(
    arrow: &Element,
    start_or_end: BindingEnd,
    elements_map: &ElementsMap<'_>,
    target_stroke_width: f64,
) -> Option<ArrowEndpointTextBinding> {
    let endpoint = global(get_point_at_index_global_coordinates(
        arrow,
        index_of(start_or_end, 0, -1),
        elements_map,
    ));
    let neighbor = global(get_point_at_index_global_coordinates(
        arrow,
        index_of(start_or_end, 1, -2),
        elements_map,
    ));

    if points_equal(endpoint, neighbor) {
        return None;
    }

    // The direction the arrow travels as it reaches this endpoint, snapped
    // to the dominant axis; a tie picks the horizontal axis.
    let direction = vector_from_point(endpoint, neighbor);
    #[derive(PartialEq)]
    enum Heading {
        Right,
        Left,
        Down,
        Up,
    }
    let heading = if direction.x.abs() >= direction.y.abs() {
        if direction.x >= 0.0 {
            Heading::Right
        } else {
            Heading::Left
        }
    } else if direction.y >= 0.0 {
        Heading::Down
    } else {
        Heading::Up
    };
    let heading_vector = match heading {
        Heading::Right => vector(1.0, 0.0),
        Heading::Left => vector(-1.0, 0.0),
        Heading::Down => vector(0.0, 1.0),
        Heading::Up => vector(0.0, -1.0),
    };

    // `getBindingGap({ strokeWidth: targetStrokeWidth })` (`binding.ts`):
    // elbow arrows terminate on the fixed point itself.
    let gap = if is_elbow_arrow(arrow) {
        0.0
    } else {
        BASE_BINDING_GAP + target_stroke_width / 2.0
    };

    // Walk back along the arrow so the anchor stays collinear with it.
    let away_from_tip = vector_normalize(vector_from_point(neighbor, endpoint));
    let along_heading = vector_dot(away_from_tip, heading_vector).abs();
    let anchor = point_from_vector(vector_scale(away_from_tip, -gap / along_heading), endpoint);
    let anchor = [anchor.x, anchor.y];

    let (fixed_point, text_align, vertical_align) = match heading {
        // text sits to the right of the tip -> bind its left side
        Heading::Right => ([0.0, 0.5], TextAlign::Left, VerticalAlign::Middle),
        // text sits to the left of the tip -> bind its right side
        Heading::Left => ([1.0, 0.5], TextAlign::Right, VerticalAlign::Middle),
        // text sits below the tip -> bind its top side
        Heading::Down => ([0.5, 0.0], TextAlign::Center, VerticalAlign::Top),
        // text sits above the tip -> bind its bottom side
        Heading::Up => ([0.5, 1.0], TextAlign::Center, VerticalAlign::Bottom),
    };
    Some(ArrowEndpointTextBinding {
        fixed_point: normalize_fixed_point(fixed_point),
        text_align,
        vertical_align,
        anchor,
    })
}

/// `isEndpointBoundText(text, elementsMap)` (`arrowEndpointText.ts:262-277`):
/// whether an arrow endpoint is bound to this text, checked on the arrows'
/// own bindings (of the arrows the text's `boundElements` lists).
pub fn is_endpoint_bound_text(text: &Element, elements_map: &ElementsMap<'_>) -> bool {
    let Some(bound) = &text.base.bound_elements else {
        return false;
    };
    bound.iter().any(|b| {
        if b.kind != excali_core::element::BoundElementType::Arrow {
            return false;
        }
        let Some(arrow) = elements_map.get(&b.id) else {
            return false;
        };
        let ElementKind::Arrow(a) = &arrow.kind else {
            return false;
        };
        let names = |binding: &Option<excali_core::element::FixedPointBinding>| {
            binding
                .as_ref()
                .is_some_and(|b| b.element_id == text.base.id)
        };
        names(&a.linear.start_binding) || names(&a.linear.end_binding)
    })
}

/// `getEndpointBoundTextDragAnchor(newElement)` (`arrowEndpointText.ts:280-288`):
/// the anchor a text bound to an arrow endpoint grows away from, by its
/// alignment.
pub fn get_endpoint_bound_text_drag_anchor(text: &Element) -> DragAnchor {
    let (text_align, vertical_align) = match &text.kind {
        ElementKind::Text(t) => (t.text_align, t.vertical_align),
        _ => (TextAlign::default(), VerticalAlign::default()),
    };
    let anchor_ratio = get_text_anchor_ratios(text_align, vertical_align)[0];
    DragAnchor {
        anchor_ratio,
        anchor_x: text.base.x + text.base.width * anchor_ratio,
    }
}

/// `dragNewTextElement({ newElement, anchorX, anchorRatio, pointerX, nextY,
/// zoom, scene })` (`dragElements.ts:227-292`): the update sizing a text as
/// it is dragged out, pinned at `anchor_x` (`anchor_ratio` along its box):
/// the width is the pointer's reach away from the anchor (twice that for a
/// centred box), at least `getMinTextElementWidth`, and the text stops
/// auto-growing once the reach passes `TEXT_AUTOWRAP_THRESHOLD / zoom`.
pub fn drag_new_text_element(
    text: &Element,
    anchor_x: f64,
    anchor_ratio: f64,
    pointer_x: f64,
    next_y: Option<f64>,
    zoom: f64,
    provider: &dyn TextMetricsProvider,
) -> NewTextDrag {
    let offset = pointer_x - anchor_x;

    // how far the pointer has travelled away from the anchor along the
    // direction the box may grow; negative once it heads back
    let reach = if anchor_ratio == 0.0 {
        offset
    } else if anchor_ratio == 1.0 {
        -offset
    } else {
        offset.abs()
    };

    let min_width = match &text.kind {
        ElementKind::Text(t) => get_min_text_element_width(
            &get_font_string(t.font_size, t.font_family),
            t.line_height,
            provider,
        ),
        _ => 0.0,
    };
    let width = excali_math::js::max(
        // a centred box grows on both sides, so it widens at twice the reach
        if anchor_ratio == 0.5 {
            reach * 2.0
        } else {
            reach
        },
        min_width,
    );

    NewTextDrag {
        x: anchor_x - width * anchor_ratio,
        y: next_y,
        width,
        auto_resize: (reach > TEXT_AUTOWRAP_THRESHOLD / zoom).then_some(false),
    }
}

/// `App.arrowText.bindText(arrowEndpoint, text, fixedPoint)`
/// (`App.arrowText.ts:114-130`): binds the endpoint to the created text at
/// the fixed point [`get_text_binding_for_arrow_endpoint`] resolved
/// (`bindBindingElementToFixedPoint`, `binding.ts:3216-3234`).
pub fn bind_text_to_arrow_endpoint(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    endpoint: &ArrowEndpoint,
    text_id: &str,
    fixed_point: [f64; 2],
) {
    bind_binding_element_to_fixed_point(
        scene,
        env,
        &endpoint.arrow_id,
        text_id,
        endpoint.start_or_end,
        fixed_point,
    );
}
