//! Object snapping (`packages/excalidraw/snapping.ts`) and the snap lines
//! the interactive canvas draws (`renderer/renderSnaps.ts`).
//!
//! While elements are dragged, resized or drawn, their corners (and centre,
//! and the side midpoints of diamonds and ellipses) snap to the corners of
//! the other visible elements within [`get_snap_distance`] (8 / zoom), and a
//! dragged selection snaps to repeat the gaps between visible elements.
//! Each call returns the offset to apply and the [`SnapLine`]s to show;
//! [`render_snaps`] turns those into canvas calls.
//!
//! Upstream keeps the reference points and gaps in a static `SnapCache`
//! that `App` fills when a drag starts and empties when it ends; here the
//! editor owns a [`SnapCache`] and passes it in.

use std::collections::HashSet;

use excali_core::element::{Element, ElementKind};
use excali_math::{js, point_from, point_rotate_rads, range_inclusive, range_intersection};
use excali_math::{ranges_overlap, GlobalPoint, Radians};
use excali_scene::bounds::{
    get_bound_text_element, get_common_bounds, get_element_absolute_coords, get_element_bounds,
    Bounds, ElementsMap,
};
use excali_scene::shape::Theme;
use indexmap::IndexMap;

use crate::tools::ToolType;
use crate::transform_handles::TransformHandleType;
use crate::viewport::{viewport_coords_to_scene_coords, ViewportState};

type P = [f64; 2];

/// `SNAP_DISTANCE` (`snapping.ts:41`), in screen pixels.
pub const SNAP_DISTANCE: f64 = 8.0;

/// `VISIBLE_GAPS_LIMIT_PER_AXIS` (`snapping.ts:45`).
const VISIBLE_GAPS_LIMIT_PER_AXIS: usize = 99999;

/// `SNAP_COLOR_LIGHT` (`renderSnaps.ts:8`).
pub const SNAP_COLOR_LIGHT: &str = "#ff6b6b";
/// `SNAP_COLOR_DARK` (`renderSnaps.ts:11`).
pub const SNAP_COLOR_DARK: &str = "#ff9090";
/// `SNAP_COLOR_DARK_ZEN` (`renderSnaps.ts:12`).
pub const SNAP_COLOR_DARK_ZEN: &str = "#da5b5b";
/// `SNAP_WIDTH` (`renderSnaps.ts:13`).
const SNAP_WIDTH: f64 = 1.0;
/// `SNAP_CROSS_SIZE` (`renderSnaps.ts:14`).
const SNAP_CROSS_SIZE: f64 = 2.0;

/// `getSnapDistance(zoomValue)` (`snapping.ts:48-50`): 8 / zoom, in scene
/// units.
pub fn get_snap_distance(zoom: f64) -> f64 {
    SNAP_DISTANCE / zoom
}

// -- types ---------------------------------------------------------------------------

/// `Gap` (`snapping.ts:64-81`): the space between two reference elements'
/// boxes that overlap on the other axis.
#[derive(Debug, Clone, PartialEq)]
pub struct Gap {
    pub start_bounds: Bounds,
    pub end_bounds: Bounds,
    pub start_side: [P; 2],
    pub end_side: [P; 2],
    pub overlap: [f64; 2],
    pub length: f64,
}

/// `getVisibleGaps`' result: gaps between boxes side by side
/// (`horizontalGaps`) and one above the other (`verticalGaps`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VisibleGaps {
    pub horizontal_gaps: Vec<Gap>,
    pub vertical_gaps: Vec<Gap>,
}

/// A snap line's direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapLineDirection {
    Horizontal,
    Vertical,
}

/// `SnapLine` (`snapping.ts:101-118`).
#[derive(Debug, Clone, PartialEq)]
pub enum SnapLine {
    /// `PointSnapLine`: the snapped points along one line.
    Points { points: Vec<P> },
    /// `PointerSnapLine`: from a reference corner to the pointer.
    Pointer {
        points: [P; 2],
        direction: SnapLineDirection,
    },
    /// `GapSnapLine`: one of the equal gaps.
    Gap {
        direction: SnapLineDirection,
        points: [P; 2],
    },
}

/// `GapSnap["direction"]` (`snapping.ts:85-91`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GapSnapDirection {
    CenterHorizontal,
    CenterVertical,
    SideLeft,
    SideRight,
    SideTop,
    SideBottom,
}

/// `Snap` (`snapping.ts:58-97`): a point snap or a gap snap.
#[derive(Debug, Clone, Copy)]
enum Snap<'g> {
    Point {
        points: [P; 2],
        offset: f64,
    },
    Gap {
        direction: GapSnapDirection,
        gap: &'g Gap,
        offset: f64,
    },
}

impl Snap<'_> {
    fn offset(&self) -> f64 {
        match *self {
            Snap::Point { offset, .. } | Snap::Gap { offset, .. } => offset,
        }
    }
}

/// `SnapCache` (`snapping.ts:122-155`): the reference points and visible
/// gaps of a drag, computed once when it starts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnapCache {
    pub reference_snap_points: Option<Vec<P>>,
    pub visible_gaps: Option<VisibleGaps>,
}

impl SnapCache {
    /// `SnapCache.destroy()`.
    pub fn destroy(&mut self) {
        self.reference_snap_points = None;
        self.visible_gaps = None;
    }

    /// `App.maybeCacheReferenceSnapPoints(event, selectedElements)`
    /// (`App.tsx:10635-10657`, `recomputeAnyways` false): fills the
    /// reference points when snapping is on and they are not cached yet.
    /// `elements` are the scene's non-deleted elements.
    pub fn maybe_cache_reference_snap_points(
        &mut self,
        state: &SnapAppState,
        event: Option<SnapEvent>,
        selected_elements: &[&Element],
        elements: &[&Element],
        elements_map: &ElementsMap<'_>,
    ) {
        if is_snapping_enabled(state, event, selected_elements)
            && self.reference_snap_points.is_none()
        {
            self.reference_snap_points = Some(get_reference_snap_points(
                elements,
                selected_elements,
                state,
                elements_map,
            ));
        }
    }

    /// `App.maybeCacheVisibleGaps(event, selectedElements)`
    /// (`App.tsx:10659-10680`, `recomputeAnyways` false).
    pub fn maybe_cache_visible_gaps(
        &mut self,
        state: &SnapAppState,
        event: Option<SnapEvent>,
        selected_elements: &[&Element],
        elements: &[&Element],
        elements_map: &ElementsMap<'_>,
    ) {
        if is_snapping_enabled(state, event, selected_elements) && self.visible_gaps.is_none() {
            self.visible_gaps = Some(get_visible_gaps(
                elements,
                selected_elements,
                state,
                elements_map,
            ));
        }
    }
}

/// The app state snapping reads.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapAppState {
    /// Scroll, `zoom.value`, the canvas size and offset: which elements are
    /// visible, and the snap distance.
    pub viewport: ViewportState,
    pub objects_snap_mode_enabled: bool,
    /// `isGridModeEnabled(app)`: `props.gridModeEnabled ??
    /// state.gridModeEnabled`.
    pub grid_mode_enabled: bool,
    /// `activeTool.type`.
    pub active_tool: ToolType,
    pub selected_elements_are_being_dragged: bool,
}

/// The modifiers snapping reads (`KeyboardModifiersObject`):
/// `event[KEYS.CTRL_OR_CMD]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapEvent {
    pub ctrl_or_cmd: bool,
}

/// What a snap returns: the offset to add to the gesture, and the lines to
/// draw.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapResult {
    pub snap_offset: P,
    pub snap_lines: Vec<SnapLine>,
}

/// `getSnapLinesAtPointer`'s result: the offset from the pointer to the
/// snapped corner, and the lines to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct PointerSnapResult {
    pub origin_offset: P,
    pub snap_lines: Vec<SnapLine>,
}

impl SnapResult {
    fn none() -> SnapResult {
        SnapResult {
            snap_offset: [0.0, 0.0],
            snap_lines: Vec::new(),
        }
    }
}

// -- predicates ----------------------------------------------------------------------

/// `isSnappingEnabled({ event, app, selectedElements })`
/// (`snapping.ts:162-190`). With an event: objects snap mode on and
/// Ctrl/Cmd up, or the mode off, Ctrl/Cmd down and no grid; never while
/// the lasso selects (only while it drags). Without one: the mode, except
/// for a lone arrow, which gives way to binding.
pub fn is_snapping_enabled(
    state: &SnapAppState,
    event: Option<SnapEvent>,
    selected_elements: &[&Element],
) -> bool {
    if let Some(event) = event {
        let is_lasso_dragging =
            state.active_tool == ToolType::Lasso && state.selected_elements_are_being_dragged;
        return (state.active_tool != ToolType::Lasso || is_lasso_dragging)
            && ((state.objects_snap_mode_enabled && !event.ctrl_or_cmd)
                || (!state.objects_snap_mode_enabled
                    && event.ctrl_or_cmd
                    && !state.grid_mode_enabled));
    }
    if selected_elements.len() == 1 && matches!(selected_elements[0].kind, ElementKind::Arrow(_)) {
        return false;
    }
    state.objects_snap_mode_enabled
}

/// `areRoughlyEqual(a, b, precision = 0.01)` (`snapping.ts:192-194`).
pub fn are_roughly_equal(a: f64, b: f64, precision: f64) -> bool {
    (a - b).abs() <= precision
}

/// `isActiveToolNonLinearSnappable(activeToolType)`
/// (`snapping.ts:1400-1414`): the tools whose pointer snaps before a drag.
pub fn is_active_tool_non_linear_snappable(tool: ToolType) -> bool {
    matches!(
        tool,
        ToolType::Rectangle
            | ToolType::Ellipse
            | ToolType::Diamond
            | ToolType::Frame
            | ToolType::Magicframe
            | ToolType::Image
            | ToolType::Text
    )
}

// -- helpers -------------------------------------------------------------------------

/// `round(x)` (`snapping.ts:866-869`): to 6 decimal places.
fn round(x: f64) -> f64 {
    js::round(x * 1e6) / 1e6
}

fn rotate(p: P, center: P, angle: f64) -> P {
    let r: GlobalPoint = point_rotate_rads(
        point_from(p[0], p[1]),
        point_from(center[0], center[1]),
        Radians(angle),
    );
    [r.x, r.y]
}

/// `getDraggedElementsBounds(elements, dragOffset)` (`bounds.ts:1031-1042`).
fn get_dragged_elements_bounds(elements: &[&Element], drag_offset: P) -> Bounds {
    let [min_x, min_y, max_x, max_y] = get_common_bounds(elements);
    [
        min_x + drag_offset[0],
        min_y + drag_offset[1],
        max_x + drag_offset[0],
        max_y + drag_offset[1],
    ]
}

/// `isBoundToContainer(element)` (`typeChecks.ts:307-316`).
fn is_bound_to_container(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Text(t) if t.container_id.is_some())
}

/// `isElementInViewport(element, width, height, appState, elementsMap)`
/// (`sizeHelpers.ts:80-115`).
fn is_element_in_viewport(
    element: &Element,
    viewport: &ViewportState,
    elements_map: &ElementsMap<'_>,
) -> bool {
    let [x1, y1, x2, y2] = get_element_bounds(element, elements_map);
    let top_left =
        viewport_coords_to_scene_coords(viewport.offset_left, viewport.offset_top, viewport);
    let bottom_right = viewport_coords_to_scene_coords(
        viewport.offset_left + viewport.width,
        viewport.offset_top + viewport.height,
        viewport,
    );
    top_left.0 <= x2 && top_left.1 <= y2 && bottom_right.0 >= x1 && bottom_right.1 >= y1
}

/// `getVisibleAndNonSelectedElements(elements, selectedElements, appState,
/// elementsMap)` (`selection.ts:101-123`).
fn get_reference_elements<'e>(
    elements: &[&'e Element],
    selected_elements: &[&Element],
    state: &SnapAppState,
    elements_map: &ElementsMap<'_>,
) -> Vec<&'e Element> {
    let selected: HashSet<&str> = selected_elements
        .iter()
        .map(|e| e.base.id.as_str())
        .collect();
    elements
        .iter()
        .copied()
        .filter(|e| {
            let visible = is_element_in_viewport(e, &state.viewport, elements_map);
            !selected.contains(e.base.id.as_str()) && visible
        })
        .collect()
}

/// `getMaximumGroups(elements, elementsMap)` (`groups.ts:332-356`): the
/// elements by outermost group (or alone), each preceded by its bound text.
fn get_maximum_groups<'e>(
    elements: &[&'e Element],
    elements_map: &ElementsMap<'e>,
) -> Vec<Vec<&'e Element>> {
    let mut groups: IndexMap<&str, Vec<&'e Element>> = IndexMap::new();
    for &element in elements {
        let group_id = element
            .base
            .group_ids
            .last()
            .map_or(element.base.id.as_str(), String::as_str);
        let members = groups.entry(group_id).or_default();
        if let Some(text) = get_bound_text_element(element, elements_map) {
            members.push(text);
        }
        members.push(element);
    }
    groups.into_values().collect()
}

/// The groups snapping refers to: every maximum group but a lone bound
/// text.
fn reference_groups<'e>(
    elements: &[&'e Element],
    selected_elements: &[&Element],
    state: &SnapAppState,
    elements_map: &ElementsMap<'e>,
) -> Vec<Vec<&'e Element>> {
    let reference = get_reference_elements(elements, selected_elements, state, elements_map);
    get_maximum_groups(&reference, elements_map)
        .into_iter()
        .filter(|group| !(group.len() == 1 && is_bound_to_container(group[0])))
        .collect()
}

// -- corners, reference points, gaps ---------------------------------------------------

/// `getElementsCorners`' options.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CornersOptions {
    pub omit_center: bool,
    pub bounding_box_corners: bool,
    pub drag_offset: Option<P>,
}

/// `getElementsCorners(elements, elementsMap, opts)` (`snapping.ts:196-303`):
/// a lone element's corners (the side midpoints of a diamond or ellipse,
/// unless `bounding_box_corners`) turned with it, several elements' common
/// box corners, and the centre unless `omit_center`; rounded to 6 places.
pub fn get_elements_corners(
    elements: &[&Element],
    elements_map: &ElementsMap<'_>,
    opts: CornersOptions,
) -> Vec<P> {
    let mut result: Vec<P> = Vec::new();
    if elements.len() == 1 {
        let element = elements[0];
        let [mut x1, mut y1, mut x2, mut y2, mut cx, mut cy] =
            get_element_absolute_coords(element, elements_map, false);
        if let Some([dx, dy]) = opts.drag_offset {
            x1 += dx;
            x2 += dx;
            cx += dx;
            y1 += dy;
            y2 += dy;
            cy += dy;
        }
        let half_width = (x2 - x1) / 2.0;
        let half_height = (y2 - y1) / 2.0;
        let center = [cx, cy];
        let angle = element.base.angle.0;
        let corners: [P; 4] = if matches!(element.kind, ElementKind::Diamond | ElementKind::Ellipse)
            && !opts.bounding_box_corners
        {
            [
                rotate([x1, y1 + half_height], center, angle),
                rotate([x1 + half_width, y1], center, angle),
                rotate([x2, y1 + half_height], center, angle),
                rotate([x1 + half_width, y2], center, angle),
            ]
        } else {
            [
                rotate([x1, y1], center, angle),
                rotate([x2, y1], center, angle),
                rotate([x1, y2], center, angle),
                rotate([x2, y2], center, angle),
            ]
        };
        result.extend(corners);
        if !opts.omit_center {
            result.push(center);
        }
    } else if elements.len() > 1 {
        let [min_x, min_y, max_x, max_y] =
            get_dragged_elements_bounds(elements, opts.drag_offset.unwrap_or([0.0, 0.0]));
        let width = max_x - min_x;
        let height = max_y - min_y;
        result.extend([
            [min_x, min_y],
            [max_x, min_y],
            [min_x, max_y],
            [max_x, max_y],
        ]);
        if !opts.omit_center {
            result.push([min_x + width / 2.0, min_y + height / 2.0]);
        }
    }
    result
        .into_iter()
        .map(|p| [round(p[0]), round(p[1])])
        .collect()
}

/// `getReferenceSnapPoints(elements, selectedElements, appState,
/// elementsMap)` (`snapping.ts:621-640`): the corners of every visible,
/// unselected group.
pub fn get_reference_snap_points<'e>(
    elements: &[&'e Element],
    selected_elements: &[&Element],
    state: &SnapAppState,
    elements_map: &ElementsMap<'e>,
) -> Vec<P> {
    reference_groups(elements, selected_elements, state, elements_map)
        .iter()
        .flat_map(|group| get_elements_corners(group, elements_map, CornersOptions::default()))
        .collect()
}

/// `getVisibleGaps(elements, selectedElements, appState, elementsMap)`
/// (`snapping.ts:318-444`): the gaps between the boxes of the visible,
/// unselected groups.
pub fn get_visible_gaps<'e>(
    elements: &[&'e Element],
    selected_elements: &[&Element],
    state: &SnapAppState,
    elements_map: &ElementsMap<'e>,
) -> VisibleGaps {
    let mut bounds: Vec<Bounds> =
        reference_groups(elements, selected_elements, state, elements_map)
            .iter()
            .map(|group| get_common_bounds(group).map(round))
            .collect();

    let by = |i: usize| {
        move |a: &Bounds, b: &Bounds| {
            (a[i] - b[i])
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    };

    // `referenceBounds.sort` sorts in place: the vertical sort starts from
    // the horizontal order.
    bounds.sort_by(by(0));
    let mut horizontal_gaps = Vec::new();
    let mut c = 0;
    'horizontal: for i in 0..bounds.len() {
        let start = bounds[i];
        for &end in &bounds[i + 1..] {
            c += 1;
            if c > VISIBLE_GAPS_LIMIT_PER_AXIS {
                break 'horizontal;
            }
            let [_, start_min_y, start_max_x, start_max_y] = start;
            let [end_min_x, end_min_y, _, end_max_y] = end;
            let a = range_inclusive(start_min_y, start_max_y);
            let b = range_inclusive(end_min_y, end_max_y);
            if start_max_x < end_min_x && ranges_overlap(a, b) {
                let overlap = range_intersection(a, b).expect("overlapping ranges intersect");
                horizontal_gaps.push(Gap {
                    start_bounds: start,
                    end_bounds: end,
                    start_side: [[start_max_x, start_min_y], [start_max_x, start_max_y]],
                    end_side: [[end_min_x, end_min_y], [end_min_x, end_max_y]],
                    length: end_min_x - start_max_x,
                    overlap: [overlap.0, overlap.1],
                });
            }
        }
    }

    bounds.sort_by(by(1));
    let mut vertical_gaps = Vec::new();
    c = 0;
    'vertical: for i in 0..bounds.len() {
        let start = bounds[i];
        for &end in &bounds[i + 1..] {
            c += 1;
            if c > VISIBLE_GAPS_LIMIT_PER_AXIS {
                break 'vertical;
            }
            let [start_min_x, _, start_max_x, start_max_y] = start;
            let [end_min_x, end_min_y, end_max_x, _] = end;
            let a = range_inclusive(start_min_x, start_max_x);
            let b = range_inclusive(end_min_x, end_max_x);
            if start_max_y < end_min_y && ranges_overlap(a, b) {
                let overlap = range_intersection(a, b).expect("overlapping ranges intersect");
                vertical_gaps.push(Gap {
                    start_bounds: start,
                    end_bounds: end,
                    start_side: [[start_min_x, start_max_y], [start_max_x, start_max_y]],
                    end_side: [[end_min_x, end_min_y], [end_max_x, end_min_y]],
                    length: end_min_y - start_max_y,
                    overlap: [overlap.0, overlap.1],
                });
            }
        }
    }

    VisibleGaps {
        horizontal_gaps,
        vertical_gaps,
    }
}

// -- nearest snaps -------------------------------------------------------------------

/// The nearest snaps on each axis and their distance (`nearestSnapsX`,
/// `nearestSnapsY`, `minOffset`).
struct Nearest<'g> {
    x: Vec<Snap<'g>>,
    y: Vec<Snap<'g>>,
    min_x: f64,
    min_y: f64,
}

impl<'g> Nearest<'g> {
    fn new(distance: f64) -> Nearest<'g> {
        Nearest {
            x: Vec::new(),
            y: Vec::new(),
            min_x: distance,
            min_y: distance,
        }
    }

    /// Resets to find the snaps at exactly the snapped position.
    fn reset(&mut self) {
        self.x.clear();
        self.y.clear();
        self.min_x = 0.0;
        self.min_y = 0.0;
    }

    /// Keeps `snap` on x when it is no further than the nearest so far,
    /// dropping those further away. Returns whether it was kept.
    fn offer_x(&mut self, snap: Snap<'g>) -> bool {
        let d = snap.offset().abs();
        if d > self.min_x {
            return false;
        }
        if d < self.min_x {
            self.x.clear();
        }
        self.min_x = d;
        self.x.push(snap);
        true
    }

    fn offer_y(&mut self, snap: Snap<'g>) -> bool {
        let d = snap.offset().abs();
        if d > self.min_y {
            return false;
        }
        if d < self.min_y {
            self.y.clear();
        }
        self.min_y = d;
        self.y.push(snap);
        true
    }

    /// `{ x: nearestSnapsX[0]?.offset ?? 0, y: nearestSnapsY[0]?.offset ?? 0 }`.
    fn snap_offset(&self) -> P {
        [
            self.x.first().map_or(0.0, Snap::offset),
            self.y.first().map_or(0.0, Snap::offset),
        ]
    }
}

/// `getGapSnaps` (`snapping.ts:446-619`).
fn get_gap_snaps<'g>(
    selected_elements: &[&Element],
    drag_offset: P,
    cache: &'g SnapCache,
    state: &SnapAppState,
    event: Option<SnapEvent>,
    nearest: &mut Nearest<'g>,
) {
    if !is_snapping_enabled(state, event, selected_elements) || selected_elements.is_empty() {
        return;
    }
    let Some(gaps) = &cache.visible_gaps else {
        return;
    };
    let [min_x, min_y, max_x, max_y] =
        get_dragged_elements_bounds(selected_elements, drag_offset).map(round);
    let center_x = (min_x + max_x) / 2.0;
    let center_y = (min_y + max_y) / 2.0;

    for gap in &gaps.horizontal_gaps {
        let overlap = range_inclusive(gap.overlap[0], gap.overlap[1]);
        if !ranges_overlap(range_inclusive(min_y, max_y), overlap) {
            continue;
        }

        // center gap
        let gap_mid_x = gap.start_side[0][0] + gap.length / 2.0;
        let center_offset = round(gap_mid_x - center_x);
        let gap_is_larger_than_selection = gap.length > max_x - min_x;
        if gap_is_larger_than_selection
            && nearest.offer_x(Snap::Gap {
                direction: GapSnapDirection::CenterHorizontal,
                gap,
                offset: center_offset,
            })
        {
            continue;
        }

        // side gap, from the right
        let end_max_x = gap.end_bounds[2];
        let distance_to_end_element_x = min_x - end_max_x;
        let side_offset_right = round(gap.length - distance_to_end_element_x);
        if nearest.offer_x(Snap::Gap {
            direction: GapSnapDirection::SideRight,
            gap,
            offset: side_offset_right,
        }) {
            continue;
        }

        // side gap, from the left
        let start_min_x = gap.start_bounds[0];
        let distance_to_start_element_x = start_min_x - max_x;
        let side_offset_left = round(distance_to_start_element_x - gap.length);
        if nearest.offer_x(Snap::Gap {
            direction: GapSnapDirection::SideLeft,
            gap,
            offset: side_offset_left,
        }) {
            continue;
        }
    }

    for gap in &gaps.vertical_gaps {
        let overlap = range_inclusive(gap.overlap[0], gap.overlap[1]);
        if !ranges_overlap(range_inclusive(min_x, max_x), overlap) {
            continue;
        }

        // center gap
        let gap_mid_y = gap.start_side[0][1] + gap.length / 2.0;
        let center_offset = round(gap_mid_y - center_y);
        let gap_is_larger_than_selection = gap.length > max_y - min_y;
        if gap_is_larger_than_selection
            && nearest.offer_y(Snap::Gap {
                direction: GapSnapDirection::CenterVertical,
                gap,
                offset: center_offset,
            })
        {
            continue;
        }

        // side gap, from the top
        let start_min_y = gap.start_bounds[1];
        let distance_to_start_element_y = start_min_y - max_y;
        let side_offset_top = round(distance_to_start_element_y - gap.length);
        if nearest.offer_y(Snap::Gap {
            direction: GapSnapDirection::SideTop,
            gap,
            offset: side_offset_top,
        }) {
            continue;
        }

        // side gap, from the bottom (upstream rounds the distance here,
        // not the offset)
        let end_max_y = gap.end_bounds[3];
        let distance_to_end_element_y = round(min_y - end_max_y);
        let side_offset_bottom = gap.length - distance_to_end_element_y;
        if nearest.offer_y(Snap::Gap {
            direction: GapSnapDirection::SideBottom,
            gap,
            offset: side_offset_bottom,
        }) {
            continue;
        }
    }
}

/// `getPointSnaps` (`snapping.ts:642-690`).
fn get_point_snaps<'g>(
    selected_elements: &[&Element],
    selection_snap_points: &[P],
    cache: &'g SnapCache,
    state: &SnapAppState,
    event: Option<SnapEvent>,
    nearest: &mut Nearest<'g>,
) {
    if !is_snapping_enabled(state, event, selected_elements)
        || (selected_elements.is_empty() && selection_snap_points.is_empty())
    {
        return;
    }
    let Some(reference) = &cache.reference_snap_points else {
        return;
    };
    for &this in selection_snap_points {
        for &other in reference {
            let offset_x = other[0] - this[0];
            let offset_y = other[1] - this[1];
            nearest.offer_x(Snap::Point {
                points: [this, other],
                offset: offset_x,
            });
            nearest.offer_y(Snap::Point {
                points: [this, other],
                offset: offset_y,
            });
        }
    }
}

// -- snap lines ----------------------------------------------------------------------

/// Two numbers as the same `String(n)`: equal (either zero), or both NaN.
fn js_key_eq(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

/// `dedupePoints(points)` (`snapping.ts:871-884`): the first of each point.
fn dedupe_points(points: Vec<P>) -> Vec<P> {
    let mut out: Vec<P> = Vec::new();
    for p in points {
        if !out
            .iter()
            .any(|q| js_key_eq(q[0], p[0]) && js_key_eq(q[1], p[1]))
        {
            out.push(p);
        }
    }
    out
}

/// The points of an object keyed by a number (`{ [key: string]: ... }`),
/// entries in the order `Object.entries` gives them: the keys that are array
/// indices ascending, then the others as inserted.
fn js_number_keyed(snaps: &[Snap<'_>], key_of: impl Fn(&[P; 2]) -> f64) -> Vec<(f64, Vec<P>)> {
    let mut entries: Vec<(f64, Vec<P>)> = Vec::new();
    for snap in snaps {
        let Snap::Point { points, .. } = snap else {
            continue;
        };
        let key = round(key_of(points));
        let rounded = points.map(|p| [round(p[0]), round(p[1])]);
        match entries.iter_mut().find(|(k, _)| js_key_eq(*k, key)) {
            Some((_, list)) => list.extend(rounded),
            None => entries.push((key, rounded.to_vec())),
        }
    }
    // `Number(String(-0))` is 0.
    for entry in &mut entries {
        if entry.0 == 0.0 {
            entry.0 = 0.0;
        }
    }
    let is_index = |k: f64| k.fract() == 0.0 && (0.0..=4_294_967_294.0).contains(&k);
    let (mut indices, others): (Vec<_>, Vec<_>) = entries.into_iter().partition(|e| is_index(e.0));
    indices.sort_by(|a, b| a.0.total_cmp(&b.0));
    indices.extend(others);
    indices
}

fn sort_by_axis(points: &mut [P], axis: usize) {
    points.sort_by(|a, b| {
        (a[axis] - b[axis])
            .partial_cmp(&0.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// `createPointSnapLines(nearestSnapsX, nearestSnapsY)`
/// (`snapping.ts:886-950`).
fn create_point_snap_lines(nearest: &Nearest<'_>) -> Vec<SnapLine> {
    let mut lines = Vec::new();
    for (key, points) in js_number_keyed(&nearest.x, |p| p[0][0]) {
        let mut points: Vec<P> = points.into_iter().map(|p| [key, p[1]]).collect();
        sort_by_axis(&mut points, 1);
        lines.push(SnapLine::Points {
            points: dedupe_points(points),
        });
    }
    for (key, points) in js_number_keyed(&nearest.y, |p| p[0][1]) {
        let mut points: Vec<P> = points.into_iter().map(|p| [p[0], key]).collect();
        sort_by_axis(&mut points, 0);
        lines.push(SnapLine::Points {
            points: dedupe_points(points),
        });
    }
    lines
}

/// `createGapSnapLines(selectedElements, dragOffset, gapSnaps)`
/// (`snapping.ts:969-1158`).
fn create_gap_snap_lines(
    selected_elements: &[&Element],
    drag_offset: P,
    gap_snaps: &[Snap<'_>],
) -> Vec<SnapLine> {
    use SnapLineDirection::{Horizontal, Vertical};
    let [min_x, min_y, max_x, max_y] = get_dragged_elements_bounds(selected_elements, drag_offset);
    let mut lines: Vec<(SnapLineDirection, [P; 2])> = Vec::new();

    for snap in gap_snaps {
        let Snap::Gap { direction, gap, .. } = *snap else {
            continue;
        };
        let [start_min_x, start_min_y, start_max_x, start_max_y] = gap.start_bounds;
        let [end_min_x, end_min_y, end_max_x, end_max_y] = gap.end_bounds;
        let overlap = range_inclusive(gap.overlap[0], gap.overlap[1]);
        let vertical_intersection = range_intersection(range_inclusive(min_y, max_y), overlap);
        let horizontal_gap_intersection =
            range_intersection(range_inclusive(min_x, max_x), overlap);
        let line_y = vertical_intersection.map(|r| (r.0 + r.1) / 2.0);
        let line_x = horizontal_gap_intersection.map(|r| (r.0 + r.1) / 2.0);

        match direction {
            GapSnapDirection::CenterHorizontal => {
                if let Some(y) = line_y {
                    lines.push((Horizontal, [[gap.start_side[0][0], y], [min_x, y]]));
                    lines.push((Horizontal, [[max_x, y], [gap.end_side[0][0], y]]));
                }
            }
            GapSnapDirection::CenterVertical => {
                if let Some(x) = line_x {
                    lines.push((Vertical, [[x, gap.start_side[0][1]], [x, min_y]]));
                    lines.push((Vertical, [[x, max_y], [x, gap.end_side[0][1]]]));
                }
            }
            GapSnapDirection::SideRight => {
                if let Some(y) = line_y {
                    lines.push((Horizontal, [[start_max_x, y], [end_min_x, y]]));
                    lines.push((Horizontal, [[end_max_x, y], [min_x, y]]));
                }
            }
            GapSnapDirection::SideLeft => {
                if let Some(y) = line_y {
                    lines.push((Horizontal, [[max_x, y], [start_min_x, y]]));
                    lines.push((Horizontal, [[start_max_x, y], [end_min_x, y]]));
                }
            }
            GapSnapDirection::SideTop => {
                if let Some(x) = line_x {
                    lines.push((Vertical, [[x, max_y], [x, start_min_y]]));
                    lines.push((Vertical, [[x, start_max_y], [x, end_min_y]]));
                }
            }
            GapSnapDirection::SideBottom => {
                if let Some(x) = line_x {
                    lines.push((Vertical, [[x, start_max_y], [x, end_min_y]]));
                    lines.push((Vertical, [[x, end_max_y], [x, min_y]]));
                }
            }
        }
    }

    // `dedupeGapSnapLines`: the first line of each set of rounded points.
    let mut out: Vec<(SnapLineDirection, [P; 2])> = Vec::new();
    for (direction, points) in lines {
        let points = points.map(|p| [round(p[0]), round(p[1])]);
        let key = [points[0][0], points[0][1], points[1][0], points[1][1]];
        let seen = out.iter().any(|(_, q)| {
            let k = [q[0][0], q[0][1], q[1][0], q[1][1]];
            k.iter().zip(key).all(|(&a, b)| js_key_eq(a, b))
        });
        if !seen {
            out.push((direction, points));
        }
    }
    out.into_iter()
        .map(|(direction, points)| SnapLine::Gap { direction, points })
        .collect()
}

// -- snapping gestures ------------------------------------------------------------------

/// `snapDraggedElements(elements, dragOffset, app, event, elementsMap)`
/// (`snapping.ts:692-864`): snaps the selected elements (those of
/// `elements` whose id is in `selected_element_ids` and not deleted), moved
/// by `drag_offset`, to the cached reference points and gaps. Rounds
/// `drag_offset` to 6 places in place, as upstream does.
pub fn snap_dragged_elements(
    elements: &[&Element],
    selected_element_ids: &HashSet<&str>,
    drag_offset: &mut P,
    cache: &SnapCache,
    state: &SnapAppState,
    event: Option<SnapEvent>,
    elements_map: &ElementsMap<'_>,
) -> SnapResult {
    let selected: Vec<&Element> = elements
        .iter()
        .copied()
        .filter(|e| selected_element_ids.contains(e.base.id.as_str()) && !e.base.is_deleted)
        .collect();
    if !is_snapping_enabled(state, event, &selected) || selected.is_empty() {
        return SnapResult::none();
    }
    drag_offset[0] = round(drag_offset[0]);
    drag_offset[1] = round(drag_offset[1]);
    let mut nearest = Nearest::new(get_snap_distance(state.viewport.zoom));
    let selection_points = get_elements_corners(
        &selected,
        elements_map,
        CornersOptions {
            drag_offset: Some(*drag_offset),
            ..CornersOptions::default()
        },
    );
    get_point_snaps(
        &selected,
        &selection_points,
        cache,
        state,
        event,
        &mut nearest,
    );
    get_gap_snaps(&selected, *drag_offset, cache, state, event, &mut nearest);
    let snap_offset = nearest.snap_offset();

    // Find the snaps again at the snapped position, for lines that do not
    // shift.
    nearest.reset();
    let new_drag_offset = [
        round(drag_offset[0] + snap_offset[0]),
        round(drag_offset[1] + snap_offset[1]),
    ];
    let corners = get_elements_corners(
        &selected,
        elements_map,
        CornersOptions {
            drag_offset: Some(new_drag_offset),
            ..CornersOptions::default()
        },
    );
    get_point_snaps(&selected, &corners, cache, state, event, &mut nearest);
    get_gap_snaps(
        &selected,
        new_drag_offset,
        cache,
        state,
        event,
        &mut nearest,
    );

    let mut snap_lines = create_point_snap_lines(&nearest);
    let gap_snaps: Vec<Snap<'_>> = nearest
        .x
        .iter()
        .chain(&nearest.y)
        .copied()
        .filter(|s| matches!(s, Snap::Gap { .. }))
        .collect();
    snap_lines.extend(create_gap_snap_lines(
        &selected,
        new_drag_offset,
        &gap_snaps,
    ));
    SnapResult {
        snap_offset,
        snap_lines,
    }
}

/// `snapResizingElements(selectedElements, selectedOriginalElements, app,
/// event, dragOffset, transformHandle)` (`snapping.ts:1160-1300`): snaps
/// the corners the handle moves to the cached reference points; the lines
/// come from `selected_elements` (the elements as resized so far).
/// A lone rotated element does not snap.
pub fn snap_resizing_elements(
    selected_elements: &[&Element],
    selected_original_elements: &[&Element],
    cache: &SnapCache,
    state: &SnapAppState,
    event: Option<SnapEvent>,
    drag_offset: P,
    transform_handle: Option<TransformHandleType>,
) -> SnapResult {
    if !is_snapping_enabled(state, event, selected_elements)
        || selected_elements.is_empty()
        || (selected_elements.len() == 1
            && !are_roughly_equal(selected_elements[0].base.angle.0, 0.0, 0.01))
    {
        return SnapResult::none();
    }

    let [mut min_x, mut min_y, mut max_x, mut max_y] =
        get_common_bounds(selected_original_elements);
    let mut selection_snap_points: Vec<P> = Vec::new();
    if let Some(handle) = transform_handle {
        // `transformHandle.includes(...)` on the handle's name: "rotation"
        // includes "n".
        let name = handle.as_str();
        if name.contains('e') {
            max_x += drag_offset[0];
        } else if name.contains('w') {
            min_x += drag_offset[0];
        }
        if name.contains('n') {
            min_y += drag_offset[1];
        } else if name.contains('s') {
            max_y += drag_offset[1];
        }
        match name {
            "e" => selection_snap_points.extend([[max_x, min_y], [max_x, max_y]]),
            "w" => selection_snap_points.extend([[min_x, min_y], [min_x, max_y]]),
            "n" => selection_snap_points.extend([[min_x, min_y], [max_x, min_y]]),
            "s" => selection_snap_points.extend([[min_x, max_y], [max_x, max_y]]),
            "ne" => selection_snap_points.push([max_x, min_y]),
            "nw" => selection_snap_points.push([min_x, min_y]),
            "se" => selection_snap_points.push([max_x, max_y]),
            "sw" => selection_snap_points.push([min_x, max_y]),
            _ => {}
        }
    }

    let mut nearest = Nearest::new(get_snap_distance(state.viewport.zoom));
    get_point_snaps(
        selected_original_elements,
        &selection_snap_points,
        cache,
        state,
        event,
        &mut nearest,
    );
    let snap_offset = nearest.snap_offset();

    nearest.reset();
    let [x1, y1, x2, y2] = get_common_bounds(selected_elements).map(round);
    let corners = [[x1, y1], [x1, y2], [x2, y1], [x2, y2]];
    get_point_snaps(
        selected_elements,
        &corners,
        cache,
        state,
        event,
        &mut nearest,
    );

    SnapResult {
        snap_offset,
        snap_lines: create_point_snap_lines(&nearest),
    }
}

/// `snapNewElement(newElement, app, event, origin, dragOffset,
/// elementsMap)` (`snapping.ts:1302-1363`): snaps the dragged corner
/// (`origin + drag_offset`) of an element being drawn to the cached
/// reference points; the lines come from its box corners.
pub fn snap_new_element(
    new_element: &Element,
    cache: &SnapCache,
    state: &SnapAppState,
    event: Option<SnapEvent>,
    origin: P,
    drag_offset: P,
    elements_map: &ElementsMap<'_>,
) -> SnapResult {
    let selected = [new_element];
    if !is_snapping_enabled(state, event, &selected) {
        return SnapResult::none();
    }
    let selection_snap_points = [[origin[0] + drag_offset[0], origin[1] + drag_offset[1]]];
    let mut nearest = Nearest::new(get_snap_distance(state.viewport.zoom));
    get_point_snaps(
        &selected,
        &selection_snap_points,
        cache,
        state,
        event,
        &mut nearest,
    );
    let snap_offset = nearest.snap_offset();

    nearest.reset();
    let corners = get_elements_corners(
        &selected,
        elements_map,
        CornersOptions {
            bounding_box_corners: true,
            omit_center: true,
            drag_offset: None,
        },
    );
    get_point_snaps(&selected, &corners, cache, state, event, &mut nearest);

    SnapResult {
        snap_offset,
        snap_lines: create_point_snap_lines(&nearest),
    }
}

/// `getSnapLinesAtPointer(elements, app, pointer, event, elementsMap)`
/// (`snapping.ts:1365-1398`): before drawing, the lines from the visible
/// elements' corners nearest the pointer on each axis, and the offset that
/// moves the pointer onto them.
pub fn get_snap_lines_at_pointer(
    elements: &[&Element],
    state: &SnapAppState,
    pointer: P,
    event: Option<SnapEvent>,
    elements_map: &ElementsMap<'_>,
) -> PointerSnapResult {
    if !is_snapping_enabled(state, event, &[]) {
        return PointerSnapResult {
            origin_offset: [0.0, 0.0],
            snap_lines: Vec::new(),
        };
    }
    let reference = get_reference_elements(elements, &[], state, elements_map);
    let distance = get_snap_distance(state.viewport.zoom);
    // Upstream keeps the signed offset and compares absolute values.
    let (mut min_x, mut min_y) = (distance, distance);
    let mut horizontal: Vec<SnapLine> = Vec::new();
    let mut vertical: Vec<SnapLine> = Vec::new();
    let mut first_vertical_x = None;
    let mut first_horizontal_y = None;

    for element in reference {
        for corner in get_elements_corners(&[element], elements_map, CornersOptions::default()) {
            let offset_x = corner[0] - pointer[0];
            if offset_x.abs() <= min_x.abs() {
                if offset_x.abs() < min_x.abs() {
                    vertical.clear();
                    first_vertical_x = None;
                }
                first_vertical_x.get_or_insert(corner[0]);
                vertical.push(SnapLine::Pointer {
                    points: [corner, [corner[0], pointer[1]]],
                    direction: SnapLineDirection::Vertical,
                });
                min_x = offset_x;
            }

            let offset_y = corner[1] - pointer[1];
            if offset_y.abs() <= min_y.abs() {
                if offset_y.abs() < min_y.abs() {
                    horizontal.clear();
                    first_horizontal_y = None;
                }
                first_horizontal_y.get_or_insert(corner[1]);
                horizontal.push(SnapLine::Pointer {
                    points: [corner, [pointer[0], corner[1]]],
                    direction: SnapLineDirection::Horizontal,
                });
                min_y = offset_y;
            }
        }
    }

    let origin_offset = [
        first_vertical_x.map_or(0.0, |x| x - pointer[0]),
        first_horizontal_y.map_or(0.0, |y| y - pointer[1]),
    ];
    vertical.extend(horizontal);
    PointerSnapResult {
        origin_offset,
        snap_lines: vertical,
    }
}

// -- rendering -----------------------------------------------------------------------

/// The app state `renderSnaps` reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapRenderState {
    pub theme: Theme,
    pub zen_mode_enabled: bool,
    /// `zoom.value`.
    pub zoom: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
}

/// A call `renderSnaps` makes on the interactive canvas's context
/// (`lineWidth` and `strokeStyle` are assignments).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SnapCanvasCall {
    Save,
    Restore,
    Translate(f64, f64),
    LineWidth(f64),
    StrokeStyle(&'static str),
    BeginPath,
    MoveTo(f64, f64),
    LineTo(f64, f64),
    Stroke,
}

/// `renderSnaps(context, appState)` (`renderSnaps.ts:16-58`): the calls
/// that draw `snap_lines` (in scene coordinates, after the canvas has been
/// scaled by zoom): lines with a cross on each point, in `#ff6b6b` (light)
/// or `#ff9090` (dark; `#da5b5b` in zen mode, which draws only the crosses
/// and the gap marks, wider). Nothing when there are no lines.
pub fn render_snaps(snap_lines: &[SnapLine], state: &SnapRenderState) -> Vec<SnapCanvasCall> {
    let mut calls = Vec::new();
    if snap_lines.is_empty() {
        return calls;
    }
    let color = match (state.theme, state.zen_mode_enabled) {
        (Theme::Light, _) => SNAP_COLOR_LIGHT,
        (Theme::Dark, true) => SNAP_COLOR_DARK_ZEN,
        (Theme::Dark, false) => SNAP_COLOR_DARK,
    };
    let width = if state.zen_mode_enabled {
        SNAP_WIDTH * 1.5
    } else {
        SNAP_WIDTH
    } / state.zoom;

    calls.push(SnapCanvasCall::Save);
    calls.push(SnapCanvasCall::Translate(state.scroll_x, state.scroll_y));
    for line in snap_lines {
        calls.push(SnapCanvasCall::LineWidth(width));
        calls.push(SnapCanvasCall::StrokeStyle(color));
        match line {
            SnapLine::Pointer { points, .. } => {
                draw_cross(&mut calls, points[0], state);
                if !state.zen_mode_enabled {
                    draw_line(&mut calls, points[0], points[1]);
                }
            }
            SnapLine::Gap { direction, points } => {
                draw_gap_line(&mut calls, points[0], points[1], *direction, state);
            }
            SnapLine::Points { points } => {
                if !state.zen_mode_enabled {
                    if let (Some(&first), Some(&last)) = (points.first(), points.last()) {
                        draw_line(&mut calls, first, last);
                    }
                }
                for &p in points {
                    draw_cross(&mut calls, p, state);
                }
            }
        }
    }
    calls.push(SnapCanvasCall::Restore);
    calls
}

/// `drawCross` (`renderSnaps.ts:94-117`).
fn draw_cross(calls: &mut Vec<SnapCanvasCall>, [x, y]: P, state: &SnapRenderState) {
    let size = if state.zen_mode_enabled {
        SNAP_CROSS_SIZE * 1.5
    } else {
        SNAP_CROSS_SIZE
    } / state.zoom;
    calls.extend([
        SnapCanvasCall::Save,
        SnapCanvasCall::BeginPath,
        SnapCanvasCall::MoveTo(x - size, y - size),
        SnapCanvasCall::LineTo(x + size, y + size),
        SnapCanvasCall::MoveTo(x + size, y - size),
        SnapCanvasCall::LineTo(x - size, y + size),
        SnapCanvasCall::Stroke,
        SnapCanvasCall::Restore,
    ]);
}

/// `drawLine` (`renderSnaps.ts:119-128`): upstream starts the path with a
/// `lineTo`, which moves when the path is empty.
fn draw_line(calls: &mut Vec<SnapCanvasCall>, from: P, to: P) {
    calls.extend([
        SnapCanvasCall::BeginPath,
        SnapCanvasCall::LineTo(from[0], from[1]),
        SnapCanvasCall::LineTo(to[0], to[1]),
        SnapCanvasCall::Stroke,
    ]);
}

/// `drawGapLine` (`renderSnaps.ts:130-213`): the gap's line with a bar at
/// each end (8 / zoom long) and a double bar in the middle.
fn draw_gap_line(
    calls: &mut Vec<SnapCanvasCall>,
    from: P,
    to: P,
    direction: SnapLineDirection,
    state: &SnapRenderState,
) {
    let full = 8.0 / state.zoom;
    let half = full / 2.0;
    let quarter = full / 4.0;
    let zen = state.zen_mode_enabled;
    match direction {
        SnapLineDirection::Horizontal => {
            let mid = [(from[0] + to[0]) / 2.0, from[1]];
            if !zen {
                draw_line(calls, [from[0], from[1] - full], [from[0], from[1] + full]);
            }
            draw_line(
                calls,
                [mid[0] - quarter, mid[1] - half],
                [mid[0] - quarter, mid[1] + half],
            );
            draw_line(
                calls,
                [mid[0] + quarter, mid[1] - half],
                [mid[0] + quarter, mid[1] + half],
            );
            if !zen {
                draw_line(calls, [to[0], to[1] - full], [to[0], to[1] + full]);
                draw_line(calls, from, to);
            }
        }
        SnapLineDirection::Vertical => {
            let mid = [from[0], (from[1] + to[1]) / 2.0];
            if !zen {
                draw_line(calls, [from[0] - full, from[1]], [from[0] + full, from[1]]);
            }
            draw_line(
                calls,
                [mid[0] - half, mid[1] - quarter],
                [mid[0] + half, mid[1] - quarter],
            );
            draw_line(
                calls,
                [mid[0] - half, mid[1] + quarter],
                [mid[0] + half, mid[1] + quarter],
            );
            if !zen {
                draw_line(calls, [to[0] - full, to[1]], [to[0] + full, to[1]]);
                draw_line(calls, from, to);
            }
        }
    }
}
