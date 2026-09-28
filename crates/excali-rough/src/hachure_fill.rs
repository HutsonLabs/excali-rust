//! `hachure-fill` 0.5.2 (the copy rough.js 4.6.4 depends on): scan-line
//! hachure lines for a list of polygons.
//!
//! The polygons are rotated by the hachure angle, scanned with horizontal
//! lines, and rotated back, all in place: the rotation there and back is not
//! exact in floating point, so the polygon points come out moved in the last
//! bits, and rough.js goes on to use them (a cross-hatch's second pass, a
//! simplified path's stroke). The port does the same to its argument.
//!
//! rough.js can hand the package the same point object twice (points-on-curve
//! simplifies a one-point subpath to `[p, p]`), and then the rotation moves
//! that point twice. [`PolygonList`] keeps points in an arena so a polygon can
//! name one point twice; [`hachure_lines`] takes plain polygons.

use excali_math::js;

use crate::Point;

/// A hachure line, `[start, end]`.
pub type Line = [Point; 2];

/// Polygons as indices into shared points, so one point can appear in a
/// polygon more than once and be moved once per appearance, as a JavaScript
/// array holding the same point object twice is.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PolygonList {
    pub points: Vec<Point>,
    pub polygons: Vec<Vec<usize>>,
}

impl PolygonList {
    /// Polygons whose points are all distinct objects.
    pub fn new(polygons: &[Vec<Point>]) -> Self {
        let mut list = Self::default();
        for polygon in polygons {
            list.push(polygon);
        }
        list
    }

    /// Appends a polygon of fresh points.
    pub fn push(&mut self, polygon: &[Point]) {
        let start = self.points.len();
        self.points.extend_from_slice(polygon);
        self.polygons.push((start..self.points.len()).collect());
    }

    /// Appends a polygon that holds one point twice (`[p, p]`).
    pub fn push_doubled(&mut self, p: Point) {
        self.points.push(p);
        let i = self.points.len() - 1;
        self.polygons.push(vec![i, i]);
    }

    pub fn len(&self) -> usize {
        self.polygons.len()
    }

    /// Polygon `i` as points.
    pub fn polygon(&self, i: usize) -> Vec<Point> {
        self.polygons[i].iter().map(|&k| self.points[k]).collect()
    }

    /// Every polygon as points.
    pub fn to_vecs(&self) -> Vec<Vec<Point>> {
        (0..self.len()).map(|i| self.polygon(i)).collect()
    }
}

/// `hachureLines(polygons, hachureGap, hachureAngle, hachureStepOffset)`:
/// the hachure lines for `polygons` at `hachure_gap` (at least 0.1) and
/// `hachure_angle` degrees. Rotates `polygons` in place and back, as the
/// package does.
///
/// Divergences where the package does not return: an empty polygon (it reads
/// `vertices[0][0]` and throws a `TypeError`) is skipped, and a polygon with
/// a non-finite `y` on a sloped edge, which keeps the package scanning
/// forever, gives no lines (see [`straight_hachure_lines`]).
pub fn hachure_lines(
    polygons: &mut [Vec<Point>],
    hachure_gap: f64,
    hachure_angle: f64,
    hachure_step_offset: f64,
) -> Vec<Line> {
    let mut list = PolygonList::new(polygons);
    let lines = hachure_lines_in(&mut list, hachure_gap, hachure_angle, hachure_step_offset);
    for (i, polygon) in polygons.iter_mut().enumerate() {
        *polygon = list.polygon(i);
    }
    lines
}

/// [`hachure_lines`] on a [`PolygonList`].
pub(crate) fn hachure_lines_in(
    list: &mut PolygonList,
    hachure_gap: f64,
    hachure_angle: f64,
    hachure_step_offset: f64,
) -> Vec<Line> {
    let angle = hachure_angle;
    let gap = js::max(hachure_gap, 0.1);
    // `if (angle)`: 0 and NaN skip the rotation
    let rotate = angle != 0.0 && !angle.is_nan();
    if rotate {
        for p in 0..list.polygons.len() {
            rotate_polygon(list, p, angle);
        }
    }
    let mut lines = straight_hachure_lines(list, gap, hachure_step_offset);
    if rotate {
        for p in 0..list.polygons.len() {
            rotate_polygon(list, p, -angle);
        }
        let (cos, sin) = cos_sin(-angle);
        for line in &mut lines {
            for point in line.iter_mut() {
                rotate_point(point, cos, sin);
            }
        }
    }
    lines
}

/// `(Math.PI / 180) * degrees`, then its cosine and sine.
fn cos_sin(degrees: f64) -> (f64, f64) {
    let angle = (std::f64::consts::PI / 180.0) * degrees;
    (angle.cos(), angle.sin())
}

/// `rotatePoints(polygon, [0, 0], degrees)`: every entry, so a point that
/// appears twice turns twice.
fn rotate_polygon(list: &mut PolygonList, polygon: usize, degrees: f64) {
    if list.polygons[polygon].is_empty() {
        return;
    }
    let (cos, sin) = cos_sin(degrees);
    for k in 0..list.polygons[polygon].len() {
        let i = list.polygons[polygon][k];
        rotate_point(&mut list.points[i], cos, sin);
    }
}

/// One point of `rotatePoints` about the centre `[0, 0]`, written as the
/// package writes it (`- cx`, `+ cx` included: they decide the sign of 0).
fn rotate_point(p: &mut Point, cos: f64, sin: f64) {
    let (cx, cy) = (0.0, 0.0);
    let [x, y] = *p;
    p[0] = ((x - cx) * cos) - ((y - cy) * sin) + cx;
    p[1] = ((x - cx) * sin) + ((y - cy) * cos) + cy;
}

/// An entry of the sorted edge table.
#[derive(Clone, Copy, Debug)]
struct Edge {
    ymin: f64,
    ymax: f64,
    x: f64,
    islope: f64,
}

/// `straightHachureLines(polygons, gap, hachureStepOffset)`: the scan.
///
/// The package scans until every edge has been retired (`ymax <= y`). An
/// edge whose `ymin` or `ymax` is NaN or infinite is never retired (or the
/// scan line never moves), so the package never returns; the port returns no
/// lines for such input. A scan line that stops moving (`y + step == y`,
/// beyond 2^53 for a step of 1) ends the scan for the same reason.
fn straight_hachure_lines(list: &PolygonList, gap: f64, hachure_step_offset: f64) -> Vec<Line> {
    let mut vertex_array: Vec<Vec<Point>> = Vec::new();
    for polygon in &list.polygons {
        if polygon.is_empty() {
            continue;
        }
        let mut vertices: Vec<Point> = polygon.iter().map(|&i| list.points[i]).collect();
        let first = vertices[0];
        let last = vertices[vertices.len() - 1];
        if !(first[0] == last[0] && first[1] == last[1]) {
            vertices.push([first[0], first[1]]);
        }
        if vertices.len() > 2 {
            vertex_array.push(vertices);
        }
    }

    let mut lines = Vec::new();
    let gap = js::max(gap, 0.1);
    let mut edges: Vec<Edge> = Vec::new();
    for vertices in &vertex_array {
        for pair in vertices.windows(2) {
            let (p1, p2) = (pair[0], pair[1]);
            if p1[1] != p2[1] {
                let ymin = js::min(p1[1], p2[1]);
                edges.push(Edge {
                    ymin,
                    ymax: js::max(p1[1], p2[1]),
                    x: if ymin == p1[1] { p1[0] } else { p2[0] },
                    islope: (p2[0] - p1[0]) / (p2[1] - p1[1]),
                });
            }
        }
    }
    if edges
        .iter()
        .any(|e| !e.ymin.is_finite() || !e.ymax.is_finite())
    {
        return lines;
    }
    js::sort(&mut edges, |e1, e2| {
        if e1.ymin < e2.ymin {
            return -1.0;
        }
        if e1.ymin > e2.ymin {
            return 1.0;
        }
        if e1.x < e2.x {
            return -1.0;
        }
        if e1.x > e2.x {
            return 1.0;
        }
        if e1.ymax == e2.ymax {
            return 0.0;
        }
        (e1.ymax - e2.ymax) / (e1.ymax - e2.ymax).abs()
    });
    if edges.is_empty() {
        return lines;
    }

    let mut active: Vec<Edge> = Vec::new();
    let mut pending = edges.into_iter().peekable();
    let mut y = pending.peek().map_or(0.0, |e| e.ymin);
    let mut iteration: u64 = 0;
    while !active.is_empty() || pending.peek().is_some() {
        // edges[i].ymin <= y from the front, stopping at the first above
        while let Some(edge) = pending.next_if(|e| e.ymin <= y) {
            active.push(edge);
        }
        active.retain(|e| e.ymax > y);
        js::sort(&mut active, |a, b| {
            if a.x == b.x {
                return 0.0;
            }
            (a.x - b.x) / (a.x - b.x).abs()
        });
        // fill between the edges
        // (with fewer than two active edges the pairs loop draws nothing)
        if hachure_step_offset != 1.0 || (iteration as f64) % gap == 0.0 {
            for pair in active.chunks_exact(2) {
                let (ce, ne) = (pair[0], pair[1]);
                lines.push([[js::round(ce.x), y], [js::round(ne.x), y]]);
            }
        }
        let next = y + hachure_step_offset;
        if next == y || next.is_nan() {
            break;
        }
        y = next;
        for e in &mut active {
            e.x += hachure_step_offset * e.islope;
        }
        iteration += 1;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_named_twice_turns_twice() {
        // points-on-curve's simplify([p]) is [p, p], one object twice:
        // rotatePoints turns it by the angle twice, and back twice.
        let mut list = PolygonList::default();
        list.push_doubled([5.0, 7.0]);
        list.push(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        hachure_lines_in(&mut list, 2.0, 30.0, 1.0);
        // Node, hachure-fill 0.5.2: const p = [5, 7];
        // hachureLines([[p, p], [[0,0],[10,0],[10,10]]], 2, 30, 1) leaves
        // [[[5.000000000000001,7.000000000000001], (the same p)],
        //  [[0,0],[10,0],[10,9.999999999999998]]]
        let p = [5.000000000000001, 7.000000000000001];
        assert_eq!(list.polygon(0), vec![p, p]);
        assert_eq!(
            list.polygon(1),
            vec![[0.0, 0.0], [10.0, 0.0], [10.0, 9.999999999999998]]
        );
        assert_eq!(list.points.len(), 4);
    }

    #[test]
    fn no_rotation_at_zero_or_nan() {
        let mut list = PolygonList::new(&[vec![[0.3, 0.1], [10.7, 0.0], [10.0, 10.9]]]);
        let before = list.clone();
        hachure_lines_in(&mut list, 2.0, 0.0, 1.0);
        hachure_lines_in(&mut list, 2.0, f64::NAN, 1.0);
        assert_eq!(list, before);
    }
}
