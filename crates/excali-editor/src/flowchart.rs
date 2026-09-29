//! Flowcharts from the keyboard: Ctrl+Arrow puts pending nodes (copies of
//! the selected node, each bound to it by an elbow arrow) next to it, and
//! Alt+Arrow walks the arrows from node to node.
//!
//! Upstream, at the pinned commit:
//!
//! - `packages/element/src/flowchart.ts`: `mergeIntervals`,
//!   `intervalIsFree` and `findNearestFreeSlot` (`:62-111`),
//!   `getConnectedFlowchartNodes` (`:113-151`), `placeCluster`
//!   (`:153-230`), `cloneFlowchartNode` (`:232-270`), `addNewNodes`
//!   (`:272-308`), `createBindingArrow` (`:310-450`), `FlowChartNavigator`
//!   (`:452-680`), `FlowChartCreator` (`:682-754`) and `isNodeInFlowchart`
//!   (`:756-771`);
//! - `packages/element/src/Scene.ts:303-309`: `triggerUpdate`, whose
//!   scene nonce draws a random integer when the start node changes;
//! - `packages/excalidraw/components/App.flowchart.ts:53-178`: what App
//!   does with the keys' operations ([`AppFlowchart`]).
//!
//! [`crate::keyboard`] resolves the keys ([`KeyEffect::FlowchartCreate`]
//! and the rest); a host answers each effect with
//! [`AppFlowchart::answer`] and applies the [`FlowchartOperation`] it
//! returns (reveal, select, insert, capture).

use std::collections::HashSet;

use excali_core::app_state::AppState;
use excali_core::color::is_transparent;
use excali_core::constants::DEFAULT_STICKY_NOTE_BG;
use excali_core::element::{BindMode, Element, ElementKind};
use excali_math::js;
use excali_scene::bounds::{Bounds, ElementsMap};
use excali_scene::frame::{element_overlaps_with_frame, elements_are_in_frame_bounds};
use excali_scene::heading::Heading;
use serde_json::{json, Map, Value};

use crate::binding::{bind_binding_element, BindingEnd};
use crate::edit_actions::{get_frame_children_insertion_index, EditEnv};
use crate::elbow_arrow::{self, ElbowArrowUpdate, ElbowArrowUpdates};
use crate::geometry::{aabb_for_element, heading_for_point_from_element};
use crate::keyboard::{FlowchartKeys, KeyEffect, LinkDirection};
use crate::linear_element_editor::{move_points, MovePointsOtherUpdates, PointUpdate};
use crate::mutate::new_element_with;
use crate::new_element::new_element_base;
use crate::scene::{ElementUpdate, Scene};

/// `VERTICAL_OFFSET` and `HORIZONTAL_OFFSET`: the gap between a node and
/// the next.
const VERTICAL_OFFSET: f64 = 100.0;
const HORIZONTAL_OFFSET: f64 = 100.0;

/// The gap between a node's outline and its arrow's end
/// (`createBindingArrow`'s `PADDING`).
const PADDING: f64 = 6.0;

/// A span of the cross axis (`Interval`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interval {
    pub start: f64,
    pub end: f64,
}

/// `mergeIntervals(intervals)`: sorted by start (a stable sort), the
/// overlapping and touching ones joined.
fn merge_intervals(intervals: &[Interval]) -> Vec<Interval> {
    let mut sorted = intervals.to_vec();
    sorted.sort_by(|a, b| compare_by(a.start - b.start));
    let mut merged: Vec<Interval> = Vec::new();
    for interval in sorted {
        match merged.last_mut() {
            Some(last) if interval.start <= last.end => {
                last.end = js::max(last.end, interval.end);
            }
            _ => merged.push(interval),
        }
    }
    merged
}

/// A comparator's number as an ordering (`NaN` and `0` equal, as
/// `Array.prototype.sort` reads it).
fn compare_by(d: f64) -> std::cmp::Ordering {
    if d < 0.0 {
        std::cmp::Ordering::Less
    } else if d > 0.0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

/// `intervalIsFree(start, size, occupied)`.
fn interval_is_free(start: f64, size: f64, occupied: &[Interval]) -> bool {
    occupied
        .iter()
        .all(|o| start + size <= o.start || start >= o.end)
}

/// `clamp(value, min, max)` (`math/src/utils.ts`).
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    js::min(js::max(value, min), max)
}

/// `findNearestFreeSlot(ideal, size, occupied)`: the nearest `start` for
/// a segment of `size` clear of every occupied interval (sorted and
/// merged), searching both sides of `ideal`; a tie goes to the later gap.
pub fn find_nearest_free_slot(ideal: f64, size: f64, occupied: &[Interval]) -> f64 {
    if interval_is_free(ideal, size, occupied) {
        return ideal;
    }
    let gap_starts: Vec<f64> = std::iter::once(f64::NEG_INFINITY)
        .chain(occupied.iter().map(|o| o.end))
        .collect();
    let gap_ends: Vec<f64> = occupied
        .iter()
        .map(|o| o.start)
        .chain(std::iter::once(f64::INFINITY))
        .collect();
    let mut best = ideal;
    let mut best_distance = f64::INFINITY;
    for (&gap_start, &gap_end) in gap_starts.iter().zip(&gap_ends) {
        if gap_end - gap_start < size {
            continue;
        }
        let start = clamp(ideal, gap_start, gap_end - size);
        let distance = (start - ideal).abs();
        if distance <= best_distance {
            best = start;
            best_distance = distance;
        }
    }
    best
}

fn is_elbow_arrow(e: &Element) -> bool {
    matches!(&e.kind, ElementKind::Arrow(a) if a.elbowed)
}

fn is_arrow(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Arrow(_))
}

/// `isFlowchartNodeElement(element)` (`typeChecks.ts:286-295`).
pub fn is_flowchart_node(e: &Element) -> bool {
    matches!(
        e.kind,
        ElementKind::Rectangle
            | ElementKind::StickyNote(_)
            | ElementKind::Ellipse
            | ElementKind::Diamond
    )
}

fn bindings(e: &Element) -> (Option<&str>, Option<&str>) {
    match &e.kind {
        ElementKind::Arrow(a) => (
            a.linear
                .start_binding
                .as_ref()
                .map(|b| b.element_id.as_str()),
            a.linear.end_binding.as_ref().map(|b| b.element_id.as_str()),
        ),
        ElementKind::Line(l) => (
            l.linear
                .start_binding
                .as_ref()
                .map(|b| b.element_id.as_str()),
            l.linear.end_binding.as_ref().map(|b| b.element_id.as_str()),
        ),
        _ => (None, None),
    }
}

/// `getConnectedFlowchartNodes(node, elementsMap)`: every bindable element
/// the elbow arrows connect to `node`, directly or through others, in the
/// breadth-first order they are found.
fn get_connected_flowchart_nodes<'a>(node_id: &str, scene: &'a Scene) -> Vec<&'a Element> {
    let non_deleted = scene.non_deleted();
    let arrows: Vec<&Element> = non_deleted
        .iter()
        .copied()
        .filter(|e| is_elbow_arrow(e))
        .collect();
    let mut visited: HashSet<String> = HashSet::from([node_id.to_owned()]);
    let mut queue: std::collections::VecDeque<String> = [node_id.to_owned()].into();
    let mut connected = Vec::new();
    while let Some(current) = queue.pop_front() {
        for arrow in &arrows {
            let (start, end) = bindings(arrow);
            let neighbor = if start == Some(current.as_str()) {
                end
            } else if end == Some(current.as_str()) {
                start
            } else {
                None
            };
            let Some(neighbor) = neighbor else { continue };
            if neighbor.is_empty() || visited.contains(neighbor) {
                continue;
            }
            visited.insert(neighbor.to_owned());
            if let Some(n) = scene.get_non_deleted(neighbor).filter(|n| n.is_bindable()) {
                connected.push(n);
                queue.push_back(neighbor.to_owned());
            }
        }
    }
    connected
}

/// Where `placeCluster` puts the nodes, and the cross-axis start of the
/// cluster.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterPlacement {
    pub positions: Vec<[f64; 2]>,
    pub cross_start: f64,
}

/// `placeCluster(parent, direction, count, obstacles, stickyCrossStart)`:
/// `count` copies of `parent` one gap away from it in `direction`, slid
/// along the cross axis into the free slot nearest the one centred on the
/// parent, clear of the `obstacles` in their band by a gap. A pending
/// cluster's start (`sticky`) holds when the grown cluster still fits
/// there or one step before it.
pub fn place_cluster(
    parent: &Element,
    direction: LinkDirection,
    count: usize,
    obstacles: &[Bounds],
    sticky: Option<f64>,
) -> ClusterPlacement {
    let p = &parent.base;
    let horizontal = matches!(direction, LinkDirection::Left | LinkDirection::Right);
    let (node_primary_size, node_cross_size) = if horizontal {
        (p.width, p.height)
    } else {
        (p.height, p.width)
    };
    let (primary_gap, cross_gap) = if horizontal {
        (HORIZONTAL_OFFSET, VERTICAL_OFFSET)
    } else {
        (VERTICAL_OFFSET, HORIZONTAL_OFFSET)
    };
    let parent_primary_start = if horizontal { p.x } else { p.y };
    let parent_cross_center = if horizontal {
        p.y + p.height / 2.0
    } else {
        p.x + p.width / 2.0
    };
    let primary_start = match direction {
        LinkDirection::Right | LinkDirection::Down => {
            parent_primary_start + node_primary_size + primary_gap
        }
        _ => parent_primary_start - primary_gap - node_primary_size,
    };

    let in_band: Vec<Interval> = obstacles
        .iter()
        .filter(|b| {
            let (start, end) = if horizontal {
                (b[0], b[2])
            } else {
                (b[1], b[3])
            };
            start < primary_start + node_primary_size && end > primary_start
        })
        .map(|b| Interval {
            start: (if horizontal { b[1] } else { b[0] }) - cross_gap,
            end: (if horizontal { b[3] } else { b[2] }) + cross_gap,
        })
        .collect();
    let occupied = merge_intervals(&in_band);

    let n = count as f64;
    let step = node_cross_size + cross_gap;
    let cluster_cross_size = n * node_cross_size + (n - 1.0) * cross_gap;

    let anchored = sticky.and_then(|sticky| {
        let mut starts: Vec<f64> = [sticky, sticky - step]
            .into_iter()
            .filter(|&s| interval_is_free(s, cluster_cross_size, &occupied))
            .collect();
        let off = |s: f64| (s + cluster_cross_size / 2.0 - parent_cross_center).abs();
        starts.sort_by(|&a, &b| compare_by(off(a) - off(b)));
        starts.first().copied()
    });
    let cross_start = anchored.unwrap_or_else(|| {
        find_nearest_free_slot(
            parent_cross_center - cluster_cross_size / 2.0,
            cluster_cross_size,
            &occupied,
        )
    });

    let positions = (0..count)
        .map(|index| {
            let cross = cross_start + index as f64 * step;
            if horizontal {
                [primary_start, cross]
            } else {
                [cross, primary_start]
            }
        })
        .collect();
    ClusterPlacement {
        positions,
        cross_start,
    }
}

/// What the creator reads of the app state.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowchartAppState {
    /// `appState.currentItemEndArrowhead` as it is (JSON).
    pub current_item_end_arrowhead: Value,
    /// `appState.zoom.value`.
    pub zoom: f64,
}

impl FlowchartAppState {
    pub fn from_app_state(app_state: &AppState) -> FlowchartAppState {
        FlowchartAppState {
            current_item_end_arrowhead: app_state
                .get("currentItemEndArrowhead")
                .cloned()
                .unwrap_or(Value::Null),
            zoom: app_state.zoom().unwrap_or(1.0),
        }
    }
}

/// `opts.x || null`: a falsy JSON value is `null`.
fn or_null(value: &Value) -> Value {
    match value {
        Value::String(s) if s.is_empty() => Value::Null,
        Value::Bool(false) => Value::Null,
        Value::Number(n) if n.as_f64() == Some(0.0) => Value::Null,
        v => v.clone(),
    }
}

/// `newElement` / `newStickyNoteElement` over the element base
/// (`_newElementBase`): the timestamp, then the id, then the seed.
fn new_base<E: EditEnv>(ty: &str, opts: &Map<String, Value>, env: &mut E) -> Map<String, Value> {
    let timestamp = excali_core::restore::RestoreEnv::now(env);
    let id = env.random_id();
    let seed = excali_core::restore::RestoreEnv::random_integer(env);
    new_element_base(ty, opts, &id, seed, timestamp)
}

/// `cloneFlowchartNode(template, x, y)`: a node of the template's type,
/// size and style at `(x, y)`.
fn clone_flowchart_node<E: EditEnv>(template: &Element, x: f64, y: f64, env: &mut E) -> Element {
    let t = template.to_map();
    let mut opts = Map::new();
    opts.insert("x".into(), json!(x));
    opts.insert("y".into(), json!(y));
    for key in [
        "width",
        "height",
        "roundness",
        "roughness",
        "backgroundColor",
        "strokeColor",
        "strokeWidth",
        "opacity",
        "fillStyle",
        "strokeStyle",
    ] {
        if let Some(v) = t.get(key) {
            opts.insert(key.into(), v.clone());
        }
    }
    let ty = template.element_type().as_str();
    let mut base = new_base(ty, &opts, env);
    if let ElementKind::StickyNote(sticky) = &template.kind {
        // newStickyNoteElement: baseHeight, then normalizeStickyNoteStyle
        base.insert("baseHeight".into(), json!(sticky.base_height));
        let node = Element::from_map(base).expect("a sticky note");
        let color = |c: &str, default: &str| {
            if c.is_empty() || is_transparent(c) {
                default.to_owned()
            } else {
                c.to_owned()
            }
        };
        let mut updates = Map::new();
        updates.insert(
            "backgroundColor".into(),
            json!(color(&node.base.background_color, DEFAULT_STICKY_NOTE_BG)),
        );
        updates.insert(
            "strokeColor".into(),
            json!(color(&node.base.stroke_color, "#1e1e1e")),
        );
        updates.insert("fillStyle".into(), json!("solid"));
        return new_element_with(&node, updates, false, env).expect("a sticky note");
    }
    Element::from_map(base).expect("a flowchart node")
}

/// `{...arrow, ...update}`: the keys `updateElbowArrowPoints` returns,
/// assigned as they are (no version bump).
fn spread_elbow_update(arrow: &mut Element, update: ElbowArrowUpdate) {
    let b = &mut arrow.base;
    if let Some(x) = update.x {
        b.x = x;
    }
    if let Some(y) = update.y {
        b.y = y;
    }
    if let Some(w) = update.width {
        b.width = w;
    }
    if let Some(h) = update.height {
        b.height = h;
    }
    if let ElementKind::Arrow(a) = &mut arrow.kind {
        if let Some(points) = update.points {
            a.linear.points = points;
        }
        if let Some(v) = update.fixed_segments {
            a.fixed_segments = Some(v);
        }
        if let Some(v) = update.start_is_special {
            a.start_is_special = Some(v);
        }
        if let Some(v) = update.end_is_special {
            a.end_is_special = Some(v);
        }
        if let Some(v) = update.start_binding {
            a.linear.start_binding = v;
        }
        if let Some(v) = update.end_binding {
            a.linear.end_binding = v;
        }
    }
}

/// `createBindingArrow(start, end, direction, appState, scene)`: an elbow
/// arrow from the start node (in the scene) to the pending `end`, bound at
/// both ends and routed. The start node's `boundElements` take the arrow
/// in the scene, and `end`'s in place.
fn create_binding_arrow<E: EditEnv>(
    start_id: &str,
    end: &mut Element,
    direction: LinkDirection,
    app: &FlowchartAppState,
    scene: &mut Scene,
    env: &mut E,
) -> Element {
    let start = scene.get(start_id).expect("the start node").clone();
    let (s, e) = (&start.base, &end.base);
    let (start_x, start_y) = match direction {
        LinkDirection::Up => (s.x + s.width / 2.0, s.y - PADDING),
        LinkDirection::Down => (s.x + s.width / 2.0, s.y + s.height + PADDING),
        LinkDirection::Right => (s.x + s.width + PADDING, s.y + s.height / 2.0),
        LinkDirection::Left => (s.x - PADDING, s.y + s.height / 2.0),
    };
    let (end_x, end_y) = match direction {
        LinkDirection::Up => (
            e.x + e.width / 2.0 - start_x,
            e.y + e.height - start_y + PADDING,
        ),
        LinkDirection::Down => (e.x + e.width / 2.0 - start_x, e.y - start_y - PADDING),
        LinkDirection::Right => (e.x - start_x - PADDING, e.y - start_y + e.height / 2.0),
        LinkDirection::Left => (
            e.x + e.width - start_x + PADDING,
            e.y - start_y + e.height / 2.0,
        ),
    };

    // newArrowElement({ type: "arrow", elbowed: true, ... })
    let mut opts = Map::new();
    opts.insert("x".into(), json!(start_x));
    opts.insert("y".into(), json!(start_y));
    opts.insert("strokeColor".into(), json!(s.stroke_color));
    let st = start.to_map();
    for key in ["strokeStyle", "strokeWidth", "opacity", "roughness"] {
        if let Some(v) = st.get(key) {
            opts.insert(key.into(), v.clone());
        }
    }
    let mut m = new_base("arrow", &opts, env);
    m.insert("points".into(), json!([[0, 0], [end_x, end_y]]));
    m.insert("startBinding".into(), Value::Null);
    m.insert("endBinding".into(), Value::Null);
    m.insert("startArrowhead".into(), Value::Null);
    m.insert(
        "endArrowhead".into(),
        or_null(&app.current_item_end_arrowhead),
    );
    m.insert("elbowed".into(), json!(true));
    m.insert("fixedSegments".into(), json!([]));
    m.insert("startIsSpecial".into(), json!(false));
    m.insert("endIsSpecial".into(), json!(false));
    let arrow = Element::from_map(m).expect("an elbow arrow");
    let arrow_id = arrow.base.id.clone();
    let end_id = end.base.id.clone();

    // bindBindingElement at both ends, scene.mutateElement changing the
    // arrow and the pending node in place
    let mut work: Vec<Element> = scene.elements().to_vec();
    work.push(end.clone());
    work.push(arrow);
    let mut work = Scene::new(work);
    for (target, which) in [
        (start_id, BindingEnd::Start),
        (end_id.as_str(), BindingEnd::End),
    ] {
        let version = |w: &Scene| w.get(start_id).map(|s| s.base.version);
        let before = version(&work);
        bind_binding_element(
            &mut work,
            env,
            &arrow_id,
            target,
            BindMode::Orbit,
            which,
            app.zoom,
            None,
            true,
            true,
        );
        if version(&work) != before {
            // the start node is the scene's: scene.mutateElement ends in
            // triggerUpdate(), which draws the scene nonce
            let _scene_nonce = excali_core::restore::RestoreEnv::random_integer(env);
        }
    }
    *end = work.get(&end_id).expect("the pending node").clone();

    // LinearElementEditor.movePoints(arrow, scene, { 1: points[1] }): the
    // scene's map has no pending node
    let mut work = Scene::new(
        work.elements()
            .iter()
            .filter(|el| el.base.id != end_id)
            .cloned()
            .collect(),
    );
    let second = match &work.get(&arrow_id).expect("the arrow").kind {
        ElementKind::Arrow(a) => a.linear.points[1],
        _ => unreachable!("an arrow"),
    };
    move_points(
        &mut work,
        env,
        &arrow_id,
        &[(1, PointUpdate::to(second))],
        MovePointsOtherUpdates::default(),
    );
    let mut arrow = work.get(&arrow_id).expect("the arrow").clone();
    if let Some(next) = work.get(start_id) {
        scene.replace_element(next.clone());
    }

    // updateElbowArrowPoints(arrow, { ...elementsMap, start, end, arrow },
    // { points })
    let routed = {
        let map = elbow_arrow::ElementsMap::new(
            scene
                .elements()
                .iter()
                .filter(|el| !el.base.is_deleted)
                .chain([&*end, &arrow]),
        );
        let points = match &arrow.kind {
            ElementKind::Arrow(a) => a.linear.points.clone(),
            _ => unreachable!("an arrow"),
        };
        elbow_arrow::update_elbow_arrow_points(
            &arrow,
            &map,
            &ElbowArrowUpdates {
                points: Some(points),
                ..ElbowArrowUpdates::default()
            },
        )
    };
    if let Ok(update) = routed {
        spread_elbow_update(&mut arrow, update);
    }
    arrow
}

/// `addNewNodes(startNode, appState, direction, scene, numberOfNodes,
/// stickyCrossStart)`: `count` nodes next to the start node, each followed
/// by its arrow, and the cluster's cross-axis start.
#[allow(clippy::too_many_arguments)]
pub fn add_new_nodes<E: EditEnv>(
    start_id: &str,
    app: &FlowchartAppState,
    direction: LinkDirection,
    scene: &mut Scene,
    count: usize,
    sticky: Option<f64>,
    env: &mut E,
) -> (Vec<Element>, f64) {
    let obstacles: Vec<Bounds> = get_connected_flowchart_nodes(start_id, scene)
        .into_iter()
        .map(|n| aabb_for_element(n, None))
        .collect();
    let start = scene.get(start_id).expect("the start node").clone();
    let placement = place_cluster(&start, direction, count, &obstacles, sticky);
    let mut nodes = Vec::with_capacity(count * 2);
    for [x, y] in placement.positions {
        let template = scene.get(start_id).expect("the start node").clone();
        let mut next = clone_flowchart_node(&template, x, y, env);
        let arrow = create_binding_arrow(start_id, &mut next, direction, app, scene, env);
        nodes.push(next);
        nodes.push(arrow);
    }
    (nodes, placement.cross_start)
}

/// `FlowChartCreator`: the pending nodes of a Ctrl+Arrow session.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlowChartCreator {
    pub is_creating_chart: bool,
    number_of_nodes: usize,
    direction: Option<LinkDirection>,
    /// The pending cluster's cross-axis start, so growing it keeps the
    /// pending nodes in place.
    cluster_cross_start: Option<f64>,
    pub pending_nodes: Option<Vec<Element>>,
}

impl FlowChartCreator {
    /// `createNodes(startNode, appState, direction, scene)`: one more
    /// pending node in the same direction, or one in a new direction; the
    /// pending nodes join the start node's frame when each of them lies in
    /// or overlaps it.
    pub fn create_nodes<E: EditEnv>(
        &mut self,
        start_id: &str,
        app: &FlowchartAppState,
        direction: LinkDirection,
        scene: &mut Scene,
        env: &mut E,
    ) {
        if Some(direction) != self.direction {
            self.number_of_nodes = 1;
            self.cluster_cross_start = None;
        } else {
            self.number_of_nodes += 1;
        }
        let (nodes, cross_start) = add_new_nodes(
            start_id,
            app,
            direction,
            scene,
            self.number_of_nodes,
            self.cluster_cross_start,
            env,
        );
        self.is_creating_chart = true;
        self.direction = Some(direction);
        self.cluster_cross_start = Some(cross_start);

        let frame_id = scene.get(start_id).and_then(|s| s.base.frame_id.clone());
        let frame = frame_id.as_deref().and_then(|f| scene.get_non_deleted(f));
        let join = frame.is_some_and(|frame| {
            let map = ElementsMap::new(scene.non_deleted());
            nodes.iter().all(|node| {
                elements_are_in_frame_bounds(&[node], frame, &map)
                    || element_overlaps_with_frame(node, frame, &map).unwrap_or(false)
            })
        });
        if join {
            let mut pending = Scene::new(nodes);
            let ids: Vec<String> = pending
                .elements()
                .iter()
                .map(|e| e.base.id.clone())
                .collect();
            for id in ids {
                pending.mutate_element(
                    &id,
                    ElementUpdate {
                        frame_id: Some(frame_id.clone()),
                        ..ElementUpdate::default()
                    },
                    env,
                );
            }
            self.pending_nodes = Some(pending.elements().to_vec());
        } else {
            self.pending_nodes = Some(nodes);
        }
    }

    /// `clear()`.
    pub fn clear(&mut self) {
        *self = FlowChartCreator::default();
    }
}

fn heading_of(direction: LinkDirection) -> Heading {
    match direction {
        LinkDirection::Up => Heading::Up,
        LinkDirection::Right => Heading::Right,
        LinkDirection::Down => Heading::Down,
        LinkDirection::Left => Heading::Left,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Relatives {
    Predecessors,
    Successors,
}

/// `FlowChartNavigator.getNodeRelatives(type, node, elementsMap,
/// direction)`: the nodes at the other end of the elbow arrows leaving
/// (successors) or reaching (predecessors) `node` on its `direction`
/// side, by where the arrow meets the node.
fn get_node_relatives<'a>(
    kind: Relatives,
    node: &Element,
    scene: &'a Scene,
    direction: LinkDirection,
) -> Vec<&'a Element> {
    let want = heading_of(direction);
    let aabb = aabb_for_element(node, None);
    let mut out = Vec::new();
    for el in scene.non_deleted() {
        let ElementKind::Arrow(a) = &el.kind else {
            continue;
        };
        if !a.elbowed {
            continue;
        }
        let (opposite, own) = match kind {
            Relatives::Predecessors => (&a.linear.start_binding, &a.linear.end_binding),
            Relatives::Successors => (&a.linear.end_binding, &a.linear.start_binding),
        };
        let Some(opposite) = opposite else { continue };
        if own.as_ref().map(|b| b.element_id.as_str()) != Some(node.base.id.as_str()) {
            continue;
        }
        let Some(relative) = scene.get_non_deleted(&opposite.element_id) else {
            continue;
        };
        let edge = match kind {
            Relatives::Predecessors => a.linear.points.last().copied().unwrap_or([0.0, 0.0]),
            Relatives::Successors => [0.0, 0.0],
        };
        let heading =
            heading_for_point_from_element(node, aabb, [edge[0] + el.base.x, edge[1] + el.base.y]);
        if heading == want {
            out.push(relative);
        }
    }
    out
}

fn linked_nodes<'a>(
    node: &Element,
    scene: &'a Scene,
    direction: LinkDirection,
) -> Vec<&'a Element> {
    let mut nodes = get_node_relatives(Relatives::Successors, node, scene, direction);
    nodes.extend(get_node_relatives(
        Relatives::Predecessors,
        node,
        scene,
        direction,
    ));
    nodes
}

/// `FlowChartNavigator`: the Alt+Arrow walk.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlowChartNavigator {
    pub is_exploring: bool,
    /// The nodes one link away (successors and predecessors).
    same_level_nodes: Vec<String>,
    same_level_index: usize,
    direction: Option<LinkDirection>,
    visited_nodes: HashSet<String>,
}

impl FlowChartNavigator {
    /// `clear()`.
    pub fn clear(&mut self) {
        *self = FlowChartNavigator::default();
    }

    /// `exploreByDirection(element, elementsMap, direction)`: the node to
    /// go to from `element_id`, if any. Pressing on in the same direction
    /// cycles through the nodes at the same level; with none in that
    /// direction, it goes to a linked node not visited yet.
    pub fn explore_by_direction(
        &mut self,
        element_id: &str,
        scene: &Scene,
        direction: LinkDirection,
    ) -> Option<String> {
        let element = scene.get_non_deleted(element_id)?;
        if !element.is_bindable() {
            return None;
        }
        if Some(direction) != self.direction {
            self.clear();
        }
        self.visited_nodes.insert(element.base.id.clone());

        if self.is_exploring && Some(direction) == self.direction && self.same_level_nodes.len() > 1
        {
            self.same_level_index = (self.same_level_index + 1) % self.same_level_nodes.len();
            return Some(self.same_level_nodes[self.same_level_index].clone());
        }

        let nodes = linked_nodes(element, scene, direction);
        if let Some(first) = nodes.first() {
            let first = first.base.id.clone();
            self.same_level_index = 0;
            self.is_exploring = true;
            self.same_level_nodes = nodes.iter().map(|n| n.base.id.clone()).collect();
            self.direction = Some(direction);
            self.visited_nodes.insert(first.clone());
            return Some(first);
        }

        if Some(direction) == self.direction || !self.is_exploring {
            if !self.is_exploring {
                self.visited_nodes.insert(element.base.id.clone());
            }
            let others: Vec<String> = [
                LinkDirection::Up,
                LinkDirection::Right,
                LinkDirection::Down,
                LinkDirection::Left,
            ]
            .into_iter()
            .filter(|&d| d != direction)
            .flat_map(|d| linked_nodes(element, scene, d))
            .map(|n| n.base.id.clone())
            .filter(|id| !self.visited_nodes.contains(id))
            .collect();
            for linked in others {
                if !self.visited_nodes.contains(&linked) {
                    self.visited_nodes.insert(linked.clone());
                    self.is_exploring = true;
                    self.direction = Some(direction);
                    return Some(linked);
                }
            }
        }
        None
    }
}

/// `isNodeInFlowchart(element, elementsMap)`: an arrow is bound to it.
pub fn is_node_in_flowchart(element: &Element, scene: &Scene) -> bool {
    scene.non_deleted().into_iter().any(|el| {
        if !is_arrow(el) {
            return false;
        }
        let (start, end) = bindings(el);
        start == Some(element.base.id.as_str()) || end == Some(element.base.id.as_str())
    })
}

/// What App does after a flowchart key (`FlowchartOperation`,
/// `App.flowchart.ts:22-28`).
#[derive(Debug, Clone, PartialEq)]
pub enum FlowchartOperation {
    /// Escape: the pending nodes are gone (`triggerRender`).
    Canceled,
    /// Ctrl+Arrow: `revealIfHidden(pending)` when there are any.
    Creating { pending: Vec<Element> },
    /// Alt+Arrow: select and reveal the node, if found.
    Navigating { node_id: Option<String> },
    /// Ctrl released: `insertNewElements(nodes)`, select and reveal the
    /// first, capture.
    Committed { nodes: Vec<Element> },
    /// Alt released: capture.
    NavigationEnded,
}

/// `AppFlowchart`: the creator and the navigator App keeps, answering the
/// keyboard's flowchart effects.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppFlowchart {
    pub creator: FlowChartCreator,
    pub navigator: FlowChartNavigator,
}

impl AppFlowchart {
    /// `appFlowchart.pendingNodes`: what the static scene draws
    /// (`pendingFlowchartNodes`).
    pub fn pending_nodes(&self) -> &[Element] {
        self.creator.pending_nodes.as_deref().unwrap_or(&[])
    }

    /// The operation for one effect of [`crate::keyboard`]'s handlers
    /// (`resolveKeyboardEventToOperation`), `None` for other effects. After
    /// Ctrl+Arrow and Alt+Arrow the keyboard's flags follow the creator and
    /// the navigator; the keyup effects end their sessions (call
    /// [`AppFlowchart::after_key_up`] too, for a walk that ended on the
    /// keyup that committed).
    pub fn answer<E: EditEnv>(
        &mut self,
        effect: &KeyEffect,
        scene: &mut Scene,
        app_state: &AppState,
        keys: &mut FlowchartKeys,
        env: &mut E,
    ) -> Option<FlowchartOperation> {
        let op = match effect {
            KeyEffect::FlowchartCanceled => {
                self.creator.clear();
                return Some(FlowchartOperation::Canceled);
            }
            KeyEffect::FlowchartCreate { start, direction } => {
                if let Some(start) = start {
                    let app = FlowchartAppState::from_app_state(app_state);
                    self.creator
                        .create_nodes(start, &app, *direction, scene, env);
                }
                FlowchartOperation::Creating {
                    pending: self.pending_nodes().to_vec(),
                }
            }
            KeyEffect::FlowchartNavigate { from, direction } => {
                let node_id = self.navigator.explore_by_direction(from, scene, *direction);
                FlowchartOperation::Navigating {
                    node_id: node_id.filter(|id| scene.get_non_deleted(id).is_some()),
                }
            }
            KeyEffect::FlowchartCommit => {
                let nodes = self.creator.pending_nodes.take().unwrap_or_default();
                self.creator.clear();
                return Some(FlowchartOperation::Committed { nodes });
            }
            KeyEffect::FlowchartNavigationEnded => {
                self.navigator.clear();
                return Some(FlowchartOperation::NavigationEnded);
            }
            _ => return None,
        };
        keys.is_creating_chart = self.creator.is_creating_chart;
        keys.is_exploring = self.navigator.is_exploring;
        Some(op)
    }

    /// After a keyup: releasing Alt ended the walk
    /// (`navigator.clear()`, `App.flowchart.ts:153-156`), which the
    /// keyboard noted by lowering its flag.
    pub fn after_key_up(&mut self, keys: &FlowchartKeys) {
        if self.navigator.is_exploring && !keys.is_exploring {
            self.navigator.clear();
        }
        if self.creator.is_creating_chart && !keys.is_creating_chart {
            // the commit took the pending nodes already
            self.creator.clear();
        }
    }

    /// `clear()`: ends any session.
    pub fn clear(&mut self) {
        self.creator.clear();
        self.navigator.clear();
    }
}

/// `App.insertNewElements(elements)` (`App.tsx:7922-7950`): the elements
/// in runs of the same frame, each run with where it goes in `elements`
/// (above the frame's last child, or the end for `None`), in order. Insert
/// each run before computing the next index.
pub fn insertion_runs(nodes: Vec<Element>) -> Vec<Vec<Element>> {
    let mut runs: Vec<Vec<Element>> = Vec::new();
    for node in nodes {
        match runs.last_mut() {
            Some(run) if run[0].base.frame_id == node.base.frame_id => run.push(node),
            _ => runs.push(vec![node]),
        }
    }
    runs
}

/// `getFrameChildrenInsertionIndex(elements, frameId)` for a run of
/// [`insertion_runs`], `None` (the end) outside a frame.
pub fn insertion_index(elements: &[Element], run: &[Element]) -> Option<usize> {
    let frame = run.first()?.base.frame_id.as_deref()?;
    let all: Vec<&Element> = elements.iter().collect();
    get_frame_children_insertion_index(&all, frame)
}
