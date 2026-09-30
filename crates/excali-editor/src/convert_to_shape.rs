//! The autoshape ("draw to shape") tool's recognizer: which shape a
//! freehand stroke reads as, and the element that replaces it.
//!
//! Upstream, at the pinned commit, `packages/element/src/convertToShape.ts`:
//! the constants (`:90-134`), `resample` (`:140-182`), the stroke features
//! (`:184-358`, on `packages/math/src/pca.ts`), the moment-based
//! classifier (`:364-438`), `getArrowEndpoint` (`:446-495`),
//! `recognizeShape` (`:502-527`) and `convertToShape` (`:535-728`).
//!
//! A moment-based recognizer: the stroke is resampled to 64 evenly spaced
//! points, an open stroke is a line or an arrow when it is straight enough
//! (the arrowhead skews the points along the major axis), and a closed one
//! is the rectangle, diamond or ellipse whose feature prototype it sits
//! closest to. Rectangle vs. diamond is deliberately not rotation
//! invariant: a rectangle turned 45° is a diamond.

use excali_core::app_state::AppState;
use excali_core::element::{Element, ElementType};
use excali_math::{
    convex_hull, distance_to_line_segment, elongation, js, kurtosis, line_segment,
    orient_principal_axes, point_from, polygon_area, principal_axes, principal_coords, skewness,
    GlobalPoint,
};
use excali_scene::bounds::{
    get_bounds_from_points, get_element_absolute_coords, Bounds, ElementsMap,
};
use serde_json::{json, Map, Value};

use crate::new_element::{
    current_stroke_width, is_using_adaptive_radius, new_element_base, ADAPTIVE_RADIUS,
    PROPORTIONAL_RADIUS,
};

/// Number of points every stroke is resampled to before feature
/// extraction.
const RESAMPLE_N: usize = 64;

/// Minimum apparent (on-screen) size of a stroke's larger bounding-box
/// dimension, in pixels, for recognition to run; compared against
/// `scene size * zoom`.
const RECOGNITION_MIN_SCREEN_SIZE: f64 = 25.0;

/// A stroke whose endpoints are farther apart than this fraction of its
/// path length is open (a line or an arrow), else closed.
const CLOSED_GAP_MAX_RATIO: f64 = 0.15;

/// Maximum elongation for an open stroke to count as straight.
const LINEAR_MAX_ELONGATION: f64 = 0.25;

/// Fraction of the start→tip distance around the tip inside which an
/// arrowhead may live, exempt from the shaft straightness check.
const ARROWHEAD_ZONE_RATIO: f64 = 0.5;

/// Maximum distance any point outside the arrowhead zone may stray from the
/// start→tip chord, as a fraction of the chord length.
const LINEAR_MAX_SHAFT_DEVIATION: f64 = 0.15;

/// Minimum |skew| along the major axis for an open, straight stroke to be
/// an arrow rather than a line.
const ARROW_MIN_SKEW: f64 = 0.3;

/// Maximum feature-space distance to a closed shape prototype, in units of
/// the per-feature tolerances.
const CLOSED_SHAPE_MAX_DISTANCE: f64 = 1.5;

/// Half-width, in resampled points, of the chord window local turning is
/// measured over.
const TURN_WINDOW: usize = 3;

const HULL_FILL_RATIO_TOLERANCE: f64 = 0.2;
const CORNER_TURN_SHARE_TOLERANCE: f64 = 0.2;
const KURTOSIS_PRODUCT_TOLERANCE: f64 = 0.7;

/// An arrow shorter than this becomes a line (`convertToShape`).
const MIN_ARROW_LENGTH: f64 = 60.0;

/// `ELEMENT_PENDING_DRAW_SHAPE_OPACITY` (`common/src/constants.ts:438`):
/// the opacity of the live preview while the stroke is drawn.
pub const ELEMENT_PENDING_DRAW_SHAPE_OPACITY: f64 = 70.0;

/// The shape a stroke is recognized as; `Freedraw` when none matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecognizedShape {
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    Arrow,
    Freedraw,
}

impl RecognizedShape {
    /// The element type name upstream uses.
    pub const fn as_str(self) -> &'static str {
        match self {
            RecognizedShape::Rectangle => "rectangle",
            RecognizedShape::Diamond => "diamond",
            RecognizedShape::Ellipse => "ellipse",
            RecognizedShape::Line => "line",
            RecognizedShape::Arrow => "arrow",
            RecognizedShape::Freedraw => "freedraw",
        }
    }
}

/// `ShapeRecognitionResult`: the recognized shape and the bounding box of
/// the input points (the converted shape's position).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeRecognitionResult {
    pub shape: RecognizedShape,
    pub bounding_box: Bounds,
}

/// `StrokeFeatures`: a statistical description of a stroke that does not
/// depend on where, how big or (except where noted) how rotated it was
/// drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeFeatures {
    /// Endpoint distance over path length; ~0 when the stroke returns to
    /// its start.
    pub gap_ratio: f64,
    /// Point cloud elongation along the major axis.
    pub elongation: f64,
    /// Skew of the points projected onto the (oriented) major axis, `<= 0`
    /// by construction.
    pub major_skew: f64,
    /// Convex hull area over bounding box area, in the screen frame.
    pub hull_fill_ratio: f64,
    /// Share of the total turning in the 4 strongest corners.
    pub corner_turn_share: f64,
    /// Kurtosis along x times kurtosis along y, in the screen frame.
    pub kurtosis_product: f64,
    /// Largest deviation from the start→tip chord outside the arrowhead
    /// zone, over the chord length.
    pub shaft_deviation_ratio: f64,
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    js::hypot(a[0] - b[0], a[1] - b[1])
}

/// `resample(pts, n)`: `n` points spaced along the stroke path, as upstream
/// computes them (a segment that takes several points interpolates each
/// from the previous inserted point by the same `t` fraction of the
/// original segment).
fn resample(pts: &[[f64; 2]], n: usize) -> Vec<[f64; 2]> {
    let mut total_len = 0.0;
    for i in 1..pts.len() {
        total_len += distance(pts[i], pts[i - 1]);
    }

    let interval = total_len / (n - 1) as f64;
    let mut accumulated = 0.0;
    let mut result = vec![pts[0]];
    let mut prev = pts[0];

    for &curr in &pts[1..] {
        let seg_len = distance(curr, prev);
        if accumulated + seg_len >= interval {
            let mut remaining = interval - accumulated;
            while remaining <= seg_len + 1e-10 {
                let t = remaining / seg_len;
                let new_pt = [
                    prev[0] + t * (curr[0] - prev[0]),
                    prev[1] + t * (curr[1] - prev[1]),
                ];
                result.push(new_pt);
                if result.len() == n {
                    return result;
                }
                prev = new_pt;
                // (upstream also zeroes `accumulated` here; the assignment
                // after the loop always replaces it)
                remaining += interval;
            }
            accumulated = seg_len - (remaining - interval);
        } else {
            accumulated += seg_len;
        }
        prev = curr;
    }

    while result.len() < n {
        result.push(pts[pts.len() - 1]);
    }
    result
}

fn global(p: [f64; 2]) -> GlobalPoint {
    point_from(p[0], p[1])
}

/// `shaftDeviationRatio(pts)`.
fn shaft_deviation_ratio(pts: &[[f64; 2]]) -> f64 {
    let start = pts[0];
    let mut tip = start;
    let mut tip_distance = 0.0;
    for &point in pts {
        let d = distance(point, start);
        if d > tip_distance {
            tip_distance = d;
            tip = point;
        }
    }
    if tip_distance == 0.0 {
        return 0.0;
    }

    let chord = line_segment(global(start), global(tip));
    let mut max_deviation: f64 = 0.0;
    for &point in pts {
        if distance(point, tip) <= ARROWHEAD_ZONE_RATIO * tip_distance {
            continue;
        }
        max_deviation = js::max(
            max_deviation,
            distance_to_line_segment(global(point), chord),
        );
    }
    max_deviation / tip_distance
}

/// `windowedTurns(pts)`: the turn angle at every point `TURN_WINDOW` from
/// either end, between the chords to the points `TURN_WINDOW` away (the
/// stroke is not wrapped around).
fn windowed_turns(pts: &[[f64; 2]]) -> Vec<f64> {
    let mut turns = Vec::new();
    for i in TURN_WINDOW..pts.len().saturating_sub(TURN_WINDOW) {
        let [ax, ay] = pts[i - TURN_WINDOW];
        let [bx, by] = pts[i];
        let [cx, cy] = pts[i + TURN_WINDOW];
        let (v1x, v1y) = (bx - ax, by - ay);
        let (v2x, v2y) = (cx - bx, cy - by);
        turns.push(js::atan2(v1x * v2y - v1y * v2x, v1x * v2x + v1y * v2y).abs());
    }
    turns
}

/// `cornerTurnShare(pts)`: greedily the 4 strongest turn peaks, each
/// taking its ±`TURN_WINDOW` neighbourhood, over the total turning.
fn corner_turn_share(pts: &[[f64; 2]]) -> f64 {
    let turns = windowed_turns(pts);
    let total = turns.iter().fold(0.0, |sum, turn| sum + turn);
    if total == 0.0 {
        return 0.0;
    }

    let mut taken = vec![false; turns.len()];
    let mut top4 = 0.0;
    for _ in 0..4 {
        let mut peak = None;
        let mut peak_turn = 0.0;
        for (i, &turn) in turns.iter().enumerate() {
            if !taken[i] && turn > peak_turn {
                peak_turn = turn;
                peak = Some(i);
            }
        }
        let Some(peak) = peak else { break };
        let from = peak.saturating_sub(TURN_WINDOW);
        let to = (peak + TURN_WINDOW).min(turns.len() - 1);
        for i in from..=to {
            if !taken[i] {
                top4 += turns[i];
                taken[i] = true;
            }
        }
    }
    top4 / total
}

/// `extractFeatures(points)`: the features of the stroke, resampled first
/// so pointer speed does not bias the moments.
pub fn extract_features(points: &[[f64; 2]]) -> StrokeFeatures {
    let pts = resample(points, RESAMPLE_N);

    let mut path_length = 0.0;
    for i in 1..pts.len() {
        path_length += distance(pts[i], pts[i - 1]);
    }
    let gap = distance(pts[pts.len() - 1], pts[0]);

    let cloud: Vec<GlobalPoint> = pts.iter().copied().map(global).collect();
    let axes = orient_principal_axes(&cloud, &principal_axes(&cloud));
    let major_projection: Vec<f64> = principal_coords(&cloud, &axes)
        .iter()
        .map(|[u, _]| *u)
        .collect();

    let hull = convex_hull(&cloud);
    let [min_x, min_y, max_x, max_y] = get_bounds_from_points(&pts, 0.0);
    let box_area = (max_x - min_x) * (max_y - min_y);

    let xs: Vec<f64> = pts.iter().map(|p| p[0]).collect();
    let ys: Vec<f64> = pts.iter().map(|p| p[1]).collect();
    StrokeFeatures {
        gap_ratio: if path_length > 0.0 {
            gap / path_length
        } else {
            0.0
        },
        elongation: elongation(&axes),
        major_skew: skewness(&major_projection),
        hull_fill_ratio: if box_area > 0.0 {
            polygon_area(&hull) / box_area
        } else {
            0.0
        },
        corner_turn_share: corner_turn_share(&pts),
        kurtosis_product: kurtosis(&xs) * kurtosis(&ys),
        shaft_deviation_ratio: shaft_deviation_ratio(&pts),
    }
}

/// `CLOSED_SHAPE_PROTOTYPES`: shape, hull fill ratio, corner turn share,
/// kurtosis product.
const CLOSED_SHAPE_PROTOTYPES: [(RecognizedShape, f64, f64, f64); 3] = [
    (RecognizedShape::Rectangle, 1.0, 0.95, 1.83),
    (RecognizedShape::Diamond, 0.5, 0.95, 3.24),
    (
        RecognizedShape::Ellipse,
        std::f64::consts::PI / 4.0,
        0.55,
        2.25,
    ),
];

/// `classifyClosedStroke(features)`: the nearest prototype, or freedraw
/// when even that is too far.
fn classify_closed_stroke(features: &StrokeFeatures) -> RecognizedShape {
    let mut best = RecognizedShape::Freedraw;
    let mut best_distance = CLOSED_SHAPE_MAX_DISTANCE;
    for (shape, hull_fill_ratio, corner_turn_share, kurtosis_product) in CLOSED_SHAPE_PROTOTYPES {
        let distance = js::hypot_n(&[
            (features.hull_fill_ratio - hull_fill_ratio) / HULL_FILL_RATIO_TOLERANCE,
            (features.corner_turn_share - corner_turn_share) / CORNER_TURN_SHARE_TOLERANCE,
            (features.kurtosis_product - kurtosis_product) / KURTOSIS_PRODUCT_TOLERANCE,
        ]);
        if distance < best_distance {
            best_distance = distance;
            best = shape;
        }
    }
    best
}

/// `classifyOpenStroke(features)`: a line or an arrow when straight enough.
fn classify_open_stroke(features: &StrokeFeatures) -> RecognizedShape {
    if features.elongation > LINEAR_MAX_ELONGATION
        || features.shaft_deviation_ratio > LINEAR_MAX_SHAFT_DEVIATION
    {
        return RecognizedShape::Freedraw;
    }
    if features.major_skew.abs() >= ARROW_MIN_SKEW {
        RecognizedShape::Arrow
    } else {
        RecognizedShape::Line
    }
}

/// `classify(features)`.
pub fn classify(features: &StrokeFeatures) -> RecognizedShape {
    if features.gap_ratio > CLOSED_GAP_MAX_RATIO {
        classify_open_stroke(features)
    } else {
        classify_closed_stroke(features)
    }
}

/// `getArrowEndpoint(points, boundingBox, startPoint)`: the input point
/// closest to the box perimeter point (corners and edge midpoints)
/// farthest from the start.
fn get_arrow_endpoint(points: &[[f64; 2]], bounding_box: Bounds, start: [f64; 2]) -> [f64; 2] {
    let [min_x, min_y, max_x, max_y] = bounding_box;
    let w = max_x - min_x;
    let h = max_y - min_y;
    let last = points[points.len() - 1];
    if w == 0.0 && h == 0.0 {
        return last;
    }

    let mid_x = (min_x + max_x) / 2.0;
    let mid_y = (min_y + max_y) / 2.0;
    let perimeter = [
        [min_x, min_y],
        [mid_x, min_y],
        [max_x, min_y],
        [max_x, mid_y],
        [max_x, max_y],
        [mid_x, max_y],
        [min_x, max_y],
        [min_x, mid_y],
    ];

    let mut ideal_dist = -1.0;
    let mut ideal = [max_x, max_y];
    for pp in perimeter {
        let d = distance(pp, start);
        if d > ideal_dist {
            ideal_dist = d;
            ideal = pp;
        }
    }

    let mut best_dist = f64::INFINITY;
    let mut best = last;
    for &pt in points {
        let d = distance(pt, ideal);
        if d < best_dist {
            best_dist = d;
            best = pt;
        }
    }
    best
}

/// `recognizeShape(points, previousElement, zoom)`: the shape the stroke
/// `points` (scene coordinates) reads as at `zoom`. `previous` is the type
/// of the element the stroke continues, if any: after an arrow only an
/// arrow is recognized.
pub fn recognize_shape(
    points: &[[f64; 2]],
    previous: Option<ElementType>,
    zoom: f64,
) -> ShapeRecognitionResult {
    let bounding_box = get_bounds_from_points(points, 0.0);
    let [min_x, min_y, max_x, max_y] = bounding_box;
    let max_dim = js::max(max_x - min_x, max_y - min_y);
    if points.len() < 3 || max_dim * zoom < RECOGNITION_MIN_SCREEN_SIZE {
        return ShapeRecognitionResult {
            shape: RecognizedShape::Freedraw,
            bounding_box,
        };
    }

    let recognized = classify(&extract_features(points));
    let shape = if previous == Some(ElementType::Arrow) && recognized != RecognizedShape::Arrow {
        RecognizedShape::Freedraw
    } else {
        recognized
    };
    ShapeRecognitionResult {
        shape,
        bounding_box,
    }
}

/// `convertToShape`'s arguments.
#[derive(Clone, Copy)]
pub struct ConvertToShape<'a> {
    /// The stroke, in scene coordinates.
    pub points: &'a [[f64; 2]],
    /// The app state: `zoom` and the `currentItem*` keys the new element
    /// takes.
    pub app_state: &'a AppState,
    pub elements_map: &'a ElementsMap<'a>,
    /// The scene's frame-like elements (`getFrameLikeElements`); the
    /// first that holds the stroke's box becomes the element's frame.
    pub frames: &'a [&'a Element],
    /// The type of the element the stroke continues, if any.
    pub previous: Option<ElementType>,
    /// The new element's `randomId()`, `randomInteger()` and
    /// `getUpdatedTimestamp()`.
    pub id: &'a str,
    pub seed: f64,
    pub timestamp: f64,
}

/// `convertToShape(points, appState, elementsMap, previousElement,
/// frames)`: the element the recognized shape becomes, `None` for
/// freedraw. A rectangle, diamond or ellipse fills the stroke's box; a
/// line runs from the first to the last point; an arrow from the first
/// point to [`get_arrow_endpoint`]'s tip, or is a line when shorter than
/// 60. Lines and arrows keep upstream's zero `width` and `height`.
pub fn convert_to_shape(args: &ConvertToShape<'_>) -> Option<Element> {
    let app_state = args.app_state;
    let zoom = app_state
        .get("zoom")
        .and_then(|z| z.get("value"))
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    let recognized = recognize_shape(args.points, args.previous, zoom);
    let [min_x, min_y, max_x, max_y] = recognized.bounding_box;

    let frame_id = args
        .frames
        .iter()
        .find(|frame| {
            let [fx1, fy1, fx2, fy2, ..] =
                get_element_absolute_coords(frame, args.elements_map, false);
            fx1 <= min_x && fy1 <= min_y && fx2 >= max_x && fy2 >= max_y
        })
        .map(|frame| frame.base.id.clone());

    let shape = recognized.shape;
    let ty = ElementType::parse(shape.as_str())?;
    let round = app_state
        .get("currentItemRoundness")
        .and_then(Value::as_str)
        == Some("round");
    let roundness = if round {
        let kind = if is_using_adaptive_radius(shape.as_str()) {
            ADAPTIVE_RADIUS
        } else {
            PROPORTIONAL_RADIUS
        };
        json!({ "type": kind })
    } else {
        Value::Null
    };

    let mut opts = Map::new();
    opts.insert(
        "frameId".into(),
        frame_id.map_or(Value::Null, Value::String),
    );
    opts.insert("roundness".into(), roundness);
    for (opt, key) in [
        ("roughness", "currentItemRoughness"),
        ("backgroundColor", "currentItemBackgroundColor"),
        ("strokeColor", "currentItemStrokeColor"),
        ("fillStyle", "currentItemFillStyle"),
        ("opacity", "currentItemOpacity"),
        ("strokeStyle", "currentItemStrokeStyle"),
    ] {
        if let Some(value) = app_state.get(key) {
            opts.insert(opt.into(), value.clone());
        }
    }
    opts.insert(
        "strokeWidth".into(),
        json!(current_stroke_width(ty, app_state)),
    );
    let base = |ty: &str, opts: &Map<String, Value>| {
        new_element_base(ty, opts, args.id, args.seed, args.timestamp)
    };

    let first = args.points[0];
    let m = match shape {
        RecognizedShape::Freedraw => return None,
        RecognizedShape::Rectangle | RecognizedShape::Diamond | RecognizedShape::Ellipse => {
            opts.insert("x".into(), json!(min_x));
            opts.insert("y".into(), json!(min_y));
            opts.insert("width".into(), json!(max_x - min_x));
            opts.insert("height".into(), json!(max_y - min_y));
            base(shape.as_str(), &opts)
        }
        RecognizedShape::Arrow | RecognizedShape::Line => {
            let end = if shape == RecognizedShape::Arrow {
                get_arrow_endpoint(args.points, recognized.bounding_box, first)
            } else {
                args.points[args.points.len() - 1]
            };
            let [x, y] = first;
            let points = [[first[0] - x, first[1] - y], [end[0] - x, end[1] - y]];
            // getNormalizeElementPointsAndCoords: offset by the first point
            let [ox, oy] = points[0];
            opts.insert("x".into(), json!(x + ox));
            opts.insert("y".into(), json!(y + oy));
            opts.insert("locked".into(), json!(false));
            let normalized: Vec<[f64; 2]> = points.iter().map(|p| [p[0] - ox, p[1] - oy]).collect();
            let as_arrow =
                shape == RecognizedShape::Arrow && distance(end, first) >= MIN_ARROW_LENGTH;
            let mut m = base(if as_arrow { "arrow" } else { "line" }, &opts);
            m.insert("points".into(), json!(normalized));
            m.insert("startBinding".into(), Value::Null);
            m.insert("endBinding".into(), Value::Null);
            if as_arrow {
                let arrowhead = |key: &str| match app_state.get(key) {
                    Some(Value::String(s)) if !s.is_empty() => Value::String(s.clone()),
                    _ => Value::Null,
                };
                m.insert(
                    "startArrowhead".into(),
                    arrowhead("currentItemStartArrowhead"),
                );
                m.insert("endArrowhead".into(), arrowhead("currentItemEndArrowhead"));
                m.insert("elbowed".into(), json!(false));
            } else {
                m.insert("startArrowhead".into(), Value::Null);
                m.insert("endArrowhead".into(), Value::Null);
                m.insert("polygon".into(), json!(false));
            }
            m
        }
    };
    Element::from_map(m).ok()
}
