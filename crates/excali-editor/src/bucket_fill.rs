//! The bucket fill tool's geometry (`packages/element/src/bucketFill.ts`
//! at the pinned commit): the closed polygon, in scene coordinates, that
//! fills the enclosed region under a click.
//!
//! All fills go through one path: a planar arrangement of the boundary
//! segments near the click (split at intersections and T-junctions, with
//! short bridge edges closing visual gaps) is built, its faces are walked,
//! and the smallest bounded face containing the click is selected.
//! Islands inside that face (other components' outside contours) become
//! holes, spliced into the ring as zero-width keyhole bridges.
//!
//! The owner is the topmost closed, visible element containing the click:
//! only elements near its bounds feed the arrangement. Without one (regions
//! formed by open lines), candidates come from a search box around the
//! click that doubles up to three times.
//!
//! Upstream iterates `Map`s and `Set`s in insertion order, and that order
//! decides ties (which face, which bridge, the order of the boundary ids);
//! the port keeps it with [`IndexMap`] and [`IndexSet`] wherever it is
//! observable, and sorts with [`js::sort`].

use std::collections::{HashMap, HashSet};

use indexmap::{IndexMap, IndexSet};

use excali_core::color::{
    apply_dark_mode_filter, is_opaque_color, is_transparent, COLOR_PALETTE,
    DEFAULT_ELEMENT_BACKGROUND_COLOR_INDEX,
};
use excali_core::element::{Element, ElementKind, FillStyle};
use excali_math::{
    distance_to_line_segment, js, line_segment, line_segment_intersection_points_with,
    point_distance, point_rotate_rads, points_equal, polygon_area_with, polygon_includes_point,
    polygon_includes_point_non_zero, polygon_signed_area_with, GlobalPoint, Point, Radians,
};
use excali_rough::points_on_curve::{curve_to_bezier, points_on_bezier_curves, simplify};
use excali_scene::bounds::{
    do_bounds_intersect, element_center_point, get_bounds_from_points, get_element_bounds,
    get_element_line_segments, Bounds, ElementsMap,
};
use excali_scene::freedraw::get_freedraw_stroke_center_points;
use excali_scene::utils::is_path_a_loop;
use serde_json::{json, Map, Value};

use crate::actions::has_background;
use crate::collision::{intersect_element_with_line_segment, is_point_in_element};
use crate::distance::distance_to_element;
use crate::mutate::get_size_from_points;
use crate::new_element::new_element_base;

/// `BUCKET_FILL_GAP_TOLERANCE`: how big a visual gap between strokes still
/// counts as closed, in scene px (a bridging radius, not a snapping one).
const BUCKET_FILL_GAP_TOLERANCE: f64 = 6.0;

/// `BUCKET_FILL_REGION_MATCH_TOLERANCE`: how far apart two fill outlines'
/// bounds may lie and still count as the same region.
const BUCKET_FILL_REGION_MATCH_TOLERANCE: f64 = 2.0;

/// `BUCKET_FILL_COVER_MARGIN`: how deep inside a coverer a stroke must lie
/// before it counts as hidden.
const BUCKET_FILL_COVER_MARGIN: f64 = 2.0;

/// `BUCKET_FILL_CURVE_MAX_DEVIATION`: the max deviation of a curved line's
/// sampled boundary from its rendered curve.
const BUCKET_FILL_CURVE_MAX_DEVIATION: f64 = 0.5;

/// `BucketFillOptions`; [`Default`] is `DEFAULT_BUCKET_FILL_OPTIONS`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BucketFillOptions {
    /// Vertices closer than this collapse to one graph node, and a node
    /// this close to a stroke splits it (T-junction).
    pub snap_epsilon: f64,
    /// Loose stroke ends within this distance of another stroke are
    /// bridged; also the broad-phase padding around the owner bounds.
    pub gap_tolerance: f64,
    /// Faces and polygons smaller than this area are discarded.
    pub min_area: f64,
    /// Above this many input segments the fill is `too_complex`.
    pub max_boundary_segments: usize,
    /// The cap on the generated polygon's points.
    pub max_generated_points: usize,
    /// The initial half-extent of the owner-less search box.
    pub fallback_search_radius: f64,
}

impl Default for BucketFillOptions {
    fn default() -> Self {
        BucketFillOptions {
            snap_epsilon: 0.5,
            gap_tolerance: BUCKET_FILL_GAP_TOLERANCE,
            min_area: 4.0,
            max_boundary_segments: 2560,
            max_generated_points: 1536,
            fallback_search_radius: 512.0,
        }
    }
}

/// `BucketFillFailureReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BucketFillFailureReason {
    NoOwner,
    OpenRegion,
    TooComplex,
    TooSmall,
    InvalidPolygon,
}

impl BucketFillFailureReason {
    /// The reason as upstream spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            BucketFillFailureReason::NoOwner => "no_owner",
            BucketFillFailureReason::OpenRegion => "open_region",
            BucketFillFailureReason::TooComplex => "too_complex",
            BucketFillFailureReason::TooSmall => "too_small",
            BucketFillFailureReason::InvalidPolygon => "invalid_polygon",
        }
    }
}

/// `BucketFillInsertion["placement"]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Above,
    Below,
}

/// `BucketFillInsertion`: where the fill goes in the scene order, relative
/// to an existing element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BucketFillInsertion {
    pub placement: Placement,
    pub element_id: String,
}

/// The `ok: true` arm of `BucketFillGeometryResult`.
#[derive(Debug, Clone, PartialEq)]
pub struct BucketFillPolygon {
    /// The closed element under the click, `None` for an owner-less fill.
    pub owner_id: Option<String>,
    /// The elements other than the owner whose outlines bound the fill.
    pub boundary_element_ids: Vec<String>,
    /// The closed ring in scene coordinates (first point repeated last),
    /// a keyhole path when the region has islands.
    pub scene_points: Vec<[f64; 2]>,
    pub insertion: BucketFillInsertion,
}

// -- small geometry helpers ----------------------------------------------------

fn gp(p: [f64; 2]) -> GlobalPoint {
    Point::new(p[0], p[1])
}

fn gps(points: &[[f64; 2]]) -> Vec<GlobalPoint> {
    points.iter().copied().map(gp).collect()
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    point_distance(gp(a), gp(b))
}

fn distance_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    distance_to_line_segment(gp(p), line_segment(gp(a), gp(b)))
}

fn rotate(p: [f64; 2], center: [f64; 2], angle: f64) -> [f64; 2] {
    let r = point_rotate_rads(gp(p), gp(center), Radians(angle));
    [r.x, r.y]
}

fn expand_bounds([x1, y1, x2, y2]: Bounds, pad: f64) -> Bounds {
    [x1 - pad, y1 - pad, x2 + pad, y2 + pad]
}

/// `projectParam(a, b, q)`: the parameter of `q` projected onto the line
/// through a-b.
fn project_param(a: [f64; 2], b: [f64; 2], q: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return 0.0;
    }
    ((q[0] - a[0]) * dx + (q[1] - a[1]) * dy) / len2
}

fn cell(v: f64, size: f64) -> i64 {
    (v / size).floor() as i64
}

/// `forEachCellAlongSegment(a, b, inflate, cellSize, visit)`: every grid
/// cell the `inflate`-expanded segment passes, column by column.
fn for_each_cell_along_segment(
    a: [f64; 2],
    b: [f64; 2],
    inflate: f64,
    cell_size: f64,
    mut visit: impl FnMut(i64, i64),
) {
    let min_x = js::min(a[0], b[0]);
    let max_x = js::max(a[0], b[0]);
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let from_cx = cell(min_x - inflate, cell_size);
    let to_cx = cell(max_x + inflate, cell_size);
    for cx in from_cx..=to_cx {
        let mut y0 = a[1];
        let mut y1 = b[1];
        if dx != 0.0 {
            let t0 = (js::max(min_x, cx as f64 * cell_size - inflate) - a[0]) / dx;
            let t1 = (js::min(max_x, (cx + 1) as f64 * cell_size + inflate) - a[0]) / dx;
            y0 = a[1] + t0 * dy;
            y1 = a[1] + t1 * dy;
        }
        let from_cy = cell(js::min(y0, y1) - inflate, cell_size);
        let to_cy = cell(js::max(y0, y1) + inflate, cell_size);
        for cy in from_cy..=to_cy {
            visit(cx, cy);
        }
    }
}

fn perpendicular_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = js::hypot(dx, dy);
    if len == 0.0 {
        return distance(p, a);
    }
    ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len
}

fn point_at_param(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// `subtractIntervals(base, cuts)`.
fn subtract_intervals(base: Vec<[f64; 2]>, cuts: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut result = base;
    for &[c0, c1] in cuts {
        let mut next = Vec::new();
        for &[b0, b1] in &result {
            if c1 <= b0 || c0 >= b1 {
                next.push([b0, b1]);
                continue;
            }
            if c0 > b0 {
                next.push([b0, c0]);
            }
            if c1 < b1 {
                next.push([c1, b1]);
            }
        }
        result = next;
    }
    result
}

fn chain(points: &[[f64; 2]]) -> Vec<[[f64; 2]; 2]> {
    points.windows(2).map(|w| [w[0], w[1]]).collect()
}

/// `lineElementIdealSegments(element, elementsMap)`: a line's boundary
/// from its logical path, or, for a curved line of more than two points,
/// the curve roughjs fits through them.
fn line_element_ideal_segments(
    element: &Element,
    points: &[[f64; 2]],
    elements_map: &ElementsMap<'_>,
) -> Vec<[[f64; 2]; 2]> {
    let center = element_center_point(element, elements_map);
    let local: Vec<[f64; 2]> = if element.base.roundness.is_some() && points.len() > 2 {
        curve_to_bezier(points, 0.0)
            .ok()
            .and_then(|bezier| {
                points_on_bezier_curves(&bezier, 0.15, Some(BUCKET_FILL_CURVE_MAX_DEVIATION)).ok()
            })
            .unwrap_or_default()
    } else {
        points.to_vec()
    };
    let scene: Vec<[f64; 2]> = local
        .iter()
        .map(|p| {
            rotate(
                [element.base.x + p[0], element.base.y + p[1]],
                center,
                element.base.angle.0,
            )
        })
        .collect();
    chain(&scene)
}

/// `freedrawIdealSegments(element, elementsMap)`: a freedraw's boundary
/// along its rendered (streamline-smoothed) centerline.
fn freedraw_ideal_segments(
    element: &Element,
    elements_map: &ElementsMap<'_>,
) -> Vec<[[f64; 2]; 2]> {
    let center = element_center_point(element, elements_map);
    let scene: Vec<[f64; 2]> = get_freedraw_stroke_center_points(element)
        .unwrap_or_default()
        .iter()
        .map(|p| {
            rotate(
                [element.base.x + p[0], element.base.y + p[1]],
                center,
                element.base.angle.0,
            )
        })
        .collect();
    chain(&scene)
}

fn element_line_segments(element: &Element, elements_map: &ElementsMap<'_>) -> Vec<[[f64; 2]; 2]> {
    get_element_line_segments(element, elements_map)
        .map(|segments| {
            segments
                .into_iter()
                .map(|s| [[s.0.x, s.0.y], [s.1.x, s.1.y]])
                .collect()
        })
        .unwrap_or_default()
}

/// `clipSegmentToVisible(a, b, coverers, elementsMap, eps, margin)`: the
/// parts of a-b not hidden (by more than `margin`) inside an opaque
/// coverer.
fn clip_segment_to_visible(
    a: [f64; 2],
    b: [f64; 2],
    coverers: &[&Element],
    elements_map: &ElementsMap<'_>,
    eps: f64,
    margin: f64,
) -> Vec<[[f64; 2]; 2]> {
    let mut intervals = vec![[0.0, 1.0]];
    for coverer in coverers {
        if intervals.is_empty() {
            break;
        }
        let hits = intersect_element_with_line_segment(coverer, elements_map, [a, b], 0.0, false);
        let mut ts: Vec<f64> = hits
            .iter()
            .map(|&p| project_param(a, b, p))
            .filter(|&t| t > 0.0 && t < 1.0)
            .collect();
        js::sort(&mut ts, |x, y| x - y);
        let mut breaks = vec![0.0];
        breaks.extend(ts);
        breaks.push(1.0);
        let mut covered = Vec::new();
        for k in 0..breaks.len() - 1 {
            let (t0, t1) = (breaks[k], breaks[k + 1]);
            if t1 - t0 < 1e-9 {
                continue;
            }
            let mid = point_at_param(a, b, (t0 + t1) / 2.0);
            if is_point_in_element(mid, coverer, elements_map)
                && distance_to_element(coverer, elements_map, mid) > margin
            {
                covered.push([t0, t1]);
            }
        }
        if !covered.is_empty() {
            intervals = subtract_intervals(intervals, &covered);
        }
    }
    intervals
        .into_iter()
        .filter(|&[t0, t1]| distance(point_at_param(a, b, t0), point_at_param(a, b, t1)) >= eps)
        .map(|[t0, t1]| [point_at_param(a, b, t0), point_at_param(a, b, t1)])
        .collect()
}

/// `NodeStore`: a spatial hash merging points within `eps` into one node.
struct NodeStore {
    nodes: Vec<[f64; 2]>,
    cells: HashMap<(i64, i64), Vec<usize>>,
    cell_size: f64,
    eps: f64,
}

impl NodeStore {
    fn new(eps: f64) -> NodeStore {
        NodeStore {
            nodes: Vec::new(),
            cells: HashMap::new(),
            cell_size: js::max(eps, 1.0),
            eps,
        }
    }

    fn get_or_create(&mut self, p: [f64; 2]) -> usize {
        let cx = cell(p[0], self.cell_size);
        let cy = cell(p[1], self.cell_size);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(bucket) = self.cells.get(&(cx + dx, cy + dy)) {
                    for &idx in bucket {
                        if distance(self.nodes[idx], p) <= self.eps {
                            return idx;
                        }
                    }
                }
            }
        }
        let idx = self.nodes.len();
        self.nodes.push(p);
        self.cells.entry((cx, cy)).or_default().push(idx);
        idx
    }

    /// Every node in the cells within `radius` of `p` (a superset).
    fn for_each_in_radius(&self, p: [f64; 2], radius: f64, mut visit: impl FnMut(usize)) {
        let from_cx = cell(p[0] - radius, self.cell_size);
        let to_cx = cell(p[0] + radius, self.cell_size);
        let from_cy = cell(p[1] - radius, self.cell_size);
        let to_cy = cell(p[1] + radius, self.cell_size);
        for cx in from_cx..=to_cx {
            for cy in from_cy..=to_cy {
                if let Some(bucket) = self.cells.get(&(cx, cy)) {
                    for &index in bucket {
                        visit(index);
                    }
                }
            }
        }
    }
}

// -- owner & boundary selection --------------------------------------------------

fn type_str(element: &Element) -> &'static str {
    element.kind.element_type().as_str()
}

fn line_points(element: &Element) -> Option<(&[[f64; 2]], bool)> {
    match &element.kind {
        ElementKind::Line(line) => Some((&line.linear.points, line.polygon)),
        _ => None,
    }
}

fn freedraw_points(element: &Element) -> Option<&[[f64; 2]]> {
    match &element.kind {
        ElementKind::Freedraw(freedraw) => Some(&freedraw.points),
        _ => None,
    }
}

/// `isValidPolygon(points)` (`typeChecks.ts:397-401`).
fn is_valid_polygon(points: &[[f64; 2]]) -> bool {
    points.len() > 3 && points_equal(gp(points[0]), gp(points[points.len() - 1]))
}

/// `rendersOpaqueFill(element)`: the element paints an opaque background
/// that hides outlines beneath it.
pub fn renders_opaque_fill(element: &Element) -> bool {
    if !has_background(type_str(element))
        || element.base.fill_style != FillStyle::Solid
        || element.base.opacity < 100.0
        || !is_opaque_color(&element.base.background_color)
    {
        return false;
    }
    // open strokes never paint their background
    let points = line_points(element)
        .map(|(points, _)| points)
        .or_else(|| freedraw_points(element));
    if let Some(points) = points {
        if !is_path_a_loop(points, 1.0) {
            return false;
        }
    }
    true
}

/// `FILL_BOUNDARY_TYPES`.
fn is_fill_boundary_type(element: &Element) -> bool {
    matches!(
        element.kind,
        ElementKind::Rectangle
            | ElementKind::Diamond
            | ElementKind::Ellipse
            | ElementKind::Frame(_)
            | ElementKind::MagicFrame(_)
            | ElementKind::Line(_)
            | ElementKind::Freedraw(_)
    )
}

fn is_invisible(element: &Element) -> bool {
    element.base.opacity <= 0.0
}

/// `rendersAnyMark(element)`: a visible stroke, a painted background or
/// image content.
fn renders_any_mark(element: &Element) -> bool {
    !is_invisible(element)
        && (matches!(element.kind, ElementKind::Image(_))
            || !is_transparent(&element.base.stroke_color)
            || (has_background(type_str(element))
                && !is_transparent(&element.base.background_color)))
}

/// `isBucketFillCompatible(element)`: a closed, visible line polygon with a
/// background and no visible stroke, the paint the bucket tool makes.
pub fn is_bucket_fill_compatible(element: &Element) -> bool {
    match line_points(element) {
        Some((points, polygon)) => {
            polygon
                && is_valid_polygon(points)
                && !is_invisible(element)
                && !is_transparent(&element.base.background_color)
                && is_transparent(&element.base.stroke_color)
        }
        None => false,
    }
}

fn is_closed_owner_candidate(element: &Element) -> bool {
    if !is_fill_boundary_type(element) {
        return false;
    }
    if let Some((points, polygon)) = line_points(element) {
        return polygon && is_valid_polygon(points);
    }
    if let Some(points) = freedraw_points(element) {
        return is_path_a_loop(points, 1.0);
    }
    true
}

fn is_eligible_boundary(element: &Element) -> bool {
    !is_invisible(element)
        && is_fill_boundary_type(element)
        && !is_transparent(&element.base.stroke_color)
}

fn find_owner<'a>(
    point: [f64; 2],
    elements: &[&'a Element],
    elements_map: &ElementsMap<'_>,
) -> Option<&'a Element> {
    elements.iter().rev().copied().find(|element| {
        renders_any_mark(element)
            && !is_bucket_fill_compatible(element)
            && is_closed_owner_candidate(element)
            && is_point_in_element(point, element, elements_map)
    })
}

// -- planar arrangement + face extraction ------------------------------------------

struct WorkingSegment {
    a: usize,
    b: usize,
    pa: [f64; 2],
    pb: [f64; 2],
    bounds: Bounds,
    element_id: String,
    splits: Vec<(usize, f64)>,
}

/// A segment and the element it came from.
struct SourceSegment {
    segment: [[f64; 2]; 2],
    element_id: String,
}

/// A face ring, the elements whose outlines bound it, and its connected
/// component.
struct Face {
    ring: Vec<[f64; 2]>,
    contributors: IndexSet<String>,
    component_id: Option<usize>,
}

fn edge_key(u: usize, v: usize) -> (usize, usize) {
    if u < v {
        (u, v)
    } else {
        (v, u)
    }
}

/// The graph `buildFaces` grows: its edges (in insertion order), each
/// node's neighbours and each edge's source elements.
struct Graph {
    edge_set: IndexSet<(usize, usize)>,
    adjacency: IndexMap<usize, Vec<usize>>,
    edge_to_elements: HashMap<(usize, usize), IndexSet<String>>,
}

impl Graph {
    fn link(&mut self, u: usize, v: usize) {
        self.adjacency.entry(u).or_default().push(v);
    }

    fn add_edge(&mut self, nodes: &[[f64; 2]], eps: f64, u: usize, v: usize, element_id: &str) {
        if u == v {
            return;
        }
        if distance(nodes[u], nodes[v]) < eps {
            return;
        }
        let key = edge_key(u, v);
        self.edge_to_elements
            .entry(key)
            .or_default()
            .insert(element_id.to_string());
        if !self.edge_set.insert(key) {
            return;
        }
        self.link(u, v);
        self.link(v, u);
    }

    fn unlink(&mut self, u: usize, v: usize) -> Option<IndexSet<String>> {
        let key = edge_key(u, v);
        self.edge_set.shift_remove(&key);
        let owners = self.edge_to_elements.remove(&key);
        // `list.splice(list.indexOf(x), 1)`: a missing entry removes the
        // last one, as indexOf's -1 does
        let mut splice = |node: usize, other: usize| {
            if let Some(list) = self.adjacency.get_mut(&node) {
                match list.iter().position(|&n| n == other) {
                    Some(i) => {
                        list.remove(i);
                    }
                    None => {
                        list.pop();
                    }
                }
            }
        };
        splice(u, v);
        splice(v, u);
        owners
    }

    fn is_dangling_end(&self, node: usize) -> bool {
        let neighbours = self.adjacency.get(&node).map(Vec::as_slice).unwrap_or(&[]);
        neighbours.iter().collect::<HashSet<_>>().len() == 1
    }
}

/// `buildFaces(rawSegments, options)`: the planar graph of the segments
/// (split at intersections and T-junctions, gaps bridged) and its face
/// rings. `Some(vec![])` without usable edges, `None` when too complex.
fn build_faces(raw_segments: &[SourceSegment], options: &BucketFillOptions) -> Option<Vec<Face>> {
    let eps = options.snap_epsilon;
    let mut store = NodeStore::new(eps);
    let mut segments: Vec<WorkingSegment> = Vec::new();
    let mut node_element: HashMap<usize, String> = HashMap::new();

    for SourceSegment {
        segment,
        element_id,
    } in raw_segments
    {
        let a = store.get_or_create(segment[0]);
        let b = store.get_or_create(segment[1]);
        node_element.entry(a).or_insert_with(|| element_id.clone());
        node_element.entry(b).or_insert_with(|| element_id.clone());
        if a == b {
            continue;
        }
        let pa = store.nodes[a];
        let pb = store.nodes[b];
        segments.push(WorkingSegment {
            a,
            b,
            pa,
            pb,
            element_id: element_id.clone(),
            bounds: [
                js::min(pa[0], pb[0]),
                js::min(pa[1], pb[1]),
                js::max(pa[0], pb[0]),
                js::max(pa[1], pb[1]),
            ],
            splits: vec![(a, 0.0), (b, 1.0)],
        });
    }

    // transversal intersections, swept by bbox minX
    let mut by_min_x: Vec<usize> = (0..segments.len()).collect();
    js::sort(&mut by_min_x, |&a, &b| {
        segments[a].bounds[0] - segments[b].bounds[0]
    });
    for oi in 0..by_min_x.len() {
        let i = by_min_x[oi];
        let sweep_max_x = segments[i].bounds[2] + eps;
        for &j in &by_min_x[oi + 1..] {
            if segments[j].bounds[0] > sweep_max_x {
                break;
            }
            if !do_bounds_intersect(expand_bounds(segments[i].bounds, eps), segments[j].bounds) {
                continue;
            }
            let (si, sj) = (&segments[i], &segments[j]);
            let Some(hit) = line_segment_intersection_points_with(
                line_segment(gp(si.pa), gp(si.pb)),
                line_segment(gp(sj.pa), gp(sj.pb)),
                eps,
            ) else {
                continue;
            };
            let node = store.get_or_create([hit.x, hit.y]);
            let q = store.nodes[node];
            node_element
                .entry(node)
                .or_insert_with(|| si.element_id.clone());
            let ti = project_param(si.pa, si.pb, q);
            let tj = project_param(sj.pa, sj.pb, q);
            segments[i].splits.push((node, ti));
            segments[j].splits.push((node, tj));
        }
    }

    if store.nodes.len() > options.max_boundary_segments * 4 {
        return None;
    }

    // T-junctions, over a grid of the segments inflated by eps
    let mut scene_min_x = f64::INFINITY;
    let mut scene_min_y = f64::INFINITY;
    let mut scene_max_x = f64::NEG_INFINITY;
    let mut scene_max_y = f64::NEG_INFINITY;
    for s in &segments {
        scene_min_x = js::min(scene_min_x, s.bounds[0]);
        scene_min_y = js::min(scene_min_y, s.bounds[1]);
        scene_max_x = js::max(scene_max_x, s.bounds[2]);
        scene_max_y = js::max(scene_max_y, s.bounds[3]);
    }
    let scene_span = js::max(
        js::max(scene_max_x - scene_min_x, scene_max_y - scene_min_y),
        0.0,
    );
    let seg_cell_size = js::max(eps * 2.0, scene_span / 128.0);
    let mut seg_grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (index, s) in segments.iter().enumerate() {
        for_each_cell_along_segment(s.pa, s.pb, eps, seg_cell_size, |cx, cy| {
            seg_grid.entry((cx, cy)).or_default().push(index);
        });
    }
    for n in 0..store.nodes.len() {
        let q = store.nodes[n];
        let Some(bucket) = seg_grid.get(&(cell(q[0], seg_cell_size), cell(q[1], seg_cell_size)))
        else {
            continue;
        };
        for &index in bucket {
            let segment = &segments[index];
            if n == segment.a || n == segment.b {
                continue;
            }
            let t = project_param(segment.pa, segment.pb, q);
            if t <= 0.0 || t >= 1.0 {
                continue;
            }
            if distance_to_segment(q, segment.pa, segment.pb) <= eps {
                segments[index].splits.push((n, t));
            }
        }
    }

    // atomic edges
    let mut graph = Graph {
        edge_set: IndexSet::new(),
        adjacency: IndexMap::new(),
        edge_to_elements: HashMap::new(),
    };
    for segment in &segments {
        let mut by_node: IndexMap<usize, f64> = IndexMap::new();
        for &(node, t) in &segment.splits {
            by_node.entry(node).or_insert(t);
        }
        let mut ordered: Vec<(usize, f64)> = by_node.into_iter().collect();
        js::sort(&mut ordered, |a, b| a.1 - b.1);
        for k in 0..ordered.len().saturating_sub(1) {
            graph.add_edge(
                &store.nodes,
                eps,
                ordered[k].0,
                ordered[k + 1].0,
                &segment.element_id,
            );
        }
    }

    if graph.edge_set.is_empty() {
        return Some(Vec::new());
    }

    // bridging: each dangling stroke end gets a connector to the nearest
    // node or edge within the gap tolerance
    let bridge_radius = js::max(eps, options.gap_tolerance);
    let loose_ends: Vec<usize> = graph
        .adjacency
        .keys()
        .copied()
        .filter(|&node| graph.is_dangling_end(node))
        .collect();
    if !loose_ends.is_empty() {
        let edge_cell_size = js::max(js::max(bridge_radius, scene_span / 128.0), 1.0);
        let mut edge_grid: HashMap<(i64, i64), Vec<(usize, usize)>> = HashMap::new();
        let insert_live_edge =
            |grid: &mut HashMap<(i64, i64), Vec<(usize, usize)>>, nodes: &[[f64; 2]], u, v| {
                for_each_cell_along_segment(nodes[u], nodes[v], 0.0, edge_cell_size, |cx, cy| {
                    grid.entry((cx, cy)).or_default().push((u, v));
                });
            };
        for &(u, v) in &graph.edge_set {
            insert_live_edge(&mut edge_grid, &store.nodes, u, v);
        }

        for loose in loose_ends {
            if !graph.is_dangling_end(loose) {
                continue;
            }
            let p = store.nodes[loose];
            let neighbours = graph.adjacency.get(&loose).cloned().unwrap_or_default();

            let mut best_node: Option<usize> = None;
            let mut best_node_distance = f64::INFINITY;
            store.for_each_in_radius(p, bridge_radius, |n| {
                if n == loose || neighbours.contains(&n) {
                    return;
                }
                let d = distance(p, store.nodes[n]);
                if d >= eps && d <= bridge_radius && d < best_node_distance {
                    best_node_distance = d;
                    best_node = Some(n);
                }
            });

            let mut best_edge: Option<(usize, usize)> = None;
            let mut best_edge_distance = f64::INFINITY;
            let mut best_edge_t = 0.0;
            let loose_cx = cell(p[0], edge_cell_size);
            let loose_cy = cell(p[1], edge_cell_size);
            let mut seen_edges = HashSet::new();
            for dcx in -1..=1 {
                for dcy in -1..=1 {
                    let Some(bucket) = edge_grid.get(&(loose_cx + dcx, loose_cy + dcy)) else {
                        continue;
                    };
                    for &(u, v) in bucket {
                        let key = edge_key(u, v);
                        if !seen_edges.insert(key) {
                            continue;
                        }
                        if !graph.edge_set.contains(&key) || u == loose || v == loose {
                            continue;
                        }
                        let (eu, ev) = (store.nodes[u], store.nodes[v]);
                        let t = project_param(eu, ev, p);
                        if t <= 0.0 || t >= 1.0 {
                            continue;
                        }
                        let d = distance_to_segment(p, eu, ev);
                        if d <= bridge_radius && d < best_edge_distance {
                            best_edge_distance = d;
                            best_edge = Some((u, v));
                            best_edge_t = t;
                        }
                    }
                }
            }

            let bridge_element = node_element
                .get(&loose)
                .cloned()
                .unwrap_or_else(|| segments[0].element_id.clone());
            match best_edge {
                Some((u, v)) if best_edge_distance < best_node_distance => {
                    let (eu, ev) = (store.nodes[u], store.nodes[v]);
                    let projection = store.get_or_create(point_at_param(eu, ev, best_edge_t));
                    if projection == u || projection == v {
                        graph.add_edge(&store.nodes, eps, loose, projection, &bridge_element);
                        insert_live_edge(&mut edge_grid, &store.nodes, loose, projection);
                    } else {
                        node_element.entry(projection).or_insert_with(|| {
                            graph
                                .edge_to_elements
                                .get(&edge_key(u, v))
                                .and_then(|owners| owners.first().cloned())
                                .unwrap_or_else(|| bridge_element.clone())
                        });
                        let owners = graph
                            .unlink(u, v)
                            .unwrap_or_else(|| IndexSet::from([bridge_element.clone()]));
                        for owner in &owners {
                            graph.add_edge(&store.nodes, eps, u, projection, owner);
                            graph.add_edge(&store.nodes, eps, projection, v, owner);
                        }
                        insert_live_edge(&mut edge_grid, &store.nodes, u, projection);
                        insert_live_edge(&mut edge_grid, &store.nodes, projection, v);
                        graph.add_edge(&store.nodes, eps, loose, projection, &bridge_element);
                        insert_live_edge(&mut edge_grid, &store.nodes, loose, projection);
                    }
                }
                _ => {
                    if let Some(n) = best_node {
                        graph.add_edge(&store.nodes, eps, loose, n, &bridge_element);
                        insert_live_edge(&mut edge_grid, &store.nodes, loose, n);
                    }
                }
            }
        }
    }

    // connected components, bridges included
    let mut component_of: HashMap<usize, usize> = HashMap::new();
    let mut component_count = 0;
    for &start in graph.adjacency.keys() {
        if component_of.contains_key(&start) {
            continue;
        }
        let mut queue = vec![start];
        component_of.insert(start, component_count);
        while let Some(node) = queue.pop() {
            for &neighbour in graph.adjacency.get(&node).map(Vec::as_slice).unwrap_or(&[]) {
                if let std::collections::hash_map::Entry::Vacant(e) = component_of.entry(neighbour)
                {
                    e.insert(component_count);
                    queue.push(neighbour);
                }
            }
        }
        component_count += 1;
    }

    // outgoing half-edges sorted by angle around each node
    let nodes = &store.nodes;
    let angle_of = |from: usize, to: usize| -> f64 {
        js::atan2(nodes[to][1] - nodes[from][1], nodes[to][0] - nodes[from][0])
    };
    let mut sorted_out: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut position_of: HashMap<usize, HashMap<usize, usize>> = HashMap::new();
    for (&node, neighbours) in &graph.adjacency {
        let mut unique: Vec<usize> = neighbours
            .iter()
            .copied()
            .collect::<IndexSet<_>>()
            .into_iter()
            .collect();
        js::sort(&mut unique, |&p, &q| angle_of(node, p) - angle_of(node, q));
        position_of.insert(
            node,
            unique.iter().enumerate().map(|(i, &n)| (n, i)).collect(),
        );
        sorted_out.insert(node, unique);
    }

    // walk half-edges into face rings
    let mut visited: HashSet<(usize, usize)> = HashSet::new();
    let mut faces = Vec::new();
    let max_steps = graph.edge_set.len() * 2 + 4;
    for (&node, neighbours) in &graph.adjacency {
        for &first in neighbours {
            if visited.contains(&(node, first)) {
                continue;
            }
            let mut ring: Vec<usize> = Vec::new();
            let (mut from, mut to) = (node, first);
            let mut steps = 0;
            while steps <= max_steps {
                steps += 1;
                visited.insert((from, to));
                ring.push(from);
                let outs = &sorted_out[&to];
                let twin_position = position_of[&to][&from];
                // the next edge is the one immediately clockwise from the twin
                let next = outs[(twin_position + outs.len() - 1) % outs.len()];
                from = to;
                to = next;
                if from == node && to == first {
                    break;
                }
            }
            if ring.len() >= 3 {
                let mut contributors = IndexSet::new();
                for k in 0..ring.len() {
                    if let Some(owners) = graph
                        .edge_to_elements
                        .get(&edge_key(ring[k], ring[(k + 1) % ring.len()]))
                    {
                        contributors.extend(owners.iter().cloned());
                    }
                }
                faces.push(Face {
                    ring: ring.iter().map(|&i| nodes[i]).collect(),
                    contributors,
                    component_id: component_of.get(&node).copied(),
                });
            }
        }
    }

    Some(faces)
}

/// The selected face and the island faces to punch out of it (indices).
struct FaceSelection {
    face: usize,
    holes: Vec<usize>,
}

/// `selectFaceFromArrangement(faces, point, options)`: the smallest bounded
/// face (negative walk area) containing the click, and the outermost
/// islands inside it.
fn select_face_from_arrangement(
    faces: &[Face],
    point: [f64; 2],
    options: &BucketFillOptions,
) -> Option<FaceSelection> {
    if faces.is_empty() {
        return None;
    }
    let rings: Vec<Vec<GlobalPoint>> = faces.iter().map(|f| gps(&f.ring)).collect();
    let area_of: Vec<f64> = rings
        .iter()
        .map(|r| -polygon_signed_area_with(r, 0.0))
        .collect();
    let click = gp(point);

    let mut best: Option<usize> = None;
    let mut best_area = f64::INFINITY;
    for (i, &area) in area_of.iter().enumerate() {
        // Math.sign(area) === 1: the outer orientation
        if area > 0.0 {
            continue;
        }
        let abs_area = area.abs();
        if abs_area < options.min_area {
            continue;
        }
        if !polygon_includes_point_non_zero(click, &rings[i]) {
            continue;
        }
        if abs_area < best_area {
            best_area = abs_area;
            best = Some(i);
        }
    }
    let selected = best?;

    let island_candidates: Vec<usize> = (0..faces.len())
        .filter(|&i| {
            faces[i].component_id != faces[selected].component_id
                && area_of[i] > 0.0
                && area_of[i].abs() >= options.min_area
                && !polygon_includes_point_non_zero(click, &rings[i])
                && polygon_includes_point_non_zero(rings[i][0], &rings[selected])
        })
        .collect();
    let holes = island_candidates
        .iter()
        .copied()
        .filter(|&hole| {
            !island_candidates.iter().any(|&other| {
                other != hole && polygon_includes_point_non_zero(rings[hole][0], &rings[other])
            })
        })
        .collect();

    Some(FaceSelection {
        face: selected,
        holes,
    })
}

// -- simplification ---------------------------------------------------------------

fn dedupe_consecutive(pts: &[[f64; 2]], eps: f64) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::new();
    for &p in pts {
        if out.last().is_none_or(|&last| distance(last, p) >= eps) {
            out.push(p);
        }
    }
    while out.len() > 1 && distance(out[0], out[out.len() - 1]) < eps {
        out.pop();
    }
    out
}

fn remove_collinear(pts: Vec<[f64; 2]>, tolerance: f64) -> Vec<[f64; 2]> {
    if pts.len() <= 3 {
        return pts;
    }
    let mut out: Vec<[f64; 2]> = Vec::new();
    for i in 0..pts.len() {
        let prev = out.last().copied().unwrap_or(pts[pts.len() - 1]);
        let next = pts[(i + 1) % pts.len()];
        if perpendicular_distance(pts[i], prev, next) >= tolerance {
            out.push(pts[i]);
        }
    }
    if out.len() >= 3 {
        out
    } else {
        pts
    }
}

/// `simplifyRing(ring, options, budget)`: RDP escalating until under the
/// point budget, then collinear points removed; `None` below a triangle.
fn simplify_ring(
    ring: &[[f64; 2]],
    options: &BucketFillOptions,
    budget: usize,
) -> Option<Vec<[f64; 2]>> {
    let pts = dedupe_consecutive(ring, options.snap_epsilon);
    if pts.len() < 3 {
        return None;
    }
    // simplify only fails for a negative distance
    let rdp = |tolerance: f64| simplify(&pts, tolerance).unwrap_or_else(|_| pts.clone());
    let mut tolerance = 0.75;
    let mut simplified = rdp(tolerance);
    while simplified.len() > budget && tolerance < 1e6 {
        tolerance *= 2.0;
        simplified = rdp(tolerance);
    }
    if simplified.len() > budget {
        return None;
    }
    let simplified = remove_collinear(simplified, 0.05);
    (simplified.len() >= 3).then_some(simplified)
}

/// `spliceHoleIntoRing(ring, hole)`: the hole walked in, with opposite
/// winding, through a zero-width bridge from the nearest ring vertex.
fn splice_hole_into_ring(ring: &[[f64; 2]], hole: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let sign = |pts: &[[f64; 2]]| js_sign(polygon_signed_area_with(&gps(pts), 0.0));
    let oriented: Vec<[f64; 2]> = if sign(hole) == sign(ring) {
        hole.iter().rev().copied().collect()
    } else {
        hole.to_vec()
    };

    let mut best_ring_index = 0;
    let mut best_hole_index = 0;
    let mut best_distance = f64::INFINITY;
    for (i, &r) in ring.iter().enumerate() {
        for (j, &h) in oriented.iter().enumerate() {
            let d = distance(r, h);
            if d < best_distance {
                best_distance = d;
                best_ring_index = i;
                best_hole_index = j;
            }
        }
    }

    let mut out: Vec<[f64; 2]> = ring[..=best_ring_index].to_vec();
    for k in 0..=oriented.len() {
        out.push(oriented[(best_hole_index + k) % oriented.len()]);
    }
    out.push(ring[best_ring_index]);
    out.extend_from_slice(&ring[best_ring_index + 1..]);
    out
}

/// `Math.sign(x)`, whose results `===` compares (NaN never equal).
fn js_sign(x: f64) -> Option<i8> {
    if x > 0.0 {
        Some(1)
    } else if x < 0.0 {
        Some(-1)
    } else if x == 0.0 {
        Some(0)
    } else {
        None
    }
}

/// `finalizePolygon(outerRing, holeRings, options)`: the simplified outer
/// ring with the simplified holes spliced in (largest first, while the
/// point budget allows), closed; and which holes were spliced.
fn finalize_polygon(
    outer_ring: &[[f64; 2]],
    hole_rings: &[&[[f64; 2]]],
    options: &BucketFillOptions,
) -> Option<(Vec<[f64; 2]>, Vec<usize>)> {
    let mut ring = simplify_ring(outer_ring, options, options.max_generated_points)?;

    let mut by_area_desc: Vec<usize> = (0..hole_rings.len()).collect();
    let area = |i: usize| polygon_area_with(&gps(hole_rings[i]), 0.0);
    js::sort(&mut by_area_desc, |&a, &b| area(b) - area(a));
    let mut spliced = Vec::new();
    for index in by_area_desc {
        // a spliced hole costs its own points + 2 bridge duplicates
        let budget = options.max_generated_points as i64 - ring.len() as i64 - 2;
        if budget < 3 {
            break;
        }
        let Some(hole) = simplify_ring(hole_rings[index], options, budget as usize) else {
            continue;
        };
        ring = splice_hole_into_ring(&ring, &hole);
        spliced.push(index);
    }

    let first = ring[0];
    ring.push(first);
    Some((ring, spliced))
}

// -- public API ------------------------------------------------------------------

/// `computeBucketFillPolygon({ point, elements, elementsMap, options })`:
/// the fill of the enclosed region under `point`, or why there is none.
/// `elements` is the non-deleted scene in z-order.
pub fn compute_bucket_fill_polygon(
    point: [f64; 2],
    elements: &[&Element],
    elements_map: &ElementsMap<'_>,
    options: &BucketFillOptions,
) -> Result<BucketFillPolygon, BucketFillFailureReason> {
    let owner = find_owner(point, elements, elements_map);

    let mut index_of: HashMap<&str, usize> = HashMap::new();
    for (i, element) in elements.iter().enumerate() {
        index_of.insert(element.base.id.as_str(), i);
    }

    // boundary segments within `candidate_bounds`, clipped to their visible
    // parts; None over the segment cap
    let collect_segments =
        |candidate_bounds: Bounds, primary: Option<&Element>| -> Option<Vec<SourceSegment>> {
            let in_range = |element: &Element| {
                do_bounds_intersect(candidate_bounds, get_element_bounds(element, elements_map))
            };
            let boundaries: Vec<&Element> = elements
                .iter()
                .copied()
                .filter(|element| {
                    Some(element.base.id.as_str()) != primary.map(|p| p.base.id.as_str())
                        && is_eligible_boundary(element)
                        && in_range(element)
                })
                .collect();
            let coverers: Vec<&Element> = elements
                .iter()
                .copied()
                .filter(|element| renders_opaque_fill(element) && in_range(element))
                .collect();

            let mut raw_segments: Vec<SourceSegment> = Vec::new();
            let collect = |element: &Element, raw_segments: &mut Vec<SourceSegment>| {
                let element_index = index_of.get(element.base.id.as_str()).copied().unwrap_or(0);
                let coverers_above: Vec<&Element> = coverers
                    .iter()
                    .copied()
                    .filter(|coverer| {
                        coverer.base.id != element.base.id
                            && index_of.get(coverer.base.id.as_str()).copied().unwrap_or(0)
                                > element_index
                    })
                    .collect();
                let clip_margin = js::max(
                    js::max(options.snap_epsilon, element.base.stroke_width / 2.0),
                    BUCKET_FILL_COVER_MARGIN,
                );
                let mut segments = if let Some((points, _)) = line_points(element) {
                    line_element_ideal_segments(element, points, elements_map)
                } else if freedraw_points(element).is_some() {
                    freedraw_ideal_segments(element, elements_map)
                } else {
                    element_line_segments(element, elements_map)
                };
                // freedraw and non-polygon line loops close as they render
                let loop_points = match &element.kind {
                    ElementKind::Freedraw(freedraw) => Some(freedraw.points.as_slice()),
                    ElementKind::Line(line) if !line.polygon => Some(line.linear.points.as_slice()),
                    _ => None,
                };
                if let Some(points) = loop_points {
                    if is_path_a_loop(points, 1.0) && !segments.is_empty() {
                        let first = segments[0][0];
                        let last = segments[segments.len() - 1][1];
                        if distance(first, last) >= options.snap_epsilon {
                            segments.push([last, first]);
                        }
                    }
                }
                for segment in segments {
                    if raw_segments.len() > options.max_boundary_segments {
                        return;
                    }
                    if distance(segment[0], segment[1]) == 0.0 {
                        continue;
                    }
                    if coverers_above.is_empty() {
                        raw_segments.push(SourceSegment {
                            segment,
                            element_id: element.base.id.clone(),
                        });
                        continue;
                    }
                    for clipped in clip_segment_to_visible(
                        segment[0],
                        segment[1],
                        &coverers_above,
                        elements_map,
                        options.snap_epsilon,
                        clip_margin,
                    ) {
                        raw_segments.push(SourceSegment {
                            segment: clipped,
                            element_id: element.base.id.clone(),
                        });
                    }
                }
            };
            for element in primary.into_iter().chain(boundaries) {
                if raw_segments.len() > options.max_boundary_segments {
                    break;
                }
                collect(element, &mut raw_segments);
            }
            (raw_segments.len() <= options.max_boundary_segments).then_some(raw_segments)
        };

    enum Built {
        Selected(Vec<Face>, FaceSelection),
        Nothing,
        TooComplex,
    }
    let build_and_select = |raw_segments: &[SourceSegment]| -> Built {
        let Some(faces) = build_faces(raw_segments, options) else {
            return Built::TooComplex;
        };
        match select_face_from_arrangement(&faces, point, options) {
            Some(selection) => Built::Selected(faces, selection),
            None => Built::Nothing,
        }
    };

    let (faces, selection) = if let Some(owner) = owner {
        let pad = options.gap_tolerance + 2.0 + js::max(owner.base.stroke_width, 1.0);
        let search_bounds = expand_bounds(get_element_bounds(owner, elements_map), pad);
        let raw_segments = collect_segments(search_bounds, Some(owner))
            .ok_or(BucketFillFailureReason::TooComplex)?;
        match build_and_select(&raw_segments) {
            Built::TooComplex => return Err(BucketFillFailureReason::TooComplex),
            Built::Nothing => return Err(BucketFillFailureReason::OpenRegion),
            Built::Selected(faces, selection) => (faces, selection),
        }
    } else {
        // owner-less fallback: an expanding search box around the click
        let total_eligible = elements.iter().filter(|e| is_eligible_boundary(e)).count();
        if total_eligible == 0 {
            return Err(BucketFillFailureReason::NoOwner);
        }
        let mut found = None;
        let mut radius = options.fallback_search_radius;
        for attempt in 0..4 {
            let search_box: Bounds = [
                point[0] - radius,
                point[1] - radius,
                point[0] + radius,
                point[1] + radius,
            ];
            radius *= 2.0;
            let in_range = elements
                .iter()
                .filter(|element| {
                    is_eligible_boundary(element)
                        && do_bounds_intersect(
                            search_box,
                            get_element_bounds(element, elements_map),
                        )
                })
                .count();
            let is_final_attempt = attempt == 3 || in_range == total_eligible;
            if in_range == 0 {
                continue;
            }
            let raw_segments =
                collect_segments(search_box, None).ok_or(BucketFillFailureReason::NoOwner)?;
            match build_and_select(&raw_segments) {
                Built::TooComplex => return Err(BucketFillFailureReason::NoOwner),
                Built::Selected(faces, selection) => {
                    let gap = options.gap_tolerance;
                    let clear_of_frontier = faces[selection.face].ring.iter().all(|p| {
                        p[0] > search_box[0] + gap
                            && p[1] > search_box[1] + gap
                            && p[0] < search_box[2] - gap
                            && p[1] < search_box[3] - gap
                    });
                    if is_final_attempt || clear_of_frontier {
                        found = Some((faces, selection));
                        break;
                    }
                }
                Built::Nothing => {}
            }
            if is_final_attempt {
                break;
            }
        }
        found.ok_or(BucketFillFailureReason::NoOwner)?
    };
    let face = &faces[selection.face];
    let holes: Vec<&Face> = selection.holes.iter().map(|&i| &faces[i]).collect();

    // simplify and validate
    let hole_rings: Vec<&[[f64; 2]]> = holes.iter().map(|h| h.ring.as_slice()).collect();
    let (scene_points, spliced_hole_indices) = finalize_polygon(&face.ring, &hole_rings, options)
        .ok_or(BucketFillFailureReason::InvalidPolygon)?;
    let region = gps(&scene_points);
    if polygon_area_with(&region, 0.0) < options.min_area {
        return Err(BucketFillFailureReason::TooSmall);
    }

    let mut contributors = face.contributors.clone();
    for &index in &spliced_hole_indices {
        contributors.extend(holes[index].contributors.iter().cloned());
    }

    // z-order: above the topmost opaque element overlapping the region,
    // else below the lowest element that must stay visible
    let mut participant_ids: HashSet<&str> = HashSet::new();
    if let Some(owner) = owner {
        participant_ids.insert(owner.base.id.as_str());
    }
    participant_ids.extend(contributors.iter().map(String::as_str));
    for hole in &holes {
        participant_ids.extend(hole.contributors.iter().map(String::as_str));
    }

    let [region_min_x, region_min_y, region_max_x, region_max_y] =
        get_bounds_from_points(&scene_points, 0.0);
    let misses_region = |[min_x, min_y, max_x, max_y]: Bounds| {
        max_x < region_min_x || min_x > region_max_x || max_y < region_min_y || min_y > region_max_y
    };
    let mark_inside_region = |element: &Element| -> bool {
        let bounds = get_element_bounds(element, elements_map);
        if misses_region(bounds) {
            return false;
        }
        let [min_x, min_y, max_x, max_y] = bounds;
        let mut samples = vec![[(min_x + max_x) / 2.0, (min_y + max_y) / 2.0]];
        let points = match &element.kind {
            ElementKind::Line(_) | ElementKind::Arrow(_) | ElementKind::Freedraw(_) => {
                element.kind.points()
            }
            _ => None,
        };
        if let Some(points) = points {
            let center = element_center_point(element, elements_map);
            let step = (points.len() / 8).max(1);
            for p in points.iter().step_by(step) {
                samples.push(rotate(
                    [element.base.x + p[0], element.base.y + p[1]],
                    center,
                    element.base.angle.0,
                ));
            }
        }
        samples
            .iter()
            .any(|&sample| polygon_includes_point(gp(sample), &region))
    };
    let paint_overlaps_region = |element: &Element| -> bool {
        if misses_region(get_element_bounds(element, elements_map)) {
            return false;
        }
        if is_point_in_element(point, element, elements_map) {
            return true;
        }
        let outline = element_line_segments(element, elements_map);
        let outline_step = (outline.len() / 16).max(1);
        if outline
            .iter()
            .step_by(outline_step)
            .any(|segment| polygon_includes_point(gp(segment[0]), &region))
        {
            return true;
        }
        let ring_step = (scene_points.len() / 16).max(1);
        scene_points
            .iter()
            .step_by(ring_step)
            .any(|&p| is_point_in_element(p, element, elements_map))
    };

    let mut lowest_above: Option<&Element> = None;
    let mut covering: Option<&Element> = None;
    for &element in elements {
        if renders_opaque_fill(element) && paint_overlaps_region(element) {
            covering = Some(element);
        }
        let must_stay_above = participant_ids.contains(element.base.id.as_str())
            || (renders_any_mark(element) && mark_inside_region(element));
        if must_stay_above && lowest_above.is_none() {
            lowest_above = Some(element);
        }
    }
    let insertion = match covering {
        Some(covering) => BucketFillInsertion {
            placement: Placement::Above,
            element_id: covering.base.id.clone(),
        },
        None => BucketFillInsertion {
            placement: Placement::Below,
            element_id: lowest_above
                .or(elements.last().copied())
                .map(|e| e.base.id.clone())
                .unwrap_or_default(),
        },
    };

    let owner_id = owner.map(|o| o.base.id.clone());
    Ok(BucketFillPolygon {
        boundary_element_ids: contributors
            .into_iter()
            .filter(|id| Some(id) != owner_id.as_ref())
            .collect(),
        owner_id,
        scene_points,
        insertion,
    })
}

/// `isRestylableFill({ hitElement, scenePoints, elementsMap })`: the clicked
/// element is fill-compatible paint covering the same region (net area
/// within 5 %, bounds within [`BUCKET_FILL_REGION_MATCH_TOLERANCE`]), so the
/// tool restyles it instead of stacking a new fill.
pub fn is_restylable_fill(
    hit_element: &Element,
    scene_points: &[[f64; 2]],
    elements_map: &ElementsMap<'_>,
) -> bool {
    if !is_bucket_fill_compatible(hit_element) {
        return false;
    }
    let Some((points, _)) = line_points(hit_element) else {
        return false;
    };
    let center = element_center_point(hit_element, elements_map);
    let ring: Vec<[f64; 2]> = points
        .iter()
        .map(|p| {
            rotate(
                [hit_element.base.x + p[0], hit_element.base.y + p[1]],
                center,
                hit_element.base.angle.0,
            )
        })
        .collect();
    let fill_area = polygon_area_with(&gps(&ring), 0.0);
    let region_area = polygon_area_with(&gps(scene_points), 0.0);
    let same_area = (fill_area - region_area).abs() <= 0.05 * js::max(fill_area, region_area);
    let [a_min_x, a_min_y, a_max_x, a_max_y] = get_bounds_from_points(&ring, 0.0);
    let [b_min_x, b_min_y, b_max_x, b_max_y] = get_bounds_from_points(scene_points, 0.0);
    let tolerance = BUCKET_FILL_REGION_MATCH_TOLERANCE;
    let same_bounds = (a_min_x - b_min_x).abs() <= tolerance
        && (a_min_y - b_min_y).abs() <= tolerance
        && (a_max_x - b_max_x).abs() <= tolerance
        && (a_max_y - b_max_y).abs() <= tolerance;
    same_area && same_bounds
}

// -- the tool (App.bucketFill.ts) ------------------------------------------------

/// `getBucketFillBackgroundColor(backgroundColor, theme)`
/// (`App.bucketFill.ts:287-300`): the colour the tool fills with, the shared
/// `currentItemBackgroundColor` or, when that is transparent, the default
/// green background shade; `dark` (display only, e.g. the cursor) applies
/// the dark mode filter.
pub fn get_bucket_fill_background_color(background_color: &str, dark: bool) -> String {
    if is_transparent(background_color) {
        COLOR_PALETTE.green[DEFAULT_ELEMENT_BACKGROUND_COLOR_INDEX].to_string()
    } else {
        apply_dark_mode_filter(background_color, dark)
    }
}

/// What a bucket fill click reads: the non-deleted scene and the whole
/// scene, the element under the pointer (`App.getElementAtPosition`), the
/// top frame at the click (`getTopLayerFrameAtSceneCoords`), the style
/// from app state, and the new element's drawn `randomId()`,
/// `randomInteger()` and `getUpdatedTimestamp()`.
pub struct BucketFillClick<'a> {
    pub point: [f64; 2],
    /// `scene.getNonDeletedElements()`, in z-order.
    pub elements: &'a [&'a Element],
    pub elements_map: &'a ElementsMap<'a>,
    /// `scene.getElementsIncludingDeleted()`: the insertion index is
    /// resolved against it.
    pub elements_including_deleted: &'a [&'a Element],
    pub hit_element: Option<&'a Element>,
    pub top_layer_frame_id: Option<&'a str>,
    /// `currentItemBackgroundColor`.
    pub background_color: &'a str,
    /// `currentItemFillStyle`.
    pub fill_style: FillStyle,
    /// `currentItemOpacity`.
    pub opacity: f64,
    pub id: &'a str,
    pub seed: f64,
    pub timestamp: f64,
}

/// What `AppBucketFill.fill` does to the scene.
#[derive(Debug, Clone, PartialEq)]
pub enum BucketFillOutcome {
    /// Nothing: clicking empty canvas (`no_owner`) stays silent.
    Nothing,
    /// A toast (`bucketfill.tooComplex` or `bucketfill.noRegion`, 3 s).
    Toast(&'static str),
    /// Restyle the existing fill-compatible element (one undoable step,
    /// its shape cache evicted).
    Restyle {
        element_id: String,
        background_color: String,
        fill_style: FillStyle,
        opacity: f64,
    },
    /// The element to restyle already has the current style.
    Unchanged,
    /// Insert the new fill at `index` of the deleted-inclusive elements
    /// (`None`: at the end), unselected, the tool staying active.
    Insert {
        fill: Box<Element>,
        index: Option<usize>,
    },
}

/// `AppBucketFill.restyle(element)`: the restyle, or [`BucketFillOutcome::Unchanged`].
fn restyle(element: &Element, click: &BucketFillClick<'_>) -> BucketFillOutcome {
    let background_color = get_bucket_fill_background_color(click.background_color, false);
    if element.base.background_color == background_color
        && element.base.fill_style == click.fill_style
        && element.base.opacity == click.opacity
    {
        return BucketFillOutcome::Unchanged;
    }
    BucketFillOutcome::Restyle {
        element_id: element.base.id.clone(),
        background_color,
        fill_style: click.fill_style,
        opacity: click.opacity,
    }
}

/// `AppBucketFill.fill(scenePointer)` (`App.bucketFill.ts:156-279`), run on
/// pointer up at the pointer-down position: restyle the paint under the
/// click when it already fills this region (or no region can be derived),
/// toast when the region is open, too small or too complex, else a new
/// strokeless line polygon of the region, taking the owner's frame and
/// groups (an owner-less fill: the frame at the click and the groups every
/// boundary shares), inserted where [`compute_bucket_fill_polygon`] says.
pub fn bucket_fill_click(click: &BucketFillClick<'_>) -> BucketFillOutcome {
    let background_color = get_bucket_fill_background_color(click.background_color, false);
    let result = compute_bucket_fill_polygon(
        click.point,
        click.elements,
        click.elements_map,
        &BucketFillOptions::default(),
    );
    let fill = match result {
        Err(reason) => {
            if let Some(hit) = click
                .hit_element
                .filter(|hit| is_bucket_fill_compatible(hit))
            {
                return restyle(hit, click);
            }
            return match reason {
                BucketFillFailureReason::TooComplex => {
                    BucketFillOutcome::Toast("bucketfill.tooComplex")
                }
                BucketFillFailureReason::NoOwner => BucketFillOutcome::Nothing,
                _ => BucketFillOutcome::Toast("bucketfill.noRegion"),
            };
        }
        Ok(fill) => fill,
    };

    if let Some(hit) = click
        .hit_element
        .filter(|hit| is_restylable_fill(hit, &fill.scene_points, click.elements_map))
    {
        return restyle(hit, click);
    }

    let (width, height) = get_size_from_points(&fill.scene_points);
    let [origin_x, origin_y] = fill.scene_points[0];
    let points: Vec<[f64; 2]> = fill
        .scene_points
        .iter()
        .map(|p| [p[0] - origin_x, p[1] - origin_y])
        .collect();

    let owner = fill
        .owner_id
        .as_deref()
        .and_then(|id| click.elements_map.get(id));
    let frame_id = match owner {
        Some(owner)
            if matches!(
                owner.kind,
                ElementKind::Frame(_) | ElementKind::MagicFrame(_)
            ) =>
        {
            Some(owner.base.id.clone())
        }
        Some(owner) => owner.base.frame_id.clone(),
        None => click.top_layer_frame_id.map(str::to_string),
    };
    let group_ids: Vec<String> = match owner {
        Some(owner) => owner.base.group_ids.iter().map(|g| g.to_string()).collect(),
        None => fill
            .boundary_element_ids
            .iter()
            .filter_map(|id| click.elements_map.get(id))
            .map(|e| {
                e.base
                    .group_ids
                    .iter()
                    .map(|g| g.to_string())
                    .collect::<Vec<_>>()
            })
            .reduce(|common, group_ids| {
                common
                    .into_iter()
                    .filter(|g| group_ids.contains(g))
                    .collect()
            })
            .unwrap_or_default(),
    };

    let mut opts = Map::new();
    opts.insert("x".into(), json!(origin_x));
    opts.insert("y".into(), json!(origin_y));
    opts.insert("width".into(), json!(width));
    opts.insert("height".into(), json!(height));
    opts.insert("strokeColor".into(), json!("transparent"));
    opts.insert("backgroundColor".into(), json!(background_color));
    opts.insert("fillStyle".into(), json!(click.fill_style));
    opts.insert("strokeWidth".into(), json!(1));
    opts.insert("strokeStyle".into(), json!("solid"));
    opts.insert("roughness".into(), json!(0));
    opts.insert("roundness".into(), Value::Null);
    opts.insert("opacity".into(), json!(click.opacity));
    opts.insert("frameId".into(), json!(frame_id));
    let mut m = new_element_base("line", &opts, click.id, click.seed, click.timestamp);
    m.insert("groupIds".into(), json!(group_ids));
    m.insert("points".into(), json!(points));
    m.insert("startBinding".into(), Value::Null);
    m.insert("endBinding".into(), Value::Null);
    m.insert("startArrowhead".into(), Value::Null);
    m.insert("endArrowhead".into(), Value::Null);
    m.insert("polygon".into(), json!(true));
    let Ok(element) = Element::from_map(m) else {
        return BucketFillOutcome::Nothing;
    };

    let anchor = click
        .elements_including_deleted
        .iter()
        .position(|e| e.base.id == fill.insertion.element_id);
    let index = anchor.map(|i| match fill.insertion.placement {
        Placement::Above => i + 1,
        Placement::Below => i,
    });
    BucketFillOutcome::Insert {
        fill: Box::new(element),
        index,
    }
}
