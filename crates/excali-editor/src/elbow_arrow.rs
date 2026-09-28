//! Elbow arrow routing: `updateElbowArrowPoints`
//! (`packages/element/src/elbowArrow.ts:907-1167`) and everything it calls,
//! the A* search over a non-uniform grid included.
//!
//! An elbow arrow's points are recomputed whenever its ends, its fixed
//! segments or its bindings change. With no fixed segments the route is
//! found from scratch:
//!
//! 1. Each end gets a global point (a bound end sits on its target's fixed
//!    point), a heading (the side of the target it leaves from, or towards
//!    the other end) and a box: the target's box grown by the binding gap
//!    (six gaps with an arrowhead, two without) on the heading side, or a
//!    4 x 4 box around a free end.
//! 2. Two "dynamic" boxes that always touch are grown from those
//!    (`generateDynamicAABBs`), padded by `BASE_PADDING` (40) less the gap
//!    on each end's heading side, and each end gets a dongle: the point
//!    where its heading leaves its dynamic box.
//! 3. The grid is every intersection of the boxes' edges, the dongles' and
//!    the common box's lines (`calculateGrid`), and A* runs from dongle to
//!    dongle (`astar`): no step may cross a box or turn back, each bend
//!    costs `bendMultiplier^3` on g, where `bendMultiplier` is the
//!    Manhattan distance between the dongles, and the heuristic adds
//!    `estBends * bendMultiplier^2` to the Manhattan distance left.
//! 4. The path gets its true ends back, loses segments shorter than 1 and
//!    collinear points, and is made relative to its first point
//!    (`normalizeArrowElementUpdate`, coordinates clamped to 1e6).
//!
//! With fixed segments the update is a renormalisation, a segment move, a
//! segment release or an endpoint drag instead (`handleSegment*`,
//! `handleEndpointDrag`).
//!
//! Upstream's development-build invariants (`!import.meta.env.PROD`,
//! `isDevEnv()`) are not checked, as in the production build. Where
//! upstream would throw (a route that cannot be found, points missing
//! where an endpoint drag needs them), [`update_elbow_arrow_points`]
//! answers an [`ElbowArrowError`] with upstream's message or the
//! `TypeError` a missing point would raise.
//!
//! The drag-time options (`isDragging`, `isBindingEnabled`,
//! `isMidpointSnappingEnabled`: the hovered element found by hit testing
//! and the end snapped to its outline, `collision.ts` and `binding.ts`) are
//! the binding tasks' and are not taken here; this is upstream's call
//! without `options`, which is what `mutateElement` passes outside a drag
//! and what `restoreElements` passes.

use std::collections::HashMap;
use std::fmt;

use excali_core::element::{
    ArrowFields, BindMode, Element, ElementKind, FixedPointBinding, FixedSegment, LocalPoint,
};
use excali_math::{js, point_distance, point_scale_from_origin, points_equal, vector_cross, Point};
use excali_scene::elbow_arrow::{validate_elbow_points, DEDUP_TRESHOLD};
use excali_scene::heading::{
    heading_for_point, heading_for_point_is_horizontal, heading_is_horizontal, vector_to_heading,
    Heading,
};
use serde_json::{Map, Value};

use crate::binary_heap::BinaryHeap;
use crate::geometry::{
    aabb_for_element, get_binding_gap, get_global_fixed_point_for_bindable_element,
    get_heading_for_elbow_arrow_snap, point_inside_bounds, Bounds, BASE_BINDING_GAP,
};

/// `BASE_PADDING` (`elbowArrow.ts:111`): how far a route keeps clear of
/// the elements it connects.
pub const BASE_PADDING: f64 = 40.0;

/// `MAX_POS` (`elbowArrow.ts:902`): coordinates are clamped to +-1e6.
const MAX_POS: f64 = 1e6;

type P = [f64; 2];

// -- the scene ----------------------------------------------------------------

/// `elementsMap` (`NonDeletedSceneElementsMap`): the scene's elements by
/// id, where a bound end finds its target. As with `arrayToMap`, a later
/// element with the same id replaces an earlier one.
#[derive(Debug, Clone, Default)]
pub struct ElementsMap<'a> {
    by_id: HashMap<&'a str, &'a Element>,
    size: usize,
}

impl<'a> ElementsMap<'a> {
    /// `arrayToMap(elements)`.
    pub fn new(elements: impl IntoIterator<Item = &'a Element>) -> ElementsMap<'a> {
        let mut by_id = HashMap::new();
        for element in elements {
            by_id.insert(element.base.id.as_str(), element);
        }
        let size = by_id.len();
        ElementsMap { by_id, size }
    }

    /// The map with `size` entries, of which only the given elements can be
    /// looked up (restore hands over elements the typed model cannot
    /// read; they still count towards `elementsMap.size`).
    pub(crate) fn with_size(mut self, size: usize) -> ElementsMap<'a> {
        self.size = size;
        self
    }

    /// `elementsMap.get(id)`.
    pub fn get(&self, id: &str) -> Option<&'a Element> {
        self.by_id.get(id).copied()
    }

    /// `elementsMap.size`.
    pub fn len(&self) -> usize {
        self.size
    }

    /// `elementsMap.size === 0`.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// `getBindableElementForId(id, elementsMap)` (`elbowArrow.ts:2090-2100`).
    fn bindable(&self, id: &str) -> Option<&'a Element> {
        self.get(id).filter(|e| e.is_bindable())
    }
}

// -- updates in and out -----------------------------------------------------

/// `updateElbowArrowPoints`' third argument. Each `Option` is whether the
/// key is present; an inner `None` is `null`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElbowArrowUpdates {
    /// The new points: all of them, or just the new first and last.
    pub points: Option<Vec<LocalPoint>>,
    pub fixed_segments: Option<Option<Vec<FixedSegment>>>,
    pub start_binding: Option<Option<FixedPointBinding>>,
    pub end_binding: Option<Option<FixedPointBinding>>,
    /// The update carries other keys too (`x`, `y`, ...): upstream counts
    /// them (`Object.keys(restOfTheUpdates)`, `elbowArrow.ts:1020`).
    pub other_keys: bool,
}

/// The `ElementUpdate` `updateElbowArrowPoints` returns: the keys to
/// assign to the arrow. `None` is a key upstream leaves out or holds
/// `undefined` (which `JSON.stringify` and `Object.assign`-then-serialise
/// drop); an inner `None` is `null`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElbowArrowUpdate {
    pub points: Option<Vec<LocalPoint>>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub fixed_segments: Option<Option<Vec<FixedSegment>>>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub start_is_special: Option<Option<bool>>,
    pub end_is_special: Option<Option<bool>>,
    /// Only when the updates are handed back (a resize, `elbowArrow.ts:1145-1147`).
    pub start_binding: Option<Option<FixedPointBinding>>,
    pub end_binding: Option<Option<FixedPointBinding>>,
}

/// A number as `JSON.stringify` writes it: integers without a fraction,
/// `-0` as `0`, and non-finite numbers as `null`.
fn number(x: f64) -> Value {
    if !x.is_finite() {
        return Value::Null;
    }
    if x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
        return Value::from(x as i64);
    }
    serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number)
}

fn point_value(p: P) -> Value {
    Value::Array(vec![number(p[0]), number(p[1])])
}

fn segments_value(segments: &Option<Vec<FixedSegment>>) -> Value {
    match segments {
        None => Value::Null,
        Some(segments) => Value::Array(
            segments
                .iter()
                .map(|s| {
                    let mut m = Map::new();
                    m.insert("start".into(), point_value(s.start));
                    m.insert("end".into(), point_value(s.end));
                    m.insert("index".into(), number(s.index));
                    Value::Object(m)
                })
                .collect(),
        ),
    }
}

fn binding_value(binding: &Option<FixedPointBinding>) -> Value {
    match binding {
        None => Value::Null,
        Some(b) => {
            let mut m = Map::new();
            m.insert("elementId".into(), Value::String(b.element_id.clone()));
            m.insert("fixedPoint".into(), point_value(b.fixed_point));
            m.insert(
                "mode".into(),
                Value::String(
                    match b.mode {
                        BindMode::Inside => "inside",
                        BindMode::Orbit => "orbit",
                        BindMode::Skip => "skip",
                    }
                    .into(),
                ),
            );
            Value::Object(m)
        }
    }
}

impl ElbowArrowUpdate {
    /// The update as the JSON object upstream returns, keys in the order
    /// `normalizeArrowElementUpdate` creates them.
    pub fn to_map(&self) -> Map<String, Value> {
        let mut m = Map::new();
        if let Some(points) = &self.points {
            m.insert(
                "points".into(),
                Value::Array(points.iter().map(|&p| point_value(p)).collect()),
            );
        }
        if let Some(x) = self.x {
            m.insert("x".into(), number(x));
        }
        if let Some(y) = self.y {
            m.insert("y".into(), number(y));
        }
        if let Some(segments) = &self.fixed_segments {
            m.insert("fixedSegments".into(), segments_value(segments));
        }
        if let Some(width) = self.width {
            m.insert("width".into(), number(width));
        }
        if let Some(height) = self.height {
            m.insert("height".into(), number(height));
        }
        let flag = |v: &Option<bool>| v.map_or(Value::Null, Value::Bool);
        if let Some(special) = &self.start_is_special {
            m.insert("startIsSpecial".into(), flag(special));
        }
        if let Some(special) = &self.end_is_special {
            m.insert("endIsSpecial".into(), flag(special));
        }
        if let Some(binding) = &self.start_binding {
            m.insert("startBinding".into(), binding_value(binding));
        }
        if let Some(binding) = &self.end_binding {
            m.insert("endBinding".into(), binding_value(binding));
        }
        m
    }
}

/// Where upstream throws instead of returning an update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElbowArrowError(pub String);

impl fmt::Display for ElbowArrowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ElbowArrowError {}

type Result<T> = std::result::Result<T, ElbowArrowError>;

/// The `TypeError` reading a coordinate of a missing point raises.
fn missing_point() -> ElbowArrowError {
    ElbowArrowError("Cannot read properties of undefined (reading '0')".into())
}

/// `array[i]`: a missing point is upstream's `TypeError`.
fn at(points: &[P], i: isize) -> Result<P> {
    usize::try_from(i)
        .ok()
        .and_then(|i| points.get(i))
        .copied()
        .ok_or_else(missing_point)
}

/// `array.at(i)`: negative indices count from the end.
fn at_relative(points: &[P], i: isize) -> Option<P> {
    let len = points.len() as isize;
    let i = if i < 0 { len + i } else { i };
    usize::try_from(i).ok().and_then(|i| points.get(i)).copied()
}

fn index_of(segments: &[FixedSegment], index: f64) -> Option<usize> {
    segments.iter().position(|s| s.index == index)
}

fn as_index(index: f64) -> isize {
    // fixed segment indices are integers; a fraction reads no point
    if index.fract() == 0.0 && index.is_finite() {
        index as isize
    } else {
        isize::MIN
    }
}

fn arrow_fields(arrow: &Element) -> Result<&ArrowFields> {
    match &arrow.kind {
        ElementKind::Arrow(fields) => Ok(fields),
        _ => Err(ElbowArrowError(format!(
            "updateElbowArrowPoints: {} is not an arrow",
            arrow.base.id
        ))),
    }
}

/// `arrow.fixedSegments` as a list, when it is one (`null` and absent are
/// not).
fn fixed_segments_of(fields: &ArrowFields) -> Option<&Vec<FixedSegment>> {
    fields.fixed_segments.as_ref().and_then(Option::as_ref)
}

// -- updateElbowArrowPoints -----------------------------------------------------

/// `updateElbowArrowPoints(arrow, elementsMap, updates)`
/// (`elbowArrow.ts:907-1167`), with no options: the keys to assign to
/// `arrow`, an elbow arrow, for `updates`.
pub fn update_elbow_arrow_points(
    arrow: &Element,
    elements_map: &ElementsMap<'_>,
    updates: &ElbowArrowUpdates,
) -> Result<ElbowArrowUpdate> {
    let fields = arrow_fields(arrow)?;
    let points = &fields.linear.points;
    let (x, y) = (arrow.base.x, arrow.base.y);

    if points.len() < 2 {
        return Ok(ElbowArrowUpdate {
            points: Some(updates.points.clone().unwrap_or_else(|| points.clone())),
            ..ElbowArrowUpdate::default()
        });
    }

    // updates.fixedSegments ?? arrow.fixedSegments ?? []
    let fixed_segments: Vec<FixedSegment> = match &updates.fixed_segments {
        Some(Some(segments)) => segments.clone(),
        _ => fixed_segments_of(fields).cloned().unwrap_or_default(),
    };

    let updated_points: Vec<P> = match &updates.points {
        Some(new) if new.len() == 2 => points
            .iter()
            .enumerate()
            .map(|(idx, &p)| {
                if idx == 0 {
                    new[0]
                } else if idx == points.len() - 1 {
                    new[1]
                } else {
                    p
                }
            })
            .collect(),
        Some(new) => new.clone(),
        None => points.clone(),
    };

    let start_binding = match &updates.start_binding {
        Some(b) => b.clone(),
        None => fields.linear.start_binding.clone(),
    };
    let end_binding = match &updates.end_binding {
        Some(b) => b.clone(),
        None => fields.linear.end_binding.clone(),
    };
    let start_element = start_binding
        .as_ref()
        .and_then(|b| elements_map.bindable(&b.element_id));
    let end_element = end_binding
        .as_ref()
        .and_then(|b| elements_map.bindable(&b.element_id));
    let are_updated_points_valid = validate_elbow_points(&updated_points);

    let rest_of_the_updates_is_empty =
        updates.points.is_none() && updates.fixed_segments.is_none() && !updates.other_keys;
    fn id_of(e: Option<&Element>) -> Option<&str> {
        e.map(|e| e.base.id.as_str())
    }
    fn binding_id(b: &Option<FixedPointBinding>) -> Option<&str> {
        b.as_ref().map(|b| b.element_id.as_str())
    }
    if (start_binding.is_some() && start_element.is_none() && are_updated_points_valid)
        || (end_binding.is_some() && end_element.is_none() && are_updated_points_valid)
        || (elements_map.is_empty() && are_updated_points_valid)
        || (rest_of_the_updates_is_empty
            && (id_of(start_element) != binding_id(&start_binding)
                || id_of(end_element) != binding_id(&end_binding)))
    {
        return normalize_arrow_element_update(
            &updated_points
                .iter()
                .map(|p| [x + p[0], y + p[1]])
                .collect::<Vec<_>>(),
            SegmentsArg::Arrow(&fields.fixed_segments),
            fields.start_is_special,
            fields.end_is_special,
        );
    }

    let state = ElbowArrowState {
        x,
        y,
        start_binding: start_binding.clone(),
        end_binding: end_binding.clone(),
        start_arrowhead: fields.linear.start_arrowhead.is_some(),
        end_arrowhead: fields.linear.end_arrowhead.is_some(),
    };
    let data = get_elbow_arrow_data(&state, elements_map, &updated_points);

    // 1. Renormalise the arrow
    let falsy_segments = !matches!(updates.fixed_segments, Some(Some(_)));
    let falsy_binding = |b: &Option<Option<FixedPointBinding>>| !matches!(b, Some(Some(_)));
    if updates.points.is_none()
        && falsy_segments
        && falsy_binding(&updates.start_binding)
        && falsy_binding(&updates.end_binding)
    {
        return handle_segment_renormalization(arrow, fields, elements_map);
    }

    // Short circuit on no-op. Upstream compares the bindings by reference,
    // which holds when the caller passes the arrow's own; equal bindings
    // stand for that here.
    let same_binding = |update: &Option<Option<FixedPointBinding>>,
                        own: &Option<FixedPointBinding>| {
        matches!(update, Some(b) if b == own)
    };
    let points_unchanged = updates.points.as_ref().is_none_or(|new| {
        new.iter().enumerate().all(|(i, &p)| {
            points
                .get(i)
                .is_some_and(|&q| points_equal(Point::<excali_math::Local>::from(p), q.into()))
        })
    });
    if same_binding(&updates.start_binding, &fields.linear.start_binding)
        && same_binding(&updates.end_binding, &fields.linear.end_binding)
        && points_unchanged
        && are_updated_points_valid
    {
        return Ok(ElbowArrowUpdate::default());
    }

    // 2. Just normal elbow arrow things
    if fixed_segments.is_empty() {
        let route = route_elbow_arrow(fields.linear.start_binding.is_some(), &data)?;
        return normalize_arrow_element_update(
            &get_elbow_arrow_corner_points(remove_elbow_arrow_short_segments(
                route.unwrap_or_default(),
            )),
            SegmentsArg::List(&fixed_segments),
            Some(None),
            Some(None),
        );
    }

    // 3. Releasing a fixed segment
    if fixed_segments_of(fields).map_or(0, Vec::len) > fixed_segments.len() {
        return handle_segment_release(arrow, fields, &fixed_segments, elements_map);
    }

    // 4. Manual segment move
    if updates.points.is_none() {
        return handle_segment_move(
            arrow,
            fields,
            fixed_segments,
            data.start_heading,
            data.end_heading,
            data.hovered_start_element.is_some(),
            data.hovered_end_element.is_some(),
        );
    }

    // 5. Resize: the updates are the answer
    if matches!(updates.fixed_segments, Some(Some(_))) {
        return Ok(ElbowArrowUpdate {
            points: updates.points.clone(),
            fixed_segments: updates.fixed_segments.clone(),
            start_binding: updates.start_binding.clone(),
            end_binding: updates.end_binding.clone(),
            ..ElbowArrowUpdate::default()
        });
    }

    // 6. One or more segments are fixed and endpoints are moved
    handle_endpoint_drag(arrow, fields, &updated_points, &fixed_segments, &data)
}

// -- the four fixed-segment cases ------------------------------------------------

/// `handleSegmentRenormalization(arrow, elementsMap)`
/// (`elbowArrow.ts:113-280`): merges collinear neighbours and drops
/// segments shorter than 1, keeping the fixed segments' indices in step; a
/// fixed first or last segment is let go, and with no fixed segment left
/// the arrow is routed afresh.
fn handle_segment_renormalization(
    arrow: &Element,
    fields: &ArrowFields,
    elements_map: &ElementsMap<'_>,
) -> Result<ElbowArrowUpdate> {
    let (x, y) = (arrow.base.x, arrow.base.y);
    let Some(own_segments) = fixed_segments_of(fields) else {
        return Ok(ElbowArrowUpdate {
            x: Some(x),
            y: Some(y),
            points: Some(fields.linear.points.clone()),
            fixed_segments: fields.fixed_segments.clone(),
            start_is_special: fields.start_is_special,
            end_is_special: fields.end_is_special,
            ..ElbowArrowUpdate::default()
        });
    };
    let mut next_fixed_segments = own_segments.clone();
    let points: Vec<P> = fields
        .linear
        .points
        .iter()
        .map(|p| [x + p[0], y + p[1]])
        .collect();

    let mut next_points_1: Vec<P> = Vec::new();
    for (i, &p) in points.iter().enumerate() {
        if i < 2 {
            next_points_1.push(p);
            continue;
        }
        let current_segment_heading = heading_for_point(p, points[i - 1]);
        let prev_segment_heading = heading_for_point(points[i - 1], points[i - 2]);
        if current_segment_heading == prev_segment_heading {
            let prev_segment_idx = index_of(&next_fixed_segments, (i - 1) as f64);
            let segment_idx = index_of(&next_fixed_segments, i as f64);
            if let Some(s) = segment_idx {
                next_fixed_segments[s].start = [points[i - 2][0] - x, points[i - 2][1] - y];
            }
            if let Some(s) = prev_segment_idx {
                next_fixed_segments.remove(s);
            }
            next_points_1.pop();
            for segment in &mut next_fixed_segments {
                if segment.index > (i - 1) as f64 {
                    segment.index -= 1.0;
                }
            }
        }
        next_points_1.push(p);
    }

    let mut next_points: Vec<P> = Vec::new();
    for (i, &p) in next_points_1.iter().enumerate() {
        if i < 3 {
            next_points.push(p);
            continue;
        }
        if js_point_distance(next_points_1[i - 2], next_points_1[i - 1]) < DEDUP_TRESHOLD {
            let prev_prev_segment_idx = index_of(&next_fixed_segments, (i - 2) as f64);
            let prev_segment_idx = index_of(&next_fixed_segments, (i - 1) as f64);
            // spliced one after the other with the positions found first
            if let Some(s) = prev_segment_idx {
                splice_one(&mut next_fixed_segments, s);
            }
            if let Some(s) = prev_prev_segment_idx {
                splice_one(&mut next_fixed_segments, s);
            }
            // nextPoints.splice(-2, 2)
            let keep = next_points.len().saturating_sub(2);
            next_points.truncate(keep);
            for segment in &mut next_fixed_segments {
                if segment.index > (i - 2) as f64 {
                    segment.index -= 2.0;
                }
            }
            let is_horizontal = heading_for_point_is_horizontal(p, next_points_1[i - 1]);
            next_points.push([
                if !is_horizontal {
                    next_points_1[i - 2][0]
                } else {
                    p[0]
                },
                if is_horizontal {
                    next_points_1[i - 2][1]
                } else {
                    p[1]
                },
            ]);
            continue;
        }
        next_points.push(p);
    }

    let last_index = next_points.len() as f64 - 1.0;
    let filtered: Vec<FixedSegment> = next_fixed_segments
        .into_iter()
        .filter(|s| s.index != 1.0 && s.index != last_index)
        .collect();
    if filtered.is_empty() {
        let state = ElbowArrowState::of(arrow, fields);
        let local: Vec<P> = next_points.iter().map(|p| [p[0] - x, p[1] - y]).collect();
        let data = get_elbow_arrow_data(&state, elements_map, &local);
        let route = route_elbow_arrow(fields.linear.start_binding.is_some(), &data)?;
        return normalize_arrow_element_update(
            &get_elbow_arrow_corner_points(remove_elbow_arrow_short_segments(
                route.unwrap_or_default(),
            )),
            SegmentsArg::List(&filtered),
            Some(None),
            Some(None),
        );
    }
    normalize_arrow_element_update(
        &next_points,
        SegmentsArg::List(&filtered),
        fields.start_is_special,
        fields.end_is_special,
    )
}

/// `array.splice(i, 1)`: removes the item at `i`, if there is one.
fn splice_one<T>(items: &mut Vec<T>, i: usize) {
    if i < items.len() {
        items.remove(i);
    }
}

/// `handleSegmentRelease(arrow, fixedSegments, elementsMap)`
/// (`elbowArrow.ts:282-460`): the stretch between the fixed segments
/// either side of the released one is routed again and spliced in.
fn handle_segment_release(
    arrow: &Element,
    fields: &ArrowFields,
    fixed_segments: &[FixedSegment],
    elements_map: &ElementsMap<'_>,
) -> Result<ElbowArrowUpdate> {
    let (ax, ay) = (arrow.base.x, arrow.base.y);
    let points = &fields.linear.points;
    let new_indices: Vec<f64> = fixed_segments.iter().map(|s| s.index).collect();
    let own = fixed_segments_of(fields).cloned().unwrap_or_default();
    let deleted_segment_idx = own.iter().position(|s| !new_indices.contains(&s.index));
    let Some(deleted_segment_idx) = deleted_segment_idx else {
        return Ok(ElbowArrowUpdate {
            points: Some(points.clone()),
            ..ElbowArrowUpdate::default()
        });
    };
    let deleted_idx = own[deleted_segment_idx].index;
    let prev_segment = deleted_segment_idx
        .checked_sub(1)
        .and_then(|i| own.get(i))
        .cloned();
    let next_segment = own.get(deleted_segment_idx + 1).cloned();

    let x = ax + prev_segment.as_ref().map_or(0.0, |s| s.end[0]);
    let y = ay + prev_segment.as_ref().map_or(0.0, |s| s.end[1]);
    let last = *points.last().ok_or_else(missing_point)?;
    let state = ElbowArrowState {
        x,
        y,
        start_binding: if prev_segment.is_some() {
            None
        } else {
            fields.linear.start_binding.clone()
        },
        end_binding: if next_segment.is_some() {
            None
        } else {
            fields.linear.end_binding.clone()
        },
        start_arrowhead: false,
        end_arrowhead: false,
    };
    let end = [
        ax + next_segment.as_ref().map_or(last[0], |s| s.start[0]) - x,
        ay + next_segment.as_ref().map_or(last[1], |s| s.start[1]) - y,
    ];
    let data = get_elbow_arrow_data(&state, elements_map, &[[0.0, 0.0], end]);
    let route = route_elbow_arrow(fields.linear.start_binding.is_some(), &data)?;
    let restored = normalize_arrow_element_update(
        &get_elbow_arrow_corner_points(remove_elbow_arrow_short_segments(
            route.unwrap_or_default(),
        )),
        SegmentsArg::List(fixed_segments),
        Some(None),
        Some(None),
    )?;
    let restored_points = restored.points.unwrap_or_default();
    if restored_points.len() < 2 {
        return Err(ElbowArrowError(
            "Property 'points' is required in the update returned by normalizeArrowElementUpdate()"
                .into(),
        ));
    }

    let mut next_points: Vec<P> = Vec::new();
    if let Some(prev) = &prev_segment {
        for i in 0..as_index(prev.index).max(0) {
            let p = at(points, i)?;
            next_points.push([ax + p[0], ay + p[1]]);
        }
    }
    for p in &restored_points {
        next_points.push([
            ax + prev_segment.as_ref().map_or(0.0, |s| s.end[0]) + p[0],
            ay + prev_segment.as_ref().map_or(0.0, |s| s.end[1]) + p[1],
        ]);
    }
    if let Some(next) = &next_segment {
        for i in as_index(next.index)..points.len() as isize {
            let p = at(points, i)?;
            next_points.push([ax + p[0], ay + p[1]]);
        }
    }

    let original_segment_count_diff = next_segment
        .as_ref()
        .map_or(points.len() as f64, |s| s.index)
        - prev_segment.as_ref().map_or(0.0, |s| s.index)
        - 1.0;
    let mut next_fixed_segments: Vec<FixedSegment> = fixed_segments
        .iter()
        .map(|s| {
            let mut s = s.clone();
            if s.index > deleted_idx {
                s.index =
                    s.index - original_segment_count_diff + (restored_points.len() as f64 - 1.0);
            }
            s
        })
        .collect();

    let mut simplified: Vec<P> = Vec::new();
    for (i, &p) in next_points.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| next_points[j]);
        let next = next_points.get(i + 1).copied();
        if let (Some(prev), Some(next)) = (prev, next) {
            let prev_heading = heading_for_point(p, prev);
            let next_heading = heading_for_point(next, p);
            if prev_heading == next_heading {
                for segment in &mut next_fixed_segments {
                    if segment.index > i as f64 {
                        segment.index -= 1.0;
                    }
                }
                continue;
            } else if prev_heading == flip_heading(next_heading) {
                for segment in &mut next_fixed_segments {
                    if segment.index > i as f64 {
                        segment.index += 1.0;
                    }
                }
                simplified.push(p);
                simplified.push(p);
                continue;
            }
        }
        simplified.push(p);
    }

    normalize_arrow_element_update(
        &simplified,
        SegmentsArg::List(&next_fixed_segments),
        Some(Some(false)),
        Some(Some(false)),
    )
}

/// `flipHeading(h)` (`heading.ts:282-286`).
fn flip_heading(h: Heading) -> Heading {
    match h {
        Heading::Right => Heading::Left,
        Heading::Left => Heading::Right,
        Heading::Up => Heading::Down,
        Heading::Down => Heading::Up,
    }
}

/// `handleSegmentMove(arrow, fixedSegments, startHeading, endHeading,
/// hoveredStartElement, hoveredEndElement)` (`elbowArrow.ts:465-704`): the
/// moved segment's points replace the old ones and drag their neighbours
/// along; moving the first or last segment adds the segments that keep the
/// arrow leaving its targets.
fn handle_segment_move(
    arrow: &Element,
    fields: &ArrowFields,
    mut fixed_segments: Vec<FixedSegment>,
    start_heading: Heading,
    end_heading: Heading,
    hovered_start: bool,
    hovered_end: bool,
) -> Result<ElbowArrowUpdate> {
    let (ax, ay) = (arrow.base.x, arrow.base.y);
    let points = &fields.linear.points;
    let own = fixed_segments_of(fields);
    let actively_modified_segment_idx = fixed_segments
        .iter()
        .enumerate()
        .find(|(i, segment)| {
            let Some(old) = own.and_then(|own| own.get(*i)) else {
                return true;
            };
            if old.index != segment.index {
                return true;
            }
            (segment.start[0] != old.start[0] && segment.end[0] != old.end[0])
                != (segment.start[1] != old.start[1] && segment.end[1] != old.end[1])
        })
        .map(|(i, _)| i);
    let Some(active) = actively_modified_segment_idx else {
        return Ok(ElbowArrowUpdate {
            points: Some(points.clone()),
            ..ElbowArrowUpdate::default()
        });
    };

    let last_point_index = points.len() as f64 - 1.0;
    let first_segment_idx = own.and_then(|own| index_of(own, 1.0));
    let last_segment_idx = own.and_then(|own| index_of(own, last_point_index));

    // Special case for a first or last segment move
    let segment_length =
        js_point_distance(fixed_segments[active].start, fixed_segments[active].end);
    let segment_is_too_short = segment_length < BASE_PADDING + 5.0;
    let padding = |positive: bool| match (positive, segment_is_too_short) {
        (true, true) => segment_length / 2.0,
        (true, false) => BASE_PADDING,
        (false, true) => -segment_length / 2.0,
        (false, false) => -BASE_PADDING,
    };
    if first_segment_idx.is_none() && fixed_segments[active].index == 1.0 && hovered_start {
        let start_is_horizontal = heading_is_horizontal(start_heading);
        let start_is_positive = if start_is_horizontal {
            start_heading == Heading::Right
        } else {
            start_heading == Heading::Down
        };
        let padding = padding(start_is_positive);
        let start = fixed_segments[active].start;
        fixed_segments[active].start = [
            start[0] + if start_is_horizontal { padding } else { 0.0 },
            start[1] + if !start_is_horizontal { padding } else { 0.0 },
        ];
    }
    if last_segment_idx.is_none() && fixed_segments[active].index == last_point_index && hovered_end
    {
        let end_is_horizontal = heading_is_horizontal(end_heading);
        let end_is_positive = if end_is_horizontal {
            end_heading == Heading::Right
        } else {
            end_heading == Heading::Down
        };
        let padding = padding(end_is_positive);
        let end = fixed_segments[active].end;
        fixed_segments[active].end = [
            end[0] + if end_is_horizontal { padding } else { 0.0 },
            end[1] + if !end_is_horizontal { padding } else { 0.0 },
        ];
    }

    // All fixed segments in global coordinates
    let mut next_fixed_segments: Vec<FixedSegment> = fixed_segments
        .iter()
        .map(|s| FixedSegment {
            start: [ax + s.start[0], ay + s.start[1]],
            end: [ax + s.end[0], ay + s.end[1]],
            index: s.index,
        })
        .collect();
    let mut new_points: Vec<P> = points.iter().map(|p| [ax + p[0], ay + p[1]]).collect();

    let start_idx = as_index(next_fixed_segments[active].index) - 1;
    let end_idx = as_index(next_fixed_segments[active].index);
    let start = next_fixed_segments[active].start;
    let end = next_fixed_segments[active].end;
    let get = |points: &[P], i: isize| usize::try_from(i).ok().and_then(|i| points.get(i)).copied();
    let prev_segment_is_horizontal = match get(&new_points, start_idx - 1) {
        Some(before) => {
            let at_start = at(&new_points, start_idx)?;
            (!points_equal(Point::<excali_math::Global>::from(at_start), before.into()))
                .then(|| heading_for_point_is_horizontal(before, at_start))
        }
        None => None,
    };
    let next_segment_is_horizontal = match get(&new_points, end_idx + 1) {
        Some(after) => {
            let at_end = at(&new_points, end_idx)?;
            (!points_equal(Point::<excali_math::Global>::from(at_end), after.into()))
                .then(|| heading_for_point_is_horizontal(after, at_end))
        }
        None => None,
    };

    // The moved segment's points override the old ones
    if let Some(horizontal) = prev_segment_is_horizontal {
        let dir = usize::from(horizontal);
        new_points[(start_idx - 1) as usize][dir] = start[dir];
    }
    set_point(&mut new_points, start_idx, start)?;
    set_point(&mut new_points, end_idx, end)?;
    if let Some(horizontal) = next_segment_is_horizontal {
        let dir = usize::from(horizontal);
        new_points[(end_idx + 1) as usize][dir] = end[dir];
    }

    // Neighbouring fixed segments follow
    if let Some(prev) = index_of(&next_fixed_segments, start_idx as f64) {
        let s = &mut next_fixed_segments[prev];
        let dir = usize::from(heading_for_point_is_horizontal(s.end, s.start));
        s.start[dir] = start[dir];
        s.end = start;
    }
    if let Some(next) = index_of(&next_fixed_segments, (end_idx + 1) as f64) {
        let s = &mut next_fixed_segments[next];
        let dir = usize::from(heading_for_point_is_horizontal(s.end, s.start));
        s.end[dir] = end[dir];
        s.start = end;
    }

    // A first segment move needs an additional segment
    if first_segment_idx.is_none() && start_idx == 0 {
        let first = at(points, 0)?;
        let start_is_horizontal = if hovered_start {
            heading_is_horizontal(start_heading)
        } else {
            heading_for_point_is_horizontal(at(&new_points, 1)?, at(&new_points, 0)?)
        };
        new_points.insert(
            0,
            [
                if start_is_horizontal {
                    start[0]
                } else {
                    ax + first[0]
                },
                if !start_is_horizontal {
                    start[1]
                } else {
                    ay + first[1]
                },
            ],
        );
        if hovered_start {
            new_points.insert(0, [ax + first[0], ay + first[1]]);
        }
        for segment in &mut next_fixed_segments {
            segment.index += if hovered_start { 2.0 } else { 1.0 };
        }
    }

    // A last segment move needs an additional segment
    if last_segment_idx.is_none() && end_idx == points.len() as isize - 1 {
        let last = *points.last().ok_or_else(missing_point)?;
        let end_is_horizontal = heading_is_horizontal(end_heading);
        new_points.push([
            if end_is_horizontal {
                end[0]
            } else {
                ax + last[0]
            },
            if !end_is_horizontal {
                end[1]
            } else {
                ay + last[1]
            },
        ]);
        if hovered_end {
            new_points.push([ax + last[0], ay + last[1]]);
        }
    }

    let local: Vec<FixedSegment> = next_fixed_segments
        .into_iter()
        .map(|s| FixedSegment {
            start: [s.start[0] - ax, s.start[1] - ay],
            end: [s.end[0] - ax, s.end[1] - ay],
            index: s.index,
        })
        .collect();
    normalize_arrow_element_update(
        &new_points,
        SegmentsArg::List(&local),
        Some(Some(false)),
        Some(Some(false)),
    )
}

/// `array[i] = p`: within the array or one past its end (an append).
fn set_point(points: &mut Vec<P>, i: isize, p: P) -> Result<()> {
    match usize::try_from(i) {
        Ok(i) if i < points.len() => {
            points[i] = p;
            Ok(())
        }
        Ok(i) if i == points.len() => {
            points.push(p);
            Ok(())
        }
        _ => Err(missing_point()),
    }
}

/// `handleEndpointDrag(...)` (`elbowArrow.ts:706-900`): the inner points
/// stay; the second and second-last points follow the moved ends, and an
/// end whose heading runs along its neighbouring segment gets two extra
/// points `BASE_PADDING` out (it becomes "special").
fn handle_endpoint_drag(
    arrow: &Element,
    fields: &ArrowFields,
    updated_points: &[P],
    fixed_segments: &[FixedSegment],
    data: &ElbowArrowData<'_>,
) -> Result<ElbowArrowUpdate> {
    let (ax, ay) = (arrow.base.x, arrow.base.y);
    let points = &fields.linear.points;
    let start_heading = data.start_heading;
    let end_heading = data.end_heading;
    let start_global_point = data.start_global_point;
    let end_global_point = data.end_global_point;
    let hovered_start = data.hovered_start_element.is_some();
    let hovered_end = data.hovered_end_element.is_some();

    // `arrow.startIsSpecial ?? null`
    let mut start_is_special: Option<bool> = fields.start_is_special.flatten();
    let mut end_is_special: Option<bool> = fields.end_is_special.flatten();
    let truthy = |v: Option<bool>| v == Some(true);
    let js_flag = |v: Option<bool>| v.map_or("null".to_owned(), |b| b.to_string());

    let mut global_updated_points: Vec<P> = Vec::with_capacity(updated_points.len());
    for (i, p) in updated_points.iter().enumerate() {
        if i == 0 || i == updated_points.len() - 1 {
            global_updated_points.push([ax + p[0], ay + p[1]]);
        } else {
            let own = at(points, i as isize)?;
            global_updated_points.push([ax + own[0], ay + own[1]]);
        }
    }
    let first = at(updated_points, 0)?;
    let mut next_fixed_segments: Vec<FixedSegment> = fixed_segments
        .iter()
        .map(|s| FixedSegment {
            start: [ax + (s.start[0] - first[0]), ay + (s.start[1] - first[1])],
            end: [ax + (s.end[0] - first[0]), ay + (s.end[1] - first[1])],
            index: s.index,
        })
        .collect();
    let mut new_points: Vec<P> = Vec::new();

    // The inside points
    let offset = 2 + usize::from(truthy(start_is_special));
    let end_offset = 2 + usize::from(truthy(end_is_special));
    let inside_end = global_updated_points.len() as isize - end_offset as isize;
    while ((new_points.len() + offset) as isize) < inside_end {
        new_points.push(global_updated_points[new_points.len() + offset]);
    }

    // The moving second point, then the start point
    {
        let second = at_relative(
            &global_updated_points,
            if truthy(start_is_special) { 2 } else { 1 },
        );
        let third = at_relative(
            &global_updated_points,
            if truthy(start_is_special) { 3 } else { 2 },
        );
        let (Some(second_point), Some(third_point)) = (second, third) else {
            return Err(ElbowArrowError(format!(
                "Second and third points must exist when handling endpoint drag ({})",
                js_flag(start_is_special)
            )));
        };
        let start_is_horizontal = heading_is_horizontal(start_heading);
        let second_is_horizontal = heading_is_horizontal(vector_to_heading([
            second_point[0] - third_point[0],
            second_point[1] - third_point[1],
        ]));
        if hovered_start && start_is_horizontal == second_is_horizontal {
            let positive = if start_is_horizontal {
                start_heading == Heading::Right
            } else {
                start_heading == Heading::Down
            };
            let pad = if positive {
                BASE_PADDING
            } else {
                -BASE_PADDING
            };
            new_points.insert(
                0,
                [
                    if !second_is_horizontal {
                        third_point[0]
                    } else {
                        start_global_point[0] + pad
                    },
                    if second_is_horizontal {
                        third_point[1]
                    } else {
                        start_global_point[1] + pad
                    },
                ],
            );
            new_points.insert(
                0,
                [
                    if start_is_horizontal {
                        start_global_point[0] + pad
                    } else {
                        start_global_point[0]
                    },
                    if !start_is_horizontal {
                        start_global_point[1] + pad
                    } else {
                        start_global_point[1]
                    },
                ],
            );
            if !truthy(start_is_special) {
                start_is_special = Some(true);
                for segment in &mut next_fixed_segments {
                    if segment.index > 1.0 {
                        segment.index += 1.0;
                    }
                }
            }
        } else {
            new_points.insert(
                0,
                [
                    if !second_is_horizontal {
                        second_point[0]
                    } else {
                        start_global_point[0]
                    },
                    if second_is_horizontal {
                        second_point[1]
                    } else {
                        start_global_point[1]
                    },
                ],
            );
            if truthy(start_is_special) {
                start_is_special = Some(false);
                for segment in &mut next_fixed_segments {
                    if segment.index > 1.0 {
                        segment.index -= 1.0;
                    }
                }
            }
        }
        new_points.insert(0, start_global_point);
    }

    // The moving second-to-last point
    {
        let len = global_updated_points.len() as isize;
        let second_to_last = at_relative(
            &global_updated_points,
            len - if truthy(end_is_special) { 3 } else { 2 },
        );
        let third_to_last = at_relative(
            &global_updated_points,
            len - if truthy(end_is_special) { 4 } else { 3 },
        );
        let (Some(second_to_last), Some(third_to_last)) = (second_to_last, third_to_last) else {
            return Err(ElbowArrowError(format!(
                "Second and third to last points must exist when handling endpoint drag ({})",
                js_flag(end_is_special)
            )));
        };
        let end_is_horizontal = heading_is_horizontal(end_heading);
        let second_is_horizontal = heading_for_point_is_horizontal(third_to_last, second_to_last);
        if hovered_end && end_is_horizontal == second_is_horizontal {
            let positive = if end_is_horizontal {
                end_heading == Heading::Right
            } else {
                end_heading == Heading::Down
            };
            let pad = if positive {
                BASE_PADDING
            } else {
                -BASE_PADDING
            };
            new_points.push([
                if !second_is_horizontal {
                    third_to_last[0]
                } else {
                    end_global_point[0] + pad
                },
                if second_is_horizontal {
                    third_to_last[1]
                } else {
                    end_global_point[1] + pad
                },
            ]);
            new_points.push([
                if end_is_horizontal {
                    end_global_point[0] + pad
                } else {
                    end_global_point[0]
                },
                if !end_is_horizontal {
                    end_global_point[1] + pad
                } else {
                    end_global_point[1]
                },
            ]);
            if !truthy(end_is_special) {
                end_is_special = Some(true);
            }
        } else {
            new_points.push([
                if !second_is_horizontal {
                    second_to_last[0]
                } else {
                    end_global_point[0]
                },
                if second_is_horizontal {
                    second_to_last[1]
                } else {
                    end_global_point[1]
                },
            ]);
            if truthy(end_is_special) {
                end_is_special = Some(false);
            }
        }
    }
    new_points.push(end_global_point);

    let mut segments = Vec::with_capacity(next_fixed_segments.len());
    for s in &next_fixed_segments {
        let index = as_index(s.index);
        let start = at(&new_points, index - 1)?;
        let end = at(&new_points, index)?;
        segments.push(FixedSegment {
            start: [
                start[0] - start_global_point[0],
                start[1] - start_global_point[1],
            ],
            end: [
                end[0] - start_global_point[0],
                end[1] - start_global_point[1],
            ],
            index: s.index,
        });
    }
    normalize_arrow_element_update(
        &new_points,
        SegmentsArg::List(&segments),
        Some(start_is_special),
        Some(end_is_special),
    )
}

// -- routing ---------------------------------------------------------------------

/// `ElbowArrowState` (`elbowArrow.ts:88-95`) plus the points it is routed
/// through: position, bindings and whether each end has an arrowhead.
struct ElbowArrowState {
    x: f64,
    y: f64,
    start_binding: Option<FixedPointBinding>,
    end_binding: Option<FixedPointBinding>,
    start_arrowhead: bool,
    end_arrowhead: bool,
}

impl ElbowArrowState {
    fn of(arrow: &Element, fields: &ArrowFields) -> ElbowArrowState {
        ElbowArrowState {
            x: arrow.base.x,
            y: arrow.base.y,
            start_binding: fields.linear.start_binding.clone(),
            end_binding: fields.linear.end_binding.clone(),
            start_arrowhead: fields.linear.start_arrowhead.is_some(),
            end_arrowhead: fields.linear.end_arrowhead.is_some(),
        }
    }
}

/// `ElbowArrowData` (`elbowArrow.ts:97-108`).
struct ElbowArrowData<'a> {
    dynamic_aabbs: [Bounds; 2],
    start_dongle_position: P,
    start_global_point: P,
    start_heading: Heading,
    end_dongle_position: P,
    end_global_point: P,
    end_heading: Heading,
    common_bounds: Bounds,
    hovered_start_element: Option<&'a Element>,
    hovered_end_element: Option<&'a Element>,
}

/// `offsetFromHeading(heading, head, side)` (`elbowArrow.ts:1507-1522`):
/// `[up, right, down, left]` with `head` on the heading's side.
fn offset_from_heading(heading: Heading, head: f64, side: f64) -> [f64; 4] {
    match heading {
        Heading::Up => [head, side, side, side],
        Heading::Right => [side, head, side, side],
        Heading::Down => [side, side, head, side],
        Heading::Left => [side, side, side, head],
    }
}

/// `getElbowArrowData(arrow, elementsMap, nextPoints)`
/// (`elbowArrow.ts:1192-1426`) outside a drag.
fn get_elbow_arrow_data<'a>(
    arrow: &ElbowArrowState,
    elements_map: &ElementsMap<'a>,
    next_points: &[P],
) -> ElbowArrowData<'a> {
    let first = next_points.first().copied().unwrap_or([f64::NAN, f64::NAN]);
    let last = next_points.last().copied().unwrap_or([f64::NAN, f64::NAN]);
    let orig_start_global_point = [first[0] + arrow.x, first[1] + arrow.y];
    let orig_end_global_point = [last[0] + arrow.x, last[1] + arrow.y];

    let hovered_start_element = arrow
        .start_binding
        .as_ref()
        .and_then(|b| elements_map.bindable(&b.element_id));
    let hovered_end_element = arrow
        .end_binding
        .as_ref()
        .and_then(|b| elements_map.bindable(&b.element_id));

    // getGlobalPoint (elbowArrow.ts:2214-2251)
    let global_point =
        |binding: &Option<FixedPointBinding>, element: Option<&Element>, orig: P| match element {
            Some(element) => get_global_fixed_point_for_bindable_element(
                binding.as_ref().map_or([0.0, 0.0], |b| b.fixed_point),
                element,
            ),
            None => orig,
        };
    let start_global_point = global_point(
        &arrow.start_binding,
        hovered_start_element,
        orig_start_global_point,
    );
    let end_global_point = global_point(
        &arrow.end_binding,
        hovered_end_element,
        orig_end_global_point,
    );
    let start_heading = get_bind_point_heading(
        start_global_point,
        end_global_point,
        hovered_start_element,
        orig_start_global_point,
    );
    let end_heading = get_bind_point_heading(
        end_global_point,
        start_global_point,
        hovered_end_element,
        orig_end_global_point,
    );
    let point_bounds = |p: P| [p[0] - 2.0, p[1] - 2.0, p[0] + 2.0, p[1] + 2.0];
    let start_point_bounds = point_bounds(start_global_point);
    let end_point_bounds = point_bounds(end_global_point);
    let element_bounds =
        |element: Option<&Element>, heading: Heading, arrowhead: bool, fallback| match element {
            Some(element) => {
                let gap = get_binding_gap(element);
                aabb_for_element(
                    element,
                    Some(offset_from_heading(
                        heading,
                        if arrowhead { gap * 6.0 } else { gap * 2.0 },
                        1.0,
                    )),
                )
            }
            None => fallback,
        };
    let start_element_bounds = element_bounds(
        hovered_start_element,
        start_heading,
        arrow.start_arrowhead,
        start_point_bounds,
    );
    let end_element_bounds = element_bounds(
        hovered_end_element,
        end_heading,
        arrow.end_arrowhead,
        end_point_bounds,
    );
    let padded = |element: Option<&Element>, heading: Heading, fallback: Bounds| match element {
        Some(element) => aabb_for_element(
            element,
            Some(offset_from_heading(heading, BASE_PADDING, BASE_PADDING)),
        ),
        None => fallback,
    };
    let bounds_overlap = point_inside_bounds(
        start_global_point,
        padded(hovered_end_element, end_heading, end_point_bounds),
    ) || point_inside_bounds(
        end_global_point,
        padded(hovered_start_element, start_heading, start_point_bounds),
    );
    let common_bounds = common_aabb(&if bounds_overlap {
        [start_point_bounds, end_point_bounds]
    } else {
        [start_element_bounds, end_element_bounds]
    });
    let neither_hovered = hovered_start_element.is_none() && hovered_end_element.is_none();
    let difference = |heading: Heading, arrowhead: bool| {
        if bounds_overlap {
            offset_from_heading(
                heading,
                if neither_hovered { 0.0 } else { BASE_PADDING },
                0.0,
            )
        } else {
            offset_from_heading(
                heading,
                if neither_hovered {
                    0.0
                } else {
                    BASE_PADDING
                        - if arrowhead {
                            BASE_BINDING_GAP * 6.0
                        } else {
                            BASE_BINDING_GAP * 2.0
                        }
                },
                BASE_PADDING,
            )
        }
    };
    let dynamic_aabbs = generate_dynamic_aabbs(
        if bounds_overlap {
            start_point_bounds
        } else {
            start_element_bounds
        },
        if bounds_overlap {
            end_point_bounds
        } else {
            end_element_bounds
        },
        common_bounds,
        difference(start_heading, arrow.start_arrowhead),
        difference(end_heading, arrow.end_arrowhead),
        bounds_overlap,
        hovered_start_element.map(|e| aabb_for_element(e, None)),
        hovered_end_element.map(|e| aabb_for_element(e, None)),
    );
    let start_dongle_position =
        get_dongle_position(dynamic_aabbs[0], start_heading, start_global_point);
    let end_dongle_position = get_dongle_position(dynamic_aabbs[1], end_heading, end_global_point);

    ElbowArrowData {
        dynamic_aabbs,
        start_dongle_position,
        start_global_point,
        start_heading,
        end_dongle_position,
        end_global_point,
        end_heading,
        common_bounds,
        hovered_start_element,
        hovered_end_element,
    }
}

/// `getBindPointHeading(p, otherPoint, hoveredElement, origPoint)`
/// (`elbowArrow.ts:2253-2279`): the snap heading, the element's box grown
/// on every side by `p`'s distance to its outline.
fn get_bind_point_heading(
    p: P,
    other_point: P,
    hovered_element: Option<&Element>,
    orig_point: P,
) -> Heading {
    let aabb = hovered_element.map(|element| {
        let d = crate::geometry::distance_to_element(element, p);
        aabb_for_element(element, Some([d, d, d, d]))
    });
    get_heading_for_elbow_arrow_snap(p, other_point, hovered_element, aabb, orig_point)
}

/// `routeElbowArrow(arrow, elbowArrowData)` (`elbowArrow.ts:1437-1505`):
/// the A* path between the dongles with the true ends added, `None` when
/// there is none. `start_bound` is whether the arrow passed in (not the
/// state the data was computed for) has a start binding.
fn route_elbow_arrow(start_bound: bool, data: &ElbowArrowData<'_>) -> Result<Option<Vec<P>>> {
    let mut grid = calculate_grid(
        &data.dynamic_aabbs,
        data.start_dongle_position,
        data.start_heading,
        data.end_dongle_position,
        data.end_heading,
        data.common_bounds,
    );
    let start_dongle = point_to_grid_node(data.start_dongle_position, &grid);
    let end_dongle = point_to_grid_node(data.end_dongle_position, &grid);

    // Do not allow stepping on the true end or true start points
    let end_node = point_to_grid_node(data.end_global_point, &grid);
    if let Some(end_node) = end_node {
        if data.hovered_end_element.is_some() {
            grid.data[end_node].closed = true;
        }
    }
    let start_node = point_to_grid_node(data.start_global_point, &grid);
    if let Some(start_node) = start_node {
        if start_bound {
            grid.data[start_node].closed = true;
        }
    }
    let dongle_overlap = match (start_dongle, end_dongle) {
        (Some(s), Some(e)) => {
            point_inside_bounds(grid.data[s].pos, data.dynamic_aabbs[1])
                || point_inside_bounds(grid.data[e].pos, data.dynamic_aabbs[0])
        }
        _ => false,
    };

    let (Some(from), Some(to)) = (start_dongle.or(start_node), end_dongle.or(end_node)) else {
        return Err(ElbowArrowError(
            "Cannot read properties of null (reading 'pos')".into(),
        ));
    };
    let path = astar(
        from,
        to,
        &mut grid,
        data.start_heading,
        data.end_heading,
        if dongle_overlap {
            &[]
        } else {
            &data.dynamic_aabbs[..]
        },
    );
    Ok(path.map(|path| {
        let mut points: Vec<P> = path.iter().map(|&n| grid.data[n].pos).collect();
        if start_dongle.is_some() {
            points.insert(0, data.start_global_point);
        }
        if end_dongle.is_some() {
            points.push(data.end_global_point);
        }
        points
    }))
}

/// A grid node (`Node`, `elbowArrow.ts:71-80`).
#[derive(Debug, Clone)]
struct Node {
    f: f64,
    g: f64,
    h: f64,
    closed: bool,
    visited: bool,
    parent: Option<usize>,
    pos: P,
    /// `[col, row]`.
    addr: [usize; 2],
}

/// `Grid` (`elbowArrow.ts:82-86`): nodes row by row.
struct Grid {
    row: usize,
    col: usize,
    data: Vec<Node>,
}

/// `m_dist(a, b)` (`elbowArrow.ts:1658-1659`): the Manhattan distance.
fn m_dist(a: P, b: P) -> f64 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs()
}

/// `pointDistance(a, b)`: `Math.hypot` of the difference.
fn js_point_distance(a: P, b: P) -> f64 {
    point_distance::<excali_math::Global>(a.into(), b.into())
}

/// `neighborIndexToHeading(idx)` (`elbowArrow.ts:2202-2212`).
fn neighbor_index_to_heading(idx: usize) -> Heading {
    match idx {
        0 => Heading::Up,
        1 => Heading::Right,
        2 => Heading::Down,
        _ => Heading::Left,
    }
}

/// `astar(start, end, grid, startHeading, endHeading, aabbs)`
/// (`elbowArrow.ts:1535-1644`): the node path, `None` when the end cannot
/// be reached.
fn astar(
    start: usize,
    end: usize,
    grid: &mut Grid,
    start_heading: Heading,
    end_heading: Heading,
    aabbs: &[Bounds],
) -> Option<Vec<usize>> {
    let bend_multiplier = m_dist(grid.data[start].pos, grid.data[end].pos);
    let mut open: BinaryHeap<usize> = BinaryHeap::new();
    open.push(start, &|n: usize| grid.data[n].f);

    while open.size() > 0 {
        let current = open.pop(&|n: usize| grid.data[n].f)?;
        if grid.data[current].closed {
            continue;
        }
        if current == end {
            return Some(path_to(start, current, grid));
        }
        grid.data[current].closed = true;

        let neighbors = get_neighbors(grid.data[current].addr, grid);
        for (i, neighbor) in neighbors.into_iter().enumerate() {
            let Some(neighbor) = neighbor else {
                continue;
            };
            if grid.data[neighbor].closed {
                continue;
            }
            let current_pos = grid.data[current].pos;
            let neighbor_pos = grid.data[neighbor].pos;

            // Intersect
            let half = point_scale_from_origin::<excali_math::Global>(
                neighbor_pos.into(),
                current_pos.into(),
                0.5,
            );
            if aabbs
                .iter()
                .any(|&aabb| point_inside_bounds([half.x, half.y], aabb))
            {
                continue;
            }

            let neighbor_heading = neighbor_index_to_heading(i);
            let previous_direction = match grid.data[current].parent {
                Some(parent) => {
                    let parent_pos = grid.data[parent].pos;
                    vector_to_heading([
                        current_pos[0] - parent_pos[0],
                        current_pos[1] - parent_pos[1],
                    ])
                }
                None => start_heading,
            };

            // Do not allow going in reverse
            let reverse_heading = flip_heading(previous_direction);
            let neighbor_is_reverse_route = reverse_heading == neighbor_heading
                || (grid.data[start].addr == grid.data[neighbor].addr
                    && neighbor_heading == start_heading)
                || (grid.data[end].addr == grid.data[neighbor].addr
                    && neighbor_heading == end_heading);
            if neighbor_is_reverse_route {
                continue;
            }

            let direction_change = previous_direction != neighbor_heading;
            let g_score = grid.data[current].g
                + m_dist(neighbor_pos, current_pos)
                + if direction_change {
                    js::pow(bend_multiplier, 3.0)
                } else {
                    0.0
                };

            let been_visited = grid.data[neighbor].visited;
            if !been_visited || g_score < grid.data[neighbor].g {
                let est_bend_count = estimate_segment_count(
                    neighbor_pos,
                    grid.data[end].pos,
                    neighbor_heading,
                    end_heading,
                );
                let end_pos = grid.data[end].pos;
                let node = &mut grid.data[neighbor];
                node.visited = true;
                node.parent = Some(current);
                node.h =
                    m_dist(end_pos, neighbor_pos) + est_bend_count * js::pow(bend_multiplier, 2.0);
                node.g = g_score;
                node.f = node.g + node.h;
                if !been_visited {
                    open.push(neighbor, &|n: usize| grid.data[n].f);
                } else {
                    open.rescore_element(neighbor, &|n: usize| grid.data[n].f);
                }
            }
        }
    }
    None
}

/// `pathTo(start, node)` (`elbowArrow.ts:1646-1656`).
fn path_to(start: usize, node: usize, grid: &Grid) -> Vec<usize> {
    let mut path = Vec::new();
    let mut curr = node;
    while let Some(parent) = grid.data[curr].parent {
        path.push(curr);
        curr = parent;
    }
    path.push(start);
    path.reverse();
    path
}

/// `generateDynamicAABBs(a, b, common, startDifference, endDifference,
/// disableSideHack, startElementBounds, endElementBounds)`
/// (`elbowArrow.ts:1666-1842`): two boxes around the ends' boxes that meet
/// halfway between them, split diagonally when they would overlap.
#[allow(clippy::too_many_arguments)]
fn generate_dynamic_aabbs(
    a: Bounds,
    b: Bounds,
    common: Bounds,
    start_difference: [f64; 4],
    end_difference: [f64; 4],
    disable_side_hack: bool,
    start_element_bounds: Option<Bounds>,
    end_element_bounds: Option<Bounds>,
) -> [Bounds; 2] {
    let start_el = start_element_bounds.unwrap_or(a);
    let end_el = end_element_bounds.unwrap_or(b);
    let [start_up, start_right, start_down, start_left] = start_difference;
    let [end_up, end_right, end_down, end_left] = end_difference;

    let first: Bounds = [
        if a[0] > b[2] {
            if a[1] > b[3] || a[3] < b[1] {
                js::min((start_el[0] + end_el[2]) / 2.0, a[0] - start_left)
            } else {
                (start_el[0] + end_el[2]) / 2.0
            }
        } else if a[0] > b[0] {
            a[0] - start_left
        } else {
            common[0] - start_left
        },
        if a[1] > b[3] {
            if a[0] > b[2] || a[2] < b[0] {
                js::min((start_el[1] + end_el[3]) / 2.0, a[1] - start_up)
            } else {
                (start_el[1] + end_el[3]) / 2.0
            }
        } else if a[1] > b[1] {
            a[1] - start_up
        } else {
            common[1] - start_up
        },
        if a[2] < b[0] {
            if a[1] > b[3] || a[3] < b[1] {
                js::max((start_el[2] + end_el[0]) / 2.0, a[2] + start_right)
            } else {
                (start_el[2] + end_el[0]) / 2.0
            }
        } else if a[2] < b[2] {
            a[2] + start_right
        } else {
            common[2] + start_right
        },
        if a[3] < b[1] {
            if a[0] > b[2] || a[2] < b[0] {
                js::max((start_el[3] + end_el[1]) / 2.0, a[3] + start_down)
            } else {
                (start_el[3] + end_el[1]) / 2.0
            }
        } else if a[3] < b[3] {
            a[3] + start_down
        } else {
            common[3] + start_down
        },
    ];
    let second: Bounds = [
        if b[0] > a[2] {
            if b[1] > a[3] || b[3] < a[1] {
                js::min((end_el[0] + start_el[2]) / 2.0, b[0] - end_left)
            } else {
                (end_el[0] + start_el[2]) / 2.0
            }
        } else if b[0] > a[0] {
            b[0] - end_left
        } else {
            common[0] - end_left
        },
        if b[1] > a[3] {
            if b[0] > a[2] || b[2] < a[0] {
                js::min((end_el[1] + start_el[3]) / 2.0, b[1] - end_up)
            } else {
                (end_el[1] + start_el[3]) / 2.0
            }
        } else if b[1] > a[1] {
            b[1] - end_up
        } else {
            common[1] - end_up
        },
        if b[2] < a[0] {
            if b[1] > a[3] || b[3] < a[1] {
                js::max((end_el[2] + start_el[0]) / 2.0, b[2] + end_right)
            } else {
                (end_el[2] + start_el[0]) / 2.0
            }
        } else if b[2] < a[2] {
            b[2] + end_right
        } else {
            common[2] + end_right
        },
        if b[3] < a[1] {
            if b[0] > a[2] || b[2] < a[0] {
                js::max((end_el[3] + start_el[1]) / 2.0, b[3] + end_down)
            } else {
                (end_el[3] + start_el[1]) / 2.0
            }
        } else if b[3] < a[3] {
            b[3] + end_down
        } else {
            common[3] + end_down
        },
    ];

    let c = common_aabb(&[first, second]);
    if !disable_side_hack
        && first[2] - first[0] + second[2] - second[0] > c[2] - c[0] + 0.00000000001
        && first[3] - first[1] + second[3] - second[1] > c[3] - c[1] + 0.00000000001
    {
        let end_center_x = (second[0] + second[2]) / 2.0;
        let end_center_y = (second[1] + second[3]) / 2.0;
        let cross = |p: [f64; 2], q: [f64; 2]| {
            vector_cross(
                [p[0] - end_center_x, p[1] - end_center_y].into(),
                [q[0] - end_center_x, q[1] - end_center_y].into(),
            )
        };
        if b[0] > a[2] && a[1] > b[3] {
            // BOTTOM LEFT
            let c_x = first[2] + (second[0] - first[2]) / 2.0;
            let c_y = second[3] + (first[1] - second[3]) / 2.0;
            if cross([a[2], a[1]], [a[0], a[3]]) > 0.0 {
                return [
                    [first[0], first[1], c_x, first[3]],
                    [c_x, second[1], second[2], second[3]],
                ];
            }
            return [
                [first[0], c_y, first[2], first[3]],
                [second[0], second[1], second[2], c_y],
            ];
        } else if a[2] < b[0] && a[3] < b[1] {
            // TOP LEFT
            let c_x = first[2] + (second[0] - first[2]) / 2.0;
            let c_y = first[3] + (second[1] - first[3]) / 2.0;
            if cross([a[0], a[1]], [a[2], a[3]]) > 0.0 {
                return [
                    [first[0], first[1], first[2], c_y],
                    [second[0], c_y, second[2], second[3]],
                ];
            }
            return [
                [first[0], first[1], c_x, first[3]],
                [c_x, second[1], second[2], second[3]],
            ];
        } else if a[0] > b[2] && a[3] < b[1] {
            // TOP RIGHT
            let c_x = second[2] + (first[0] - second[2]) / 2.0;
            let c_y = first[3] + (second[1] - first[3]) / 2.0;
            if cross([a[2], a[1]], [a[0], a[3]]) > 0.0 {
                return [
                    [c_x, first[1], first[2], first[3]],
                    [second[0], second[1], c_x, second[3]],
                ];
            }
            return [
                [first[0], first[1], first[2], c_y],
                [second[0], c_y, second[2], second[3]],
            ];
        } else if a[0] > b[2] && a[1] > b[3] {
            // BOTTOM RIGHT
            let c_x = second[2] + (first[0] - second[2]) / 2.0;
            let c_y = second[3] + (first[1] - second[3]) / 2.0;
            if cross([a[0], a[1]], [a[2], a[3]]) > 0.0 {
                return [
                    [c_x, first[1], first[2], first[3]],
                    [second[0], second[1], c_x, second[3]],
                ];
            }
            return [
                [first[0], c_y, first[2], first[3]],
                [second[0], second[1], second[2], c_y],
            ];
        }
    }
    [first, second]
}

/// Adds `value` to a JS `Set` of numbers (SameValueZero: `-0` is `0`,
/// NaN is NaN), keeping insertion order.
fn set_add(set: &mut Vec<f64>, value: f64) {
    let seen = set
        .iter()
        .any(|&v| v == value || (v.is_nan() && value.is_nan()));
    if !seen {
        set.push(value);
    }
}

/// `calculateGrid(aabbs, start, startHeading, end, endHeading, common)`
/// (`elbowArrow.ts:1851-1906`): a node at every crossing of the boxes'
/// edges, the common box's edges and the lines through the dongles along
/// their headings.
fn calculate_grid(
    aabbs: &[Bounds],
    start: P,
    start_heading: Heading,
    end: P,
    end_heading: Heading,
    common: Bounds,
) -> Grid {
    let mut horizontal: Vec<f64> = Vec::new();
    let mut vertical: Vec<f64> = Vec::new();
    if heading_is_horizontal(start_heading) {
        set_add(&mut vertical, start[1]);
    } else {
        set_add(&mut horizontal, start[0]);
    }
    if heading_is_horizontal(end_heading) {
        set_add(&mut vertical, end[1]);
    } else {
        set_add(&mut horizontal, end[0]);
    }
    for aabb in aabbs {
        set_add(&mut horizontal, aabb[0]);
        set_add(&mut horizontal, aabb[2]);
        set_add(&mut vertical, aabb[1]);
        set_add(&mut vertical, aabb[3]);
    }
    set_add(&mut horizontal, common[0]);
    set_add(&mut horizontal, common[2]);
    set_add(&mut vertical, common[1]);
    set_add(&mut vertical, common[3]);

    js::sort(&mut vertical, |a, b| a - b);
    js::sort(&mut horizontal, |a, b| a - b);

    let mut data = Vec::with_capacity(vertical.len() * horizontal.len());
    for (row, &y) in vertical.iter().enumerate() {
        for (col, &x) in horizontal.iter().enumerate() {
            data.push(Node {
                f: 0.0,
                g: 0.0,
                h: 0.0,
                closed: false,
                visited: false,
                parent: None,
                addr: [col, row],
                pos: [x, y],
            });
        }
    }
    Grid {
        row: vertical.len(),
        col: horizontal.len(),
        data,
    }
}

/// `getDonglePosition(bounds, heading, p)` (`elbowArrow.ts:1908-1922`):
/// where the heading leaves the box, level with `p`.
fn get_dongle_position(bounds: Bounds, heading: Heading, p: P) -> P {
    match heading {
        Heading::Up => [p[0], bounds[1]],
        Heading::Right => [bounds[2], p[1]],
        Heading::Down => [p[0], bounds[3]],
        Heading::Left => [bounds[0], p[1]],
    }
}

/// `estimateSegmentCount(start, end, startHeading, endHeading)`
/// (`elbowArrow.ts:1924-2037`): how many more bends the route from `start`
/// needs at least.
fn estimate_segment_count(start: P, end: P, start_heading: Heading, end_heading: Heading) -> f64 {
    use Heading::{Down, Left, Right, Up};
    let count = match (end_heading, start_heading) {
        (Right, Right) => {
            if start[0] >= end[0] {
                4
            } else if start[1] == end[1] {
                0
            } else {
                2
            }
        }
        (Right, Up) => {
            if start[1] > end[1] && start[0] < end[0] {
                1
            } else {
                3
            }
        }
        (Right, Down) => {
            if start[1] < end[1] && start[0] < end[0] {
                1
            } else {
                3
            }
        }
        (Right, Left) => {
            if start[1] == end[1] {
                4
            } else {
                2
            }
        }
        (Left, Right) => {
            if start[1] == end[1] {
                4
            } else {
                2
            }
        }
        (Left, Up) => {
            if start[1] > end[1] && start[0] > end[0] {
                1
            } else {
                3
            }
        }
        (Left, Down) => {
            if start[1] < end[1] && start[0] > end[0] {
                1
            } else {
                3
            }
        }
        (Left, Left) => {
            if start[0] <= end[0] {
                4
            } else if start[1] == end[1] {
                0
            } else {
                2
            }
        }
        (Up, Right) => {
            if start[1] > end[1] && start[0] < end[0] {
                1
            } else {
                3
            }
        }
        (Up, Up) => {
            if start[1] >= end[1] {
                4
            } else if start[0] == end[0] {
                0
            } else {
                2
            }
        }
        (Up, Down) => {
            if start[0] == end[0] {
                4
            } else {
                2
            }
        }
        (Up, Left) => {
            if start[1] > end[1] && start[0] > end[0] {
                1
            } else {
                3
            }
        }
        (Down, Right) => {
            if start[1] < end[1] && start[0] < end[0] {
                1
            } else {
                3
            }
        }
        (Down, Up) => {
            if start[0] == end[0] {
                4
            } else {
                2
            }
        }
        (Down, Down) => {
            if start[1] <= end[1] {
                4
            } else if start[0] == end[0] {
                0
            } else {
                2
            }
        }
        (Down, Left) => {
            if start[1] < end[1] && start[0] > end[0] {
                1
            } else {
                3
            }
        }
    };
    f64::from(count)
}

/// `getNeighbors([col, row], grid)` (`elbowArrow.ts:2042-2048`): up,
/// right, down, left.
fn get_neighbors(addr: [usize; 2], grid: &Grid) -> [Option<usize>; 4] {
    let [col, row] = addr.map(|v| v as isize);
    [
        grid_node_from_addr(col, row - 1, grid),
        grid_node_from_addr(col + 1, row, grid),
        grid_node_from_addr(col, row + 1, grid),
        grid_node_from_addr(col - 1, row, grid),
    ]
}

/// `gridNodeFromAddr([col, row], grid)` (`elbowArrow.ts:2050-2059`).
fn grid_node_from_addr(col: isize, row: isize, grid: &Grid) -> Option<usize> {
    if col < 0 || col >= grid.col as isize || row < 0 || row >= grid.row as isize {
        return None;
    }
    let idx = row as usize * grid.col + col as usize;
    (idx < grid.data.len()).then_some(idx)
}

/// `pointToGridNode(point, grid)` (`elbowArrow.ts:2064-2079`): the node
/// exactly at `point`, searched column by column.
fn point_to_grid_node(point: P, grid: &Grid) -> Option<usize> {
    for col in 0..grid.col {
        for row in 0..grid.row {
            if let Some(candidate) = grid_node_from_addr(col as isize, row as isize, grid) {
                let pos = grid.data[candidate].pos;
                if point[0] == pos[0] && point[1] == pos[1] {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// `commonAABB(aabbs)` (`elbowArrow.ts:2081-2086`).
fn common_aabb(aabbs: &[Bounds]) -> Bounds {
    let fold = |i: usize, init: f64, f: fn(f64, f64) -> f64| {
        aabbs.iter().fold(init, |m, aabb| f(m, aabb[i]))
    };
    [
        fold(0, f64::INFINITY, js::min),
        fold(1, f64::INFINITY, js::min),
        fold(2, f64::NEG_INFINITY, js::max),
        fold(3, f64::NEG_INFINITY, js::max),
    ]
}

// -- normalisation -------------------------------------------------------------

/// The fixed segments `normalizeArrowElementUpdate` is given: a list, or
/// the arrow's own key as read (absent, `null` or a list).
enum SegmentsArg<'s> {
    List(&'s [FixedSegment]),
    Arrow(&'s Option<Option<Vec<FixedSegment>>>),
}

/// `normalizeArrowElementUpdate(global, nextFixedSegments, startIsSpecial,
/// endIsSpecial)` (`elbowArrow.ts:2102-2154`): the points relative to the
/// first, which becomes `x`, `y`, everything clamped to +-1e6, with the
/// size of the points; an empty segment list becomes `null`.
fn normalize_arrow_element_update(
    global: &[P],
    next_fixed_segments: SegmentsArg<'_>,
    start_is_special: Option<Option<bool>>,
    end_is_special: Option<Option<bool>>,
) -> Result<ElbowArrowUpdate> {
    let Some(&[offset_x, offset_y]) = global.first() else {
        return Err(missing_point());
    };
    // Upstream logs past MAX_POS (elbowArrow.ts:2119-2138) and clamps below.
    let points: Vec<P> = global
        .iter()
        .map(|p| {
            [
                excali_math::clamp(p[0] + -offset_x, -MAX_POS, MAX_POS),
                excali_math::clamp(p[1] + -offset_y, -MAX_POS, MAX_POS),
            ]
        })
        .collect();
    let segments: Option<Vec<FixedSegment>> = match next_fixed_segments {
        SegmentsArg::List(list) => (!list.is_empty()).then(|| list.to_vec()),
        SegmentsArg::Arrow(own) => own
            .as_ref()
            .and_then(Option::as_ref)
            .filter(|l| !l.is_empty())
            .cloned(),
    };
    let (width, height) = size_from_points(&points);
    Ok(ElbowArrowUpdate {
        points: Some(points),
        x: Some(excali_math::clamp(offset_x, -MAX_POS, MAX_POS)),
        y: Some(excali_math::clamp(offset_y, -MAX_POS, MAX_POS)),
        fixed_segments: Some(segments),
        width: Some(width),
        height: Some(height),
        start_is_special,
        end_is_special,
        ..ElbowArrowUpdate::default()
    })
}

/// `getSizeFromPoints(points)` (`common/src/points.ts:10-19`).
fn size_from_points(points: &[P]) -> (f64, f64) {
    let span = |i: usize| {
        let max = points
            .iter()
            .fold(f64::NEG_INFINITY, |m, p| js::max(m, p[i]));
        let min = points.iter().fold(f64::INFINITY, |m, p| js::min(m, p[i]));
        max - min
    };
    (span(0), span(1))
}

/// `getElbowArrowCornerPoints(points)` (`elbowArrow.ts:2156-2182`): only
/// the points where the route turns, and the ends.
fn get_elbow_arrow_corner_points(points: Vec<P>) -> Vec<P> {
    if points.len() <= 1 {
        return points;
    }
    let horizontal = |a: P, b: P| (a[1] - b[1]).abs() < (a[0] - b[0]).abs();
    let mut previous_horizontal = horizontal(points[0], points[1]);
    let last = points.len() - 1;
    let mut kept = Vec::with_capacity(points.len());
    for (idx, &p) in points.iter().enumerate() {
        if idx == 0 || idx == last {
            kept.push(p);
            continue;
        }
        let next_horizontal = horizontal(p, points[idx + 1]);
        if previous_horizontal == next_horizontal {
            previous_horizontal = next_horizontal;
            continue;
        }
        previous_horizontal = next_horizontal;
        kept.push(p);
    }
    kept
}

/// `removeElbowArrowShortSegments(points)` (`elbowArrow.ts:2184-2200`):
/// with four points or more, inner points within 1 of the point before
/// them go.
fn remove_elbow_arrow_short_segments(points: Vec<P>) -> Vec<P> {
    if points.len() < 4 {
        return points;
    }
    let last = points.len() - 1;
    points
        .iter()
        .enumerate()
        .filter(|&(idx, &p)| {
            idx == 0 || idx == last || js_point_distance(points[idx - 1], p) > DEDUP_TRESHOLD
        })
        .map(|(_, &p)| p)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_points_keep_turns() {
        let points = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [20.0, 0.0],
            [20.0, 10.0],
            [30.0, 10.0],
        ];
        assert_eq!(
            get_elbow_arrow_corner_points(points),
            [[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [30.0, 10.0]]
        );
    }

    #[test]
    fn short_segments_go() {
        let points = vec![[0.0, 0.0], [0.5, 0.0], [0.5, 10.0], [20.0, 10.0]];
        assert_eq!(
            remove_elbow_arrow_short_segments(points),
            [[0.0, 0.0], [0.5, 10.0], [20.0, 10.0]]
        );
        // fewer than four points are kept as they are
        let three = vec![[0.0, 0.0], [0.5, 0.0], [0.5, 10.0]];
        assert_eq!(remove_elbow_arrow_short_segments(three.clone()), three);
    }

    #[test]
    fn grid_is_sorted_and_deduplicated() {
        let grid = calculate_grid(
            &[[0.0, 0.0, 10.0, 10.0], [10.0, -0.0, 20.0, 5.0]],
            [10.0, 5.0],
            Heading::Right,
            [20.0, 2.0],
            Heading::Up,
            [0.0, 0.0, 20.0, 10.0],
        );
        assert_eq!((grid.col, grid.row), (3, 3));
        let xs: Vec<f64> = grid.data[..3].iter().map(|n| n.pos[0]).collect();
        let ys: Vec<f64> = grid.data.iter().step_by(3).map(|n| n.pos[1]).collect();
        assert_eq!(xs, [0.0, 10.0, 20.0]);
        assert_eq!(ys, [0.0, 5.0, 10.0]);
        assert_eq!(point_to_grid_node([20.0, 5.0], &grid), Some(5));
    }

    #[test]
    fn bends_are_estimated() {
        // heading right towards an end entered from the left, level: none
        assert_eq!(
            estimate_segment_count([0.0, 0.0], [10.0, 0.0], Heading::Right, Heading::Right),
            0.0
        );
        // already past the end: a loop around
        assert_eq!(
            estimate_segment_count([20.0, 0.0], [10.0, 0.0], Heading::Right, Heading::Right),
            4.0
        );
        assert_eq!(
            estimate_segment_count([0.0, 10.0], [10.0, 0.0], Heading::Up, Heading::Right),
            1.0
        );
    }

    #[test]
    fn normalised_update_is_relative_and_clamped() {
        let update = normalize_arrow_element_update(
            &[[5.0, 5.0], [5.0, 2_000_005.0]],
            SegmentsArg::List(&[]),
            Some(None),
            None,
        )
        .expect("points");
        assert_eq!(update.points, Some(vec![[0.0, 0.0], [0.0, 1e6]]));
        assert_eq!((update.x, update.y), (Some(5.0), Some(5.0)));
        assert_eq!((update.width, update.height), (Some(0.0), Some(1e6)));
        assert_eq!(update.fixed_segments, Some(None));
        let map = update.to_map();
        let keys: Vec<&str> = map.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "points",
                "x",
                "y",
                "fixedSegments",
                "width",
                "height",
                "startIsSpecial"
            ]
        );
        assert!(normalize_arrow_element_update(&[], SegmentsArg::List(&[]), None, None).is_err());
    }
}
