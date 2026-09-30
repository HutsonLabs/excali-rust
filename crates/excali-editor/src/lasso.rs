//! The lasso: which elements a freehand selection path takes
//! (`packages/excalidraw/lasso/utils.ts`, `getLassoSelectedElementIds`,
//! and `lasso/index.ts`, `LassoTrail`), without the trail's drawing.
//!
//! Each new point reselects from the whole path. An element whose box the
//! path's box touches is tested against the path closed into a polygon
//! (non-zero winding): in `contain` mode (the default `boxSelectionMode`)
//! it is taken when every end of every segment of its outline (and its
//! label's) is inside or on the path and the path crosses neither, and a
//! group only when all its members are; in `overlap` mode as soon as one
//! end is inside or the path crosses its outline or label. An element
//! clipped by its frame is tested on its visible part only. Locked
//! elements and bound text are never taken. The path is simplified by
//! 5 / zoom first (Ramer–Douglas–Peucker, `points-on-curve`).
//!
//! The trail then selects what was taken (`selectElementsFromIds`): with
//! the previous selection when kept (Shift), a frame's children dropped
//! for the frame, whole groups, and the linear element editor for a lone
//! line or arrow.
//!
//! The trail's points are its `originalPoints`: the pointer positions, a
//! repeat of the last one dropped (`LaserPointer.addPoint`).

use std::collections::HashMap;

use excali_core::element::{Element, ElementKind};
use excali_math::{
    line_segment, point_on_line_segment, polygon_from_points, polygon_includes_point_non_zero,
    Global, GlobalPoint, LineSegment, Point, PRECISION,
};
use excali_rough::points_on_curve::simplify;
use excali_scene::bounds::{
    bounds_contain_bounds, do_bounds_intersect, get_bound_text_element, get_element_bounds,
    get_element_line_segments, point_inside_bounds_inclusive, Bounds, ElementsMap,
};
use excali_scene::frame::{element_overlaps_with_frame, is_frame_like};
use excali_scene::render_element::get_containing_frame;
use excali_text::text_element::compute_bound_text_position;
use indexmap::IndexSet;
use serde_json::{Map, Value};

use crate::collision::intersect_element_with_line_segment;
use crate::groups::select_groups_for_selected_elements;
use crate::selection::BoxSelectionMode;
use crate::text_layout::SceneArrowGeometry;

/// `ElementsSegmentsMap`: each element's outline as segments, by id.
pub type ElementsSegments = HashMap<String, Vec<LineSegment<Global>>>;

fn gp([x, y]: [f64; 2]) -> GlobalPoint {
    Point::new(x, y)
}

/// `isBoundToContainer`: text naming a container.
fn is_bound_to_container(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Text(t) if t.container_id.is_some())
}

/// The segments of each element (`getElementLineSegments`), as
/// `LassoTrail.updateSelection` gathers them; an element whose shape
/// cannot be built has none.
pub fn get_elements_segments(
    elements: &[&Element],
    elements_map: &ElementsMap<'_>,
) -> ElementsSegments {
    elements
        .iter()
        .map(|e| {
            let segments = get_element_line_segments(e, elements_map).unwrap_or_default();
            (e.base.id.clone(), segments)
        })
        .collect()
}

/// The input of `getLassoSelectedElementIds` (`utils.ts:37-46`).
pub struct LassoInput<'a> {
    /// The path, in scene coordinates.
    pub lasso_path: &'a [[f64; 2]],
    /// The candidates, in scene order.
    pub elements: &'a [&'a Element],
    /// The scene's non-deleted elements.
    pub elements_map: &'a ElementsMap<'a>,
    pub elements_segments: &'a ElementsSegments,
    /// Simplify the path by this distance first (none when `None` or 0).
    pub simplify_distance: Option<f64>,
    pub mode: BoxSelectionMode,
}

/// `getLassoSelectedElementIds(input)` (`utils.ts:37-140`): the ids the
/// path takes, the enclosed ones in scene order (`contain`), or the
/// crossed ones followed by the enclosed ones (`overlap`).
pub fn get_lasso_selected_element_ids(input: &LassoInput<'_>) -> Vec<String> {
    let lasso_path = input.lasso_path;
    let path: Vec<[f64; 2]> = match input.simplify_distance {
        Some(d) if d != 0.0 && !d.is_nan() => {
            simplify(lasso_path, d).unwrap_or_else(|_| lasso_path.to_vec())
        }
        _ => lasso_path.to_vec(),
    };
    let lasso_bounds: Bounds = lasso_path.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |acc, p| {
            [
                excali_math::js::min(acc[0], p[0]),
                excali_math::js::min(acc[1], p[1]),
                excali_math::js::max(acc[2], p[0]),
                excali_math::js::max(acc[3], p[1]),
            ]
        },
    );
    let lasso_polygon = polygon_from_points(path.iter().copied().map(gp).collect());
    let contain_bounds: Bounds = [
        lasso_bounds[0] - PRECISION,
        lasso_bounds[1] - PRECISION,
        lasso_bounds[2] + PRECISION,
        lasso_bounds[3] + PRECISION,
    ];
    let contain = input.mode == BoxSelectionMode::Contain;
    let mut everything: Option<Vec<Element>> = None;

    let mut enclosed: IndexSet<String> = IndexSet::new();
    let mut intersected: IndexSet<String> = IndexSet::new();
    for element in input
        .elements
        .iter()
        .copied()
        .filter(|e| !e.base.locked && !is_bound_to_container(e))
    {
        // only the visible (frame-clipped) part of an element counts
        let bounds = get_element_bounds(element, input.elements_map);
        let clip_bounds = get_frame_clip_bounds(element, bounds, input.elements_map);
        // the boxes first: much cheaper than the shapes
        let element_bounds = clip_to_bounds(bounds, clip_bounds);
        if !do_bounds_intersect(lasso_bounds, element_bounds)
            || intersected.contains(&element.base.id)
            || enclosed.contains(&element.base.id)
        {
            continue;
        }
        if contain && !bounds_contain_bounds(contain_bounds, element_bounds) {
            continue;
        }
        let segments = get_selection_segments(
            element,
            input.elements_map,
            input.elements_segments,
            clip_bounds,
        );
        let is_enclosed = enclosure_test(&lasso_polygon, &segments, element_bounds, contain);
        let mut crosses = || {
            intersection_test(
                &path,
                element,
                input.elements_map,
                clip_bounds,
                &mut everything,
            )
        };
        if contain {
            if is_enclosed && !crosses() {
                enclosed.insert(element.base.id.clone());
            }
        } else if is_enclosed {
            enclosed.insert(element.base.id.clone());
        } else if crosses() {
            intersected.insert(element.base.id.clone());
        }
    }

    if contain {
        exclude_incomplete_groups(enclosed.into_iter().collect(), input.elements_map)
    } else {
        intersected.into_iter().chain(enclosed).collect()
    }
}

/// `getFrameClipBounds` (`utils.ts:146-167`): the bounds of the frame
/// that clips the element, or `None` when nothing of it is clipped.
fn get_frame_clip_bounds(
    element: &Element,
    element_bounds: Bounds,
    elements_map: &ElementsMap<'_>,
) -> Option<Bounds> {
    let frame = get_containing_frame(element, elements_map)?;
    let frame_bounds = get_element_bounds(frame, elements_map);
    // nothing is clipped away from an element fitting inside its frame
    if bounds_contain_bounds(frame_bounds, element_bounds) {
        return None;
    }
    element_overlaps_with_frame(element, frame, elements_map)
        .unwrap_or(false)
        .then_some(frame_bounds)
}

/// `clipToBounds` (`utils.ts:169-177`).
fn clip_to_bounds(bounds: Bounds, clip: Option<Bounds>) -> Bounds {
    match clip {
        Some(c) => [
            excali_math::js::max(bounds[0], c[0]),
            excali_math::js::max(bounds[1], c[1]),
            excali_math::js::min(bounds[2], c[2]),
            excali_math::js::min(bounds[3], c[3]),
        ],
        None => bounds,
    }
}

/// `getSelectionSegments` (`utils.ts:183-201`): the element's outline and
/// its label's, clipped to its frame.
fn get_selection_segments(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    elements_segments: &ElementsSegments,
    clip_bounds: Option<Bounds>,
) -> Vec<LineSegment<Global>> {
    let mut segments = elements_segments
        .get(&element.base.id)
        .cloned()
        .unwrap_or_default();
    if let Some(label) = get_bound_text_element(element, elements_map) {
        if let Some(label_segments) = elements_segments.get(&label.base.id) {
            segments.extend_from_slice(label_segments);
        }
    }
    match clip_bounds {
        Some(bounds) => clip_segments_to_bounds(&segments, bounds),
        None => segments,
    }
}

/// `clipSegmentsToBounds` (`utils.ts:203-257`): Liang–Barsky, keeping the
/// part of each segment inside the bounds.
fn clip_segments_to_bounds(
    segments: &[LineSegment<Global>],
    [min_x, min_y, max_x, max_y]: Bounds,
) -> Vec<LineSegment<Global>> {
    let mut clipped = Vec::new();
    for segment in segments {
        let (x1, y1) = (segment.0.x, segment.0.y);
        let (x2, y2) = (segment.1.x, segment.1.y);
        let dx = x2 - x1;
        let dy = y2 - y1;
        let mut t0 = 0.0;
        let mut t1 = 1.0;
        let mut inside = true;
        // each of the four edges in turn narrows [t0, t1] to the part inside
        for (p, q) in [
            (-dx, x1 - min_x),
            (dx, max_x - x1),
            (-dy, y1 - min_y),
            (dy, max_y - y1),
        ] {
            if p == 0.0 {
                // parallel to this edge: all inside or all outside
                if q < 0.0 {
                    inside = false;
                    break;
                }
                continue;
            }
            let t = q / p;
            if p < 0.0 {
                if t > t1 {
                    inside = false;
                    break;
                }
                t0 = excali_math::js::max(t0, t);
            } else {
                if t < t0 {
                    inside = false;
                    break;
                }
                t1 = excali_math::js::min(t1, t);
            }
        }
        if inside {
            clipped.push(line_segment(
                Point::new(x1 + t0 * dx, y1 + t0 * dy),
                Point::new(x1 + t1 * dx, y1 + t1 * dy),
            ));
        }
    }
    clipped
}

/// `excludeIncompleteGroups` (`utils.ts:266-309`): in `contain` mode a
/// group is only taken when all its (selectable) members are; outermost
/// groups only.
fn exclude_incomplete_groups(selected: Vec<String>, elements_map: &ElementsMap<'_>) -> Vec<String> {
    let outermost = |id: &str| {
        elements_map
            .get(id)
            .and_then(|e| e.base.group_ids.last().cloned())
    };
    if !selected.iter().any(|id| {
        elements_map
            .get(id)
            .is_some_and(|e| !e.base.group_ids.is_empty())
    }) {
        return selected;
    }
    let mut groups: HashMap<String, Vec<&str>> = HashMap::new();
    for element in elements_map.values() {
        // locked elements and bound text never count
        let Some(group_id) = element.base.group_ids.last() else {
            continue;
        };
        if element.base.locked || is_bound_to_container(element) {
            continue;
        }
        groups
            .entry(group_id.clone())
            .or_default()
            .push(element.base.id.as_str());
    }
    if groups.is_empty() {
        return selected;
    }
    let taken: IndexSet<&str> = selected.iter().map(String::as_str).collect();
    selected
        .iter()
        .filter(|id| match outermost(id).and_then(|g| groups.get(&g)) {
            Some(group) => group.iter().all(|m| taken.contains(m)),
            None => true,
        })
        .cloned()
        .collect()
}

/// `enclosureTest` (`utils.ts:311-332`): every end of every segment
/// (`contain`, on the path counting) or some end (`overlap`) inside the
/// path; an element without an outline by the corners of its box.
fn enclosure_test(
    lasso_polygon: &[GlobalPoint],
    segments: &[LineSegment<Global>],
    element_bounds: Bounds,
    contain: bool,
) -> bool {
    let includes = |p: GlobalPoint| {
        polygon_includes_point_non_zero(p, lasso_polygon)
            || (contain && is_point_on_polygon(p, lasso_polygon))
    };
    if segments.is_empty() {
        let [min_x, min_y, max_x, max_y] = element_bounds;
        let corners = [
            gp([min_x, min_y]),
            gp([max_x, min_y]),
            gp([max_x, max_y]),
            gp([min_x, max_y]),
        ];
        return if contain {
            corners.into_iter().all(includes)
        } else {
            corners.into_iter().any(includes)
        };
    }
    if contain {
        segments.iter().all(|s| includes(s.0) && includes(s.1))
    } else {
        segments.iter().any(|s| includes(s.0) || includes(s.1))
    }
}

/// `isPointOnPolygon` (`utils.ts:341-349`).
fn is_point_on_polygon(point: GlobalPoint, polygon: &[GlobalPoint]) -> bool {
    polygon
        .windows(2)
        .any(|w| point_on_line_segment(point, line_segment(w[0], w[1])))
}

/// `intersectionTest` (`utils.ts:351-400`): whether a segment of the
/// path, closed, crosses the element's outline or its label's; a crossing
/// its frame clips away does not count.
fn intersection_test(
    lasso_path: &[[f64; 2]],
    element: &Element,
    elements_map: &ElementsMap<'_>,
    clip_bounds: Option<Bounds>,
    everything: &mut Option<Vec<Element>>,
) -> bool {
    let Some(&last) = lasso_path.last() else {
        return false;
    };
    let lasso_segments = lasso_path
        .windows(2)
        .map(|w| [w[0], w[1]])
        .chain([[last, lasso_path[0]]]);

    let intersects = |target: &Element, segment: [[f64; 2]; 2]| {
        let hits = intersect_element_with_line_segment(
            target,
            elements_map,
            segment,
            0.0,
            clip_bounds.is_none(),
        );
        match clip_bounds {
            Some(clip) => hits.iter().any(|h| point_inside_bounds_inclusive(*h, clip)),
            None => !hits.is_empty(),
        }
    };

    // the label where its container puts it
    let label = get_bound_text_element(element, elements_map).and_then(|label| {
        let all =
            everything.get_or_insert_with(|| elements_map.values().map(|e| (*e).clone()).collect());
        let [x, y] = compute_bound_text_position(element, label, all, &mut SceneArrowGeometry)?;
        let mut placed = label.clone();
        placed.base.x = x;
        placed.base.y = y;
        Some(placed)
    });

    lasso_segments.into_iter().any(|segment| {
        intersects(element, segment) || label.as_ref().is_some_and(|l| intersects(l, segment))
    })
}

/// The selection a trail op leaves: the app state's
/// `selectedElementIds`, `selectedGroupIds` and `selectedLinearElement`
/// (the id of the element its editor holds).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LassoSelection {
    pub selected_element_ids: Map<String, Value>,
    pub selected_group_ids: Map<String, Value>,
    pub selected_linear_element: Option<String>,
}

/// What the trail reads from the app.
#[derive(Debug, Clone)]
pub struct LassoScene<'a> {
    /// The scene's non-deleted elements, in order.
    pub elements: Vec<&'a Element>,
    /// The elements on screen (`App.visibleElements`), which the lasso
    /// tests.
    pub visible: Vec<&'a Element>,
    pub zoom: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
    /// `appState.boxSelectionMode`.
    pub mode: BoxSelectionMode,
    /// The selection before the op.
    pub selected_element_ids: Map<String, Value>,
    pub editing_group_id: Option<String>,
}

/// `LassoTrail` (`lasso/index.ts`): the path and what it selected.
#[derive(Debug, Clone, Default)]
pub struct LassoTrail {
    points: Vec<[f64; 2]>,
    active: bool,
    keep_previous_selection: bool,
    /// The segments of the visible elements, for the viewport they were
    /// gathered at (`[scrollX, scrollY, zoom]`).
    elements_segments: Option<([f64; 3], ElementsSegments)>,
}

impl LassoTrail {
    /// `hasCurrentTrail`.
    pub fn has_current_trail(&self) -> bool {
        self.active
    }

    /// The path so far (`originalPoints`).
    pub fn points(&self) -> &[[f64; 2]] {
        &self.points
    }

    /// `startPath(x, y, keepPreviousSelection)` (`index.ts:76-92`): a new
    /// path at the point; unless the previous selection is kept, the
    /// selection to clear it to.
    pub fn start_path(&mut self, x: f64, y: f64, keep: bool) -> Option<LassoSelection> {
        self.end_path();
        self.points.push([x, y]);
        self.active = true;
        self.keep_previous_selection = keep;
        (!keep).then(LassoSelection::default)
    }

    /// `addPointToPath(x, y, keepPreviousSelection)` (`index.ts:152-158`):
    /// the point added and the selection the whole path now takes
    /// (`updateSelection`, `:160-196`); `None` without a path.
    pub fn add_point_to_path(
        &mut self,
        x: f64,
        y: f64,
        keep: bool,
        scene: &LassoScene<'_>,
    ) -> Option<LassoSelection> {
        if self.active && self.points.last() != Some(&[x, y]) {
            self.points.push([x, y]);
        }
        self.keep_previous_selection = keep;
        if !self.active {
            return None;
        }
        let translate = [scene.scroll_x, scene.scroll_y, scene.zoom];
        if self
            .elements_segments
            .as_ref()
            .is_none_or(|(at, _)| *at != translate)
        {
            let visible_map = ElementsMap::new(scene.visible.iter().copied());
            let segments = get_elements_segments(&scene.visible, &visible_map);
            self.elements_segments = Some((translate, segments));
        }
        let (_, segments) = self.elements_segments.as_ref()?;
        let elements_map = ElementsMap::new(scene.elements.iter().copied());
        let ids = get_lasso_selected_element_ids(&LassoInput {
            lasso_path: &self.points,
            elements: &scene.visible,
            elements_map: &elements_map,
            elements_segments: segments,
            simplify_distance: Some(5.0 / scene.zoom),
            mode: scene.mode,
        });
        Some(self.select_elements_from_ids(&ids, scene))
    }

    /// `selectElementsFromIds(ids)` (`index.ts:94-150`): the ids selected,
    /// with the previous selection when kept, a selected frame's children
    /// dropped, whole groups, and the linear element editor for a lone
    /// line or arrow.
    pub fn select_elements_from_ids(
        &self,
        ids: &[String],
        scene: &LassoScene<'_>,
    ) -> LassoSelection {
        let mut next: Map<String, Value> = Map::new();
        for id in ids {
            next.insert(id.clone(), Value::Bool(true));
        }
        if self.keep_previous_selection {
            for id in scene.selected_element_ids.keys() {
                next.insert(id.clone(), Value::Bool(true));
            }
        }
        let find = |id: &str| scene.elements.iter().copied().find(|e| e.base.id == id);
        // a selected frame stands for its children
        let frames: Vec<String> = next
            .keys()
            .filter(|id| find(id).is_some_and(is_frame_like))
            .cloned()
            .collect();
        for frame in &frames {
            for child in scene
                .elements
                .iter()
                .filter(|e| e.base.frame_id.as_deref() == Some(frame.as_str()))
            {
                next.remove(&child.base.id);
            }
        }
        let selection = select_groups_for_selected_elements(
            &next,
            scene.editing_group_id.as_deref(),
            &scene.elements,
        );
        let selected: Vec<&String> = selection.selected_element_ids.keys().collect();
        let selected_linear_element = match selected.as_slice() {
            [id] if selection.selected_group_ids.is_empty() => find(id)
                .filter(|e| e.element_type().is_linear())
                .map(|e| e.base.id.clone()),
            _ => None,
        };
        LassoSelection {
            selected_element_ids: selection.selected_element_ids,
            selected_group_ids: selection.selected_group_ids,
            selected_linear_element,
        }
    }

    /// `endPath()` (`index.ts:198-204`): the path and its segments gone.
    pub fn end_path(&mut self) {
        self.points.clear();
        self.active = false;
        self.elements_segments = None;
    }
}
