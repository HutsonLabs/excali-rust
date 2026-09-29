//! The eraser: which elements a pointer path erases
//! (`packages/excalidraw/eraser/index.ts`, `EraserTrail`), without the
//! trail's drawing.
//!
//! Each new point of the path tests its last segment against every visible
//! unlocked element (`eraserTest`): a filled or closed shape the segment
//! ends inside, a freedraw stroke within reach of its outline, an arrow or
//! open line within reach of its segments, or anything whose outline (or
//! label) the segment crosses. An element taken takes its whole outermost
//! group, its label and its container with it. With Alt held
//! (`restore`), the path gives elements back instead.
//!
//! The trail's points are its `originalPoints`: the pointer positions, a
//! repeat of the last one dropped (`LaserPointer.addPoint`).

use std::collections::HashSet;

use excali_core::element::{Element, ElementKind};
use excali_math::{
    line_segment, line_segments_distance, polygon_includes_point_non_zero, GlobalPoint, Point,
};
use excali_math::{point_rotate_rads, Radians};
use excali_scene::bounds::{
    do_bounds_intersect, get_bound_text_element, get_bound_text_element_id, get_element_bounds,
    get_element_line_segments, ElementsMap,
};
use excali_scene::freedraw::get_freedraw_outline_points;
use excali_scene::geometric_shape::should_test_inside;
use excali_text::text_element::compute_bound_text_position;
use indexmap::IndexSet;

use crate::collision::{intersect_element_with_line_segment, is_point_in_element};
use crate::groups::get_elements_in_group;
use crate::text_layout::SceneArrowGeometry;

fn gp([x, y]: [f64; 2]) -> GlobalPoint {
    Point::new(x, y)
}

/// `EraserTrail`: the path and what it erases.
#[derive(Debug, Clone, Default)]
pub struct EraserTrail {
    points: Vec<[f64; 2]>,
    elements_to_erase: IndexSet<String>,
    groups_to_erase: HashSet<String>,
}

fn container_id(element: &Element) -> Option<&str> {
    match &element.kind {
        ElementKind::Text(t) => t.container_id.as_deref(),
        _ => None,
    }
}

impl EraserTrail {
    /// `startPath(x, y)`: a new path at the point, nothing to erase.
    pub fn start_path(&mut self, x: f64, y: f64) {
        self.end_path();
        self.points.push([x, y]);
    }

    /// `endPath()`.
    pub fn end_path(&mut self) {
        self.points.clear();
        self.elements_to_erase.clear();
        self.groups_to_erase.clear();
    }

    /// The ids to erase, in the order they were taken.
    pub fn elements_to_erase(&self) -> Vec<String> {
        self.elements_to_erase.iter().cloned().collect()
    }

    /// `addPointToPath(x, y, restore)` with the app's visible elements
    /// (`app.visibleElements`; locked ones are skipped) at `zoom`: the ids
    /// to erase so far.
    pub fn add_point_to_path(
        &mut self,
        x: f64,
        y: f64,
        restore: bool,
        visible: &[&Element],
        zoom: f64,
    ) -> Vec<String> {
        if self.points.last() != Some(&[x, y]) {
            self.points.push([x, y]);
        }
        let n = self.points.len();
        if n < 2 {
            return Vec::new();
        }
        let segment = [self.points[n - 1], self.points[n - 2]];
        let candidates: Vec<&Element> =
            visible.iter().copied().filter(|e| !e.base.locked).collect();
        let map = ElementsMap::new(candidates.iter().copied());
        let live: Vec<&Element> = visible
            .iter()
            .copied()
            .filter(|e| !e.base.is_deleted)
            .collect();
        for element in &candidates {
            let id = element.base.id.clone();
            let taken = self.elements_to_erase.contains(&id);
            if restore && taken {
                if !eraser_test(segment, element, &map, zoom) {
                    continue;
                }
                if let Some(group) = element.base.group_ids.last() {
                    if self.groups_to_erase.contains(group) {
                        for member in get_elements_in_group(&live, group) {
                            self.elements_to_erase.shift_remove(&member.base.id);
                        }
                        self.groups_to_erase.remove(group);
                    }
                }
                if let Some(container) = container_id(element) {
                    self.elements_to_erase.shift_remove(container);
                }
                if let Some(label) = get_bound_text_element_id(element) {
                    self.elements_to_erase.shift_remove(label);
                }
                self.elements_to_erase.shift_remove(&id);
            } else if !restore && !taken {
                if !eraser_test(segment, element, &map, zoom) {
                    continue;
                }
                if let Some(group) = element.base.group_ids.last() {
                    if !self.groups_to_erase.contains(group) {
                        for member in get_elements_in_group(&live, group) {
                            self.elements_to_erase.insert(member.base.id.clone());
                        }
                        self.groups_to_erase.insert(group.clone());
                    }
                }
                if let Some(label) = get_bound_text_element_id(element) {
                    self.elements_to_erase.insert(label.to_owned());
                }
                if let Some(container) = container_id(element) {
                    self.elements_to_erase.insert(container.to_owned());
                }
                self.elements_to_erase.insert(id);
            }
        }
        self.elements_to_erase()
    }
}

/// `eraserTest(pathSegment, element, elementsMap, zoom)`
/// (`eraser/index.ts:210-304`); the segment runs from the newest point
/// back to the one before.
fn eraser_test(
    segment: [[f64; 2]; 2],
    element: &Element,
    elements_map: &ElementsMap<'_>,
    zoom: f64,
) -> bool {
    let last_point = segment[1];
    let freedraw = matches!(element.kind, ElementKind::Freedraw(_));
    let threshold = if freedraw {
        15.0
    } else {
        element.base.stroke_width / 2.0
    };
    let segment_bounds = [
        segment[0][0].min(segment[1][0]) - threshold,
        segment[0][1].min(segment[1][1]) - threshold,
        segment[0][0].max(segment[1][0]) + threshold,
        segment[0][1].max(segment[1][1]) + threshold,
    ];
    let [x1, y1, x2, y2] = get_element_bounds(element, elements_map);
    let element_bounds = [
        x1 - threshold,
        y1 - threshold,
        x2 + threshold,
        y2 + threshold,
    ];
    if !do_bounds_intersect(segment_bounds, element_bounds) {
        return false;
    }
    if should_test_inside(element) && is_point_in_element(last_point, element, elements_map) {
        return true;
    }
    let path = line_segment(gp(segment[0]), gp(segment[1]));

    if freedraw {
        let Ok(outline) = get_freedraw_outline_points(element) else {
            return false;
        };
        if outline.len() < 2 {
            return false;
        }
        let tolerance = (5.0 / zoom).max(2.25);
        for s in freedraw_outline_segments(element, &outline, elements_map) {
            if line_segments_distance(s, path) <= tolerance {
                return true;
            }
        }
        let polygon: Vec<GlobalPoint> = outline
            .iter()
            .map(|[x, y]| gp([element.base.x + x, element.base.y + y]))
            .collect();
        return polygon_includes_point_non_zero(gp(segment[0]), &polygon);
    }

    let open_line = match &element.kind {
        ElementKind::Arrow(_) => true,
        ElementKind::Line(line) => !line.polygon,
        _ => false,
    };
    if open_line {
        let tolerance = (element.base.stroke_width * 2.0 / zoom).max(element.base.stroke_width);
        let Ok(segments) = get_element_line_segments(element, elements_map) else {
            return false;
        };
        return segments
            .into_iter()
            .any(|s| line_segments_distance(s, path) <= tolerance);
    }

    if !intersect_element_with_line_segment(element, elements_map, segment, 0.0, true).is_empty() {
        return true;
    }
    // the label, where computeBoundTextPosition puts it
    let Some(label) = get_bound_text_element(element, elements_map) else {
        return false;
    };
    let everything: Vec<Element> = elements_map.values().map(|e| (*e).clone()).collect();
    let Some([x, y]) =
        compute_bound_text_position(element, label, &everything, &mut SceneArrowGeometry)
    else {
        return false;
    };
    let mut placed = label.clone();
    placed.base.x = x;
    placed.base.y = y;
    !intersect_element_with_line_segment(&placed, elements_map, segment, 0.0, true).is_empty()
}

/// `getFreedrawOutlineAsSegments(element, points, elementsMap)`
/// (`renderElement.ts:1275-1330`): the outline's points in scene
/// coordinates, turned about the unrotated bounds' centre, chained into
/// segments.
fn freedraw_outline_segments(
    element: &Element,
    points: &[[f64; 2]],
    elements_map: &ElementsMap<'_>,
) -> Vec<excali_math::LineSegment<excali_math::Global>> {
    let mut unrotated = element.clone();
    unrotated.base.angle.0 = 0.0;
    let [bx1, by1, bx2, by2] = get_element_bounds(&unrotated, elements_map);
    let center = gp([(bx1 + bx2) / 2.0, (by1 + by2) / 2.0]);
    let angle = Radians(element.base.angle.0);
    let at = |[x, y]: [f64; 2]| {
        point_rotate_rads(gp([x + element.base.x, y + element.base.y]), center, angle)
    };
    let mut segments = vec![line_segment(at(points[0]), at(points[1]))];
    for p in &points[2..] {
        let last = segments[segments.len() - 1].1;
        segments.push(line_segment(last, at(*p)));
    }
    segments
}
