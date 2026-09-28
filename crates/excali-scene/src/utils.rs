//! Element geometry helpers from `packages/element/src/utils.ts`.

use excali_core::constants::{
    DEFAULT_ADAPTIVE_RADIUS, DEFAULT_PROPORTIONAL_RADIUS, LINE_CONFIRM_THRESHOLD,
};
use excali_core::element::{Element, ElementKind, LocalPoint, RoundnessType};
use excali_math::{curve, js, line_segment, point_from, Curve, GlobalPoint, LineSegment};

use crate::bounds::get_diamond_points;

/// `getCornerRadius(x, element)` (`utils.ts:528-549`): the corner radius
/// for a side of length `x`.
///
/// - legacy (1) and proportional (2) roundness: `x * 0.25`
///   (`DEFAULT_PROPORTIONAL_RADIUS`);
/// - adaptive (3): `x * 0.25` up to the cutoff `R / 0.25`, then the fixed
///   radius `R = roundness.value ?? 32` (`DEFAULT_ADAPTIVE_RADIUS`);
/// - no roundness: 0.
pub fn get_corner_radius(x: f64, element: &Element) -> f64 {
    let Some(roundness) = element.base.roundness else {
        return 0.0;
    };
    match roundness.kind {
        RoundnessType::ProportionalRadius | RoundnessType::Legacy => {
            x * DEFAULT_PROPORTIONAL_RADIUS
        }
        RoundnessType::AdaptiveRadius => {
            let fixed_radius_size = roundness.value.unwrap_or(DEFAULT_ADAPTIVE_RADIUS);
            let cutoff_size = fixed_radius_size / DEFAULT_PROPORTIONAL_RADIUS;
            if x <= cutoff_size {
                return x * DEFAULT_PROPORTIONAL_RADIUS;
            }
            fixed_radius_size
        }
    }
}

/// `isPathALoop(points, zoomValue)` (`utils.ts:512-526`): a path of at
/// least three points whose last point is within
/// `LINE_CONFIRM_THRESHOLD / zoom` of its first. Upstream's default zoom
/// is 1.
pub fn is_path_a_loop(points: &[LocalPoint], zoom: f64) -> bool {
    if points.len() >= 3 {
        let (first, last) = (points[0], points[points.len() - 1]);
        // pointDistance (math/src/point.ts:195-200)
        let distance = js::hypot(last[0] - first[0], last[1] - first[1]);
        return distance <= LINE_CONFIRM_THRESHOLD / zoom;
    }
    false
}

/// `STICKY_NOTE_CORNER_RADIUS_RATIO` and `STICKY_NOTE_MAX_CORNER_RADIUS`
/// (`stickyNote.ts:64-65`).
const STICKY_NOTE_CORNER_RADIUS_RATIO: f64 = 0.04;
const STICKY_NOTE_MAX_CORNER_RADIUS: f64 = 16.0;

/// `getStickyNoteCornerRadius(element)` (`stickyNote.ts:213-224`): 4 % of
/// the shorter side, at most 16; 0 without roundness.
pub fn get_sticky_note_corner_radius(element: &Element) -> f64 {
    if element.base.roundness.is_none() {
        return 0.0;
    }
    let b = &element.base;
    js::min(
        js::min(b.width, b.height) * STICKY_NOTE_CORNER_RADIUS_RATIO,
        STICKY_NOTE_MAX_CORNER_RADIUS,
    )
}

/// An outline's **unrotated** sides and corner curves (`ElementShape`,
/// `utils.ts`), as `deconstructRectanguloidElement` and
/// `deconstructDiamondElement` return them without an offset: the side
/// `i` runs from the end of corner `i` to the start of corner `i + 1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementOutline {
    pub sides: [LineSegment; 4],
    pub corners: [Curve; 4],
}

impl ElementOutline {
    /// The sides between consecutive corners (`utils.ts:357-374`,
    /// `:472-489`).
    fn from_corners(corners: [Curve; 4]) -> ElementOutline {
        let side = |a: usize, b: usize| line_segment(corners[a].3, corners[b].0);
        ElementOutline {
            sides: [side(0, 1), side(1, 2), side(2, 3), side(3, 0)],
            corners,
        }
    }
}

fn gp(x: f64, y: f64) -> GlobalPoint {
    point_from(x, y)
}

/// `deconstructRectanguloidElement(element)` (`utils.ts:245-376`) with no
/// offset: the box's sides shortened by the corner radius
/// ([`get_corner_radius`] of the shorter side, or a sticky note's
/// [`get_sticky_note_corner_radius`]; 0.01 when that is 0) and the four
/// corners as cubic curves whose handles sit 2/3 of the way to the box's
/// corner, starting top left.
pub fn deconstruct_rectanguloid_element(element: &Element) -> ElementOutline {
    let b = &element.base;
    let mut radius = if matches!(element.kind, ElementKind::StickyNote(_)) {
        get_sticky_note_corner_radius(element)
    } else {
        get_corner_radius(js::min(b.width, b.height), element)
    };
    if radius == 0.0 {
        radius = 0.01;
    }
    let r0 = [b.x, b.y];
    let r1 = [b.x + b.width, b.y + b.height];
    let top = [[r0[0] + radius, r0[1]], [r1[0] - radius, r0[1]]];
    let right = [[r1[0], r0[1] + radius], [r1[0], r1[1] - radius]];
    let bottom = [[r0[0] + radius, r1[1]], [r1[0] - radius, r1[1]]];
    let left = [[r0[0], r1[1] - radius], [r0[0], r0[1] + radius]];
    let toward = |p: [f64; 2], cx: f64, cy: f64| {
        gp(
            p[0] + (2.0 / 3.0) * (cx - p[0]),
            p[1] + (2.0 / 3.0) * (cy - p[1]),
        )
    };
    let at = |p: [f64; 2]| gp(p[0], p[1]);
    ElementOutline::from_corners([
        // TOP LEFT
        curve(
            at(left[1]),
            toward(left[1], r0[0], r0[1]),
            toward(top[0], r0[0], r0[1]),
            at(top[0]),
        ),
        // TOP RIGHT
        curve(
            at(top[1]),
            toward(top[1], r1[0], r0[1]),
            toward(right[0], r1[0], r0[1]),
            at(right[0]),
        ),
        // BOTTOM RIGHT
        curve(
            at(right[1]),
            toward(right[1], r1[0], r1[1]),
            toward(bottom[1], r1[0], r1[1]),
            at(bottom[1]),
        ),
        // BOTTOM LEFT
        curve(
            at(bottom[0]),
            toward(bottom[0], r0[0], r1[1]),
            toward(left[0], r0[0], r1[1]),
            at(left[0]),
        ),
    ])
}

/// `getDiamondBaseCorners(element)` and `deconstructDiamondElement(element)`
/// (`utils.ts:378-493`) with no offset: the corners at the right, bottom,
/// left and top vertices as curves through the vertex, their reach the
/// [`get_corner_radius`] of the half diagonals when rounded, else 1 % of
/// them.
pub fn deconstruct_diamond_element(element: &Element) -> ElementOutline {
    let b = &element.base;
    let [top_x, top_y, right_x, right_y, bottom_x, bottom_y, left_x, left_y] =
        get_diamond_points(element);
    let (v, h) = if b.roundness.is_some() {
        (
            get_corner_radius((top_x - left_x).abs(), element),
            get_corner_radius((right_y - top_y).abs(), element),
        )
    } else {
        ((top_x - left_x) * 0.01, (right_y - top_y) * 0.01)
    };
    let top = [b.x + top_x, b.y + top_y];
    let right = [b.x + right_x, b.y + right_y];
    let bottom = [b.x + bottom_x, b.y + bottom_y];
    let left = [b.x + left_x, b.y + left_y];
    let at = |p: [f64; 2]| gp(p[0], p[1]);
    ElementOutline::from_corners([
        // RIGHT
        curve(
            gp(right[0] - v, right[1] - h),
            at(right),
            at(right),
            gp(right[0] - v, right[1] + h),
        ),
        // BOTTOM
        curve(
            gp(bottom[0] + v, bottom[1] - h),
            at(bottom),
            at(bottom),
            gp(bottom[0] - v, bottom[1] - h),
        ),
        // LEFT
        curve(
            gp(left[0] + v, left[1] + h),
            at(left),
            at(left),
            gp(left[0] + v, left[1] - h),
        ),
        // TOP
        curve(
            gp(top[0] - v, top[1] + h),
            at(top),
            at(top),
            gp(top[0] + v, top[1] + h),
        ),
    ])
}
