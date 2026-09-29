//! Box selection: which elements a selection box takes
//! (`packages/element/src/selection.ts:33-100`, `elementsOverlappingBBox`
//! of `packages/element/src/bounds.ts:1273-1545`).
//!
//! In `contain` mode (upstream's default `boxSelectionMode`) an element is
//! taken when the box wraps its bounds grown by half its stroke (and its
//! label's, for an arrow); in `overlap` mode also when the box crosses its
//! label or its outline. A group is taken whole: in `contain` mode only
//! when every selectable member is in the box, in `overlap` mode as soon as
//! one is. Locked elements and bound text are never taken; with
//! `exclude_elements_in_frames`, neither are the children of a frame the
//! box takes.

use std::collections::{HashMap, HashSet};

use excali_core::element::{Element, ElementKind};
use excali_math::{js, point_rotate_rads, GlobalPoint, Point, Radians};
use excali_scene::bounds::{
    bounds_contain_bounds, do_bounds_intersect, element_center_point, get_bound_text_element_id,
    get_element_bounds, get_element_bounds_non_rotated, Bounds, ElementsMap,
};
use excali_scene::frame::{element_overlaps_with_frame, is_frame_like};
use excali_scene::linear_element::get_bound_text_element_position;
use excali_scene::render_element::get_containing_frame;

use crate::collision::intersect_element_with_line_segment;

/// `BoxSelectionMode` (`types.ts`): `appState.boxSelectionMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxSelectionMode {
    /// The box must wrap the element (the default).
    #[default]
    Contain,
    /// The box may cross the element.
    Overlap,
}

impl BoxSelectionMode {
    /// `"contain"` or `"overlap"`; anything else is the default.
    pub fn from_name(name: &str) -> BoxSelectionMode {
        match name {
            "overlap" => BoxSelectionMode::Overlap,
            _ => BoxSelectionMode::Contain,
        }
    }
}

/// `shouldIgnoreElementFromSelection` (`selection.ts:33-34`).
fn should_ignore(element: &Element) -> bool {
    element.base.locked || matches!(&element.kind, ElementKind::Text(t) if t.container_id.is_some())
}

/// `pointInsideBounds` (`math/src/point.ts`): strictly inside.
fn point_inside_bounds(p: GlobalPoint, [x1, y1, x2, y2]: Bounds) -> bool {
    p.x > x1 && p.x < x2 && p.y > y1 && p.y < y2
}

/// `bounds` cut to `frame`'s.
fn clip_to([a, b, c, d]: Bounds, [fx1, fy1, fx2, fy2]: Bounds) -> Bounds {
    [
        js::max(a, fx1),
        js::max(b, fy1),
        js::min(c, fx2),
        js::min(d, fy2),
    ]
}

fn gp([x, y]: [f64; 2]) -> GlobalPoint {
    Point::new(x, y)
}

/// `elementsOverlappingBBox({ elements, elementsMap, bounds, type,
/// excludeElementsInFrames, shouldIgnoreElementFromSelection })`
/// (`bounds.ts:1273-1545`), with `selectionBounds` given as bounds and the
/// selection's ignore rule. The elements it takes, in scene order.
pub fn elements_overlapping_bbox<'a>(
    elements: &[&'a Element],
    elements_map: &ElementsMap<'_>,
    selection_bounds: Bounds,
    mode: BoxSelectionMode,
    exclude_elements_in_frames: bool,
) -> Vec<&'a Element> {
    let [sx1, sy1, sx2, sy2] = selection_bounds;
    let selection_edges = [
        [[sx1, sy1], [sx2, sy1]],
        [[sx2, sy1], [sx2, sy2]],
        [[sx2, sy2], [sx1, sy2]],
        [[sx1, sy2], [sx1, sy1]],
    ];
    let mut frames_in_selection: Option<HashSet<&str>> =
        exclude_elements_in_frames.then(HashSet::new);
    let mut groups: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut in_selection: HashSet<&str> = HashSet::new();

    for &element in elements {
        if should_ignore(element) {
            continue;
        }
        // only selectable top-level group members count for the group
        if let Some(group_id) = element.base.group_ids.last() {
            groups
                .entry(group_id.as_str())
                .or_default()
                .push(element.base.id.as_str());
        }

        let stroke_width = element.base.stroke_width;
        let mut label_aabb: Option<Bounds> = None;
        let [x1, y1, x2, y2] = get_element_bounds(element, elements_map);
        let mut element_aabb = [
            x1 - stroke_width / 2.0,
            y1 - stroke_width / 2.0,
            x2 + stroke_width / 2.0,
            y2 + stroke_width / 2.0,
        ];

        // an arrow's bounds take its label in
        if matches!(element.kind, ElementKind::Arrow(_)) {
            let label = get_bound_text_element_id(element)
                .and_then(|id| elements_map.get(id))
                .filter(|t| !t.base.is_deleted);
            if let Some(label) = label {
                let [x, y] = get_bound_text_element_position(element, label, elements_map);
                label_aabb = Some([x, y, x + label.base.width, y + label.base.height]);
            }
        }

        // only the part of the element its frame shows can be selected
        if let Some(frame) = get_containing_frame(element, elements_map) {
            if element_overlaps_with_frame(element, frame, elements_map).unwrap_or(false) {
                let frame_aabb = get_element_bounds(frame, elements_map);
                element_aabb = clip_to(element_aabb, frame_aabb);
                label_aabb = label_aabb.map(|l| clip_to(l, frame_aabb));
            }
        }

        let common_aabb = match label_aabb {
            Some([a, b, c, d]) => [
                js::min(a, element_aabb[0]),
                js::min(b, element_aabb[1]),
                js::max(c, element_aabb[2]),
                js::max(d, element_aabb[3]),
            ],
            None => element_aabb,
        };

        // 1. the box wraps the element's box: taken, whatever the mode
        if bounds_contain_bounds(selection_bounds, common_aabb) {
            if let Some(frames) = frames_in_selection.as_mut() {
                if is_frame_like(element) {
                    frames.insert(element.base.id.as_str());
                }
            }
            in_selection.insert(element.base.id.as_str());
            continue;
        }

        // 2. the box crosses the label
        if mode == BoxSelectionMode::Overlap
            && label_aabb.is_some_and(|l| do_bounds_intersect(selection_bounds, l))
        {
            in_selection.insert(element.base.id.as_str());
            continue;
        }

        // 3. the box crosses the outline
        if mode == BoxSelectionMode::Overlap && do_bounds_intersect(selection_bounds, element_aabb)
        {
            let center = gp(element_center_point(element, elements_map));
            let angle = Radians(element.base.angle.0);
            let mut hit = match element.kind.points() {
                Some(points)
                    if matches!(
                        element.kind,
                        ElementKind::Line(_) | ElementKind::Arrow(_) | ElementKind::Freedraw(_)
                    ) =>
                {
                    points.iter().any(|p| {
                        let rotated = point_rotate_rads(
                            gp([element.base.x + p[0], element.base.y + p[1]]),
                            center,
                            angle,
                        );
                        point_inside_bounds(rotated, selection_bounds)
                    })
                }
                _ => {
                    let [nx1, ny1, nx2, ny2] =
                        get_element_bounds_non_rotated(element, elements_map);
                    [
                        [(nx1 + nx2) / 2.0, ny1],
                        [nx2, (ny1 + ny2) / 2.0],
                        [(nx1 + nx2) / 2.0, ny2],
                        [nx1, (ny1 + ny2) / 2.0],
                    ]
                    .into_iter()
                    .any(|p| {
                        // rotated twice, as upstream does
                        let once = point_rotate_rads(gp(p), center, angle);
                        point_inside_bounds(
                            point_rotate_rads(once, center, angle),
                            selection_bounds,
                        )
                    })
                }
            };
            if !hit {
                hit = selection_edges.iter().any(|edge| {
                    !intersect_element_with_line_segment(
                        element,
                        elements_map,
                        *edge,
                        stroke_width / 2.0,
                        true,
                    )
                    .is_empty()
                });
            }
            if hit {
                if let Some(frames) = frames_in_selection.as_mut() {
                    if is_frame_like(element) {
                        frames.insert(element.base.id.as_str());
                    }
                }
                in_selection.insert(element.base.id.as_str());
                continue;
            }
        }
    }

    if let Some(frames) = &frames_in_selection {
        in_selection.retain(|id| {
            elements_map
                .get(id)
                .and_then(|e| e.base.frame_id.as_deref())
                .is_none_or(|f| !frames.contains(f))
        });
    }

    let group_of = |id: &str| {
        elements_map
            .get(id)
            .and_then(|e| e.base.group_ids.last())
            .and_then(|g| groups.get(g.as_str()))
    };
    match mode {
        BoxSelectionMode::Overlap => {
            let taken: Vec<&str> = in_selection.iter().copied().collect();
            for id in taken {
                if let Some(group) = group_of(id) {
                    in_selection.extend(group.iter().copied());
                }
            }
        }
        BoxSelectionMode::Contain => {
            let whole: HashSet<&str> = in_selection
                .iter()
                .copied()
                .filter(|id| {
                    group_of(id).is_none_or(|g| g.iter().all(|m| in_selection.contains(m)))
                })
                .collect();
            in_selection = whole;
        }
    }

    elements
        .iter()
        .copied()
        .filter(|e| in_selection.contains(e.base.id.as_str()))
        .collect()
}

/// `getElementsWithinSelection(elements, selection, elementsMap,
/// excludeElementsInFrames, boxSelectionMode)` (`selection.ts:70-100`),
/// the selection element given by its corners (`getElementAbsoluteCoords`
/// of the selection element), in any order.
pub fn get_elements_within_selection<'a>(
    elements: &[&'a Element],
    selection: Bounds,
    elements_map: &ElementsMap<'_>,
    exclude_elements_in_frames: bool,
    mode: BoxSelectionMode,
) -> Vec<&'a Element> {
    let [ax, ay, bx, by] = selection;
    let bounds = [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)];
    elements_overlapping_bbox(
        elements,
        elements_map,
        bounds,
        mode,
        exclude_elements_in_frames,
    )
}
