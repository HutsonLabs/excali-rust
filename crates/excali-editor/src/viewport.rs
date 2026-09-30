//! The viewport: zoom limits and normalisation, the viewport/scene
//! coordinate transforms, scrolling, scroll and zoom locks, the wheel and
//! the zoom actions, zoom-to-fit.
//!
//! Upstream, at the pinned commit:
//!
//! - `packages/common/src/constants.ts:362-364`: `ZOOM_STEP`, `MIN_ZOOM`,
//!   `MAX_ZOOM` ([`excali_core::constants`]);
//! - `packages/excalidraw/scene/normalize.ts:7-9`: [`get_normalized_zoom`];
//! - `packages/common/src/utils.ts:317-358`:
//!   [`viewport_coords_to_scene_coords`], [`scene_coords_to_viewport_coords`];
//! - `packages/excalidraw/viewport.ts`: [`DEFAULT_OVERSCROLL`],
//!   [`constrain_scroll_state`], `getViewportForZoom` and
//!   [`get_viewport_for_zoom_with_scroll_constraints`],
//!   [`zoom_to_fit_bounds`], [`get_closest_element_bounds`],
//!   [`center_scroll_on`], [`scroll_bounds_into_view`],
//!   [`get_scroll_to_content_state`];
//! - `packages/excalidraw/components/App.viewport.ts:771-825`:
//!   `AppViewport.translate` ([`translate`]);
//! - `packages/excalidraw/components/App.viewport.ts:173-351, 672-760,
//!   860-863`: `setViewport` for a box, with `getConstrainedTargetViewport`,
//!   `interpolateViewport` and the eased `animateToViewport`
//!   ([`AppViewport`], [`get_constrained_target_viewport`],
//!   [`interpolate_viewport`]), and `easeOut`
//!   (`packages/common/src/utils.ts:232-234`, [`ease_out`]);
//! - `packages/excalidraw/components/App.wheel.ts`: `AppWheel.handle` and
//!   `zoomBy` ([`handle_wheel`], [`wheel_zoom_value`]);
//! - `packages/excalidraw/appState.ts:352-355`: `resolveInputDevice`
//!   ([`InputDevice::resolve`]);
//! - `packages/excalidraw/actions/actionCanvas.tsx`: `actionZoomIn`,
//!   `actionZoomOut`, `actionResetZoom`, `actionZoomToFit`,
//!   `actionZoomToFitSelection` and `actionZoomToFitSelectionInViewport`
//!   ([`ZoomAction`], [`perform_zoom_action`]);
//! - `packages/excalidraw/types.ts:288-324, 1498-1503`: `InputDevice`,
//!   `ScrollConstraints`, `Offsets`.
//!
//! See `site/content/research/rendering.md` section 4 (zoom).
//!
//! Nothing here touches the DOM or holds state: each function takes the
//! viewport part of the app state ([`ViewportState`]) and returns the
//! viewport to commit ([`Viewport`]) with the side effects upstream's App
//! would run named in the result ([`Translation`], [`WheelOutcome`]):
//! requesting to stop following a collaborator, cancelling or scheduling
//! the scroll-lock rubberband snap-back, flushing a pending drag-pan move,
//! the zoom-scaled bitmap flag. The one state kept is [`AppViewport`]'s:
//! the animated `setViewport` transition, which the host advances frame by
//! frame and which takes over (or, into a locked viewport, holds off) the
//! user's pans and zooms. The rubberband snap-back animation is the
//! host's.

use excali_core::app_state::AppState;
use excali_core::constants::{MAX_ZOOM, MIN_ZOOM, ZOOM_STEP};
use excali_core::element::Element;
use excali_math::{clamp, js, round, round_to_step, RoundingFn};
use excali_scene::bounds::{get_common_bounds, get_element_bounds, Bounds, ElementsMap};
use excali_scene::new_element_scene::is_invisibly_small_element;
use serde_json::{json, Map, Value};

/// `DEFAULT_OVERSCROLL` (`viewport.ts:41`): the default rubberband give of a
/// scroll lock, in viewport px.
pub const DEFAULT_OVERSCROLL: f64 = 150.0;

/// `MouseEvent.buttons` bit of the wheel (middle) button
/// (`WHEEL_BUTTON_MASK`, `App.wheel.ts:11`).
pub const WHEEL_BUTTON_MASK: u16 = 4;

/// `getNormalizedZoom(zoom)` (`scene/normalize.ts:7-9`):
/// `clamp(round(zoom, 6), MIN_ZOOM, MAX_ZOOM)`. NaN stays NaN.
pub fn get_normalized_zoom(zoom: f64) -> f64 {
    clamp(round(zoom, 6.0, RoundingFn::Round), MIN_ZOOM, MAX_ZOOM)
}

/// `Offsets` (`types.ts:1498-1503`): per-side insets in viewport px.
/// Upstream's sides are optional and every reader takes a missing one as
/// 0, which is what [`Offsets::default`] holds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Offsets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Offsets {
    /// The offsets of a JSON object; a missing or non-numeric side is 0.
    pub fn from_json(value: &Value) -> Offsets {
        let side = |key: &str| value.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        Offsets {
            top: side("top"),
            right: side("right"),
            bottom: side("bottom"),
            left: side("left"),
        }
    }

    /// The offsets as JSON, with the zero sides left out.
    pub fn to_json(&self) -> Value {
        let mut map = Map::new();
        for (key, value) in [
            ("top", self.top),
            ("right", self.right),
            ("bottom", self.bottom),
            ("left", self.left),
        ] {
            if value != 0.0 {
                map.insert(key.to_owned(), json!(value));
            }
        }
        Value::Object(map)
    }
}

/// `ScrollConstraints` (`types.ts:291-324`): a box in scene coordinates
/// that pan and zoom are locked to (`appState.scrollConstraints`).
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollConstraints {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Panning keeps the viewport within the box.
    pub lock_scroll: bool,
    /// The viewport cannot zoom out below [`ScrollConstraints::zoom`].
    pub lock_zoom: bool,
    /// The zoom the lock's navigation settled on.
    pub zoom: f64,
    /// Rubberband give past the resting clamp, in viewport px.
    pub overscroll: f64,
    /// Extra scrollable room past each edge of the box, in viewport px.
    pub offsets: Offsets,
}

impl ScrollConstraints {
    /// `appState.scrollConstraints`: `None` for `null` or anything that is
    /// not a box.
    pub fn from_json(value: &Value) -> Option<ScrollConstraints> {
        let object = value.as_object()?;
        let number = |key: &str| object.get(key).and_then(Value::as_f64);
        let flag = |key: &str| object.get(key).and_then(Value::as_bool).unwrap_or(false);
        Some(ScrollConstraints {
            x: number("x")?,
            y: number("y")?,
            width: number("width")?,
            height: number("height")?,
            lock_scroll: flag("lockScroll"),
            lock_zoom: flag("lockZoom"),
            zoom: number("zoom").unwrap_or(1.0),
            overscroll: number("overscroll").unwrap_or(0.0),
            offsets: object
                .get("offsets")
                .map(Offsets::from_json)
                .unwrap_or_default(),
        })
    }

    /// The constraints as upstream stores them.
    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "x": self.x,
            "y": self.y,
            "width": self.width,
            "height": self.height,
            "lockScroll": self.lock_scroll,
            "lockZoom": self.lock_zoom,
            "zoom": self.zoom,
            "overscroll": self.overscroll,
        });
        if self.offsets != Offsets::default() {
            value["offsets"] = self.offsets.to_json();
        }
        value
    }

    /// The box as bounds (`getScrollConstraintsBounds`,
    /// `actionCanvas.tsx`): what zoom-to-fit fits under a lock.
    pub fn bounds(&self) -> Bounds {
        [self.x, self.y, self.x + self.width, self.y + self.height]
    }

    /// The lowest zoom the lock allows: its zoom under a zoom lock,
    /// [`MIN_ZOOM`] otherwise.
    pub fn min_zoom(&self) -> f64 {
        if self.lock_zoom {
            self.zoom
        } else {
            MIN_ZOOM
        }
    }
}

/// The scroll and zoom a viewport change commits
/// (`Pick<AppState, "scrollX" | "scrollY" | "zoom">`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub scroll_x: f64,
    pub scroll_y: f64,
    /// `zoom.value`.
    pub zoom: f64,
}

impl Viewport {
    /// Writes `scrollX`, `scrollY` and `zoom` into the app state.
    pub fn write_to(&self, app_state: &mut AppState) {
        app_state.insert("scrollX", json!(self.scroll_x));
        app_state.insert("scrollY", json!(self.scroll_y));
        app_state.insert("zoom", json!({ "value": self.zoom }));
    }
}

/// The app state the viewport reads: scroll, zoom, the canvas size and
/// offset in the page, and the scroll lock.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewportState {
    pub scroll_x: f64,
    pub scroll_y: f64,
    /// `zoom.value`.
    pub zoom: f64,
    /// The canvas's CSS size.
    pub width: f64,
    pub height: f64,
    /// The canvas's offset in the page (`offsetLeft`, `offsetTop`).
    pub offset_left: f64,
    pub offset_top: f64,
    pub scroll_constraints: Option<ScrollConstraints>,
}

impl ViewportState {
    /// The viewport keys of an app state; a missing size or offset is 0
    /// (upstream's App measures them), a missing zoom 1.
    pub fn from_app_state(app_state: &AppState) -> ViewportState {
        let number = |key: &str| app_state.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        ViewportState {
            scroll_x: number("scrollX"),
            scroll_y: number("scrollY"),
            zoom: app_state.zoom().unwrap_or(1.0),
            width: number("width"),
            height: number("height"),
            offset_left: number("offsetLeft"),
            offset_top: number("offsetTop"),
            scroll_constraints: app_state
                .get("scrollConstraints")
                .and_then(ScrollConstraints::from_json),
        }
    }

    /// The scroll and zoom.
    pub fn viewport(&self) -> Viewport {
        Viewport {
            scroll_x: self.scroll_x,
            scroll_y: self.scroll_y,
            zoom: self.zoom,
        }
    }

    /// This state with `viewport` committed.
    pub fn with_viewport(&self, viewport: Viewport) -> ViewportState {
        ViewportState {
            scroll_x: viewport.scroll_x,
            scroll_y: viewport.scroll_y,
            zoom: viewport.zoom,
            ..self.clone()
        }
    }
}

/// JavaScript's falsy test on a number: 0, -0 and NaN.
fn falsy(x: f64) -> bool {
    x == 0.0 || x.is_nan()
}

/// `Math.sign(x)`: -1, 1, or `x` itself for ±0 and NaN.
fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

// -- coordinate transforms ---------------------------------------------------------

/// `viewportCoordsToSceneCoords({clientX, clientY}, appState)`
/// (`utils.ts:317-337`): `(client - offset) / zoom - scroll`.
pub fn viewport_coords_to_scene_coords(
    client_x: f64,
    client_y: f64,
    state: &ViewportState,
) -> (f64, f64) {
    let x = (client_x - state.offset_left) / state.zoom - state.scroll_x;
    let y = (client_y - state.offset_top) / state.zoom - state.scroll_y;
    (x, y)
}

/// `sceneCoordsToViewportCoords({sceneX, sceneY}, appState)`
/// (`utils.ts:339-358`): `(scene + scroll) * zoom + offset`.
pub fn scene_coords_to_viewport_coords(
    scene_x: f64,
    scene_y: f64,
    state: &ViewportState,
) -> (f64, f64) {
    let x = (scene_x + state.scroll_x) * state.zoom + state.offset_left;
    let y = (scene_y + state.scroll_y) * state.zoom + state.offset_top;
    (x, y)
}

// -- scroll constraints ------------------------------------------------------------

/// `constrainScrollAxis` (`viewport.ts`): keeps the visible span
/// `[-scroll, -scroll + visibleSize]` inside the box widened by the
/// offsets, resting centred when the box cannot cover the viewport, with
/// `overscroll` of give on both sides.
fn constrain_scroll_axis(
    scroll: f64,
    box_start: f64,
    box_size: f64,
    visible_size: f64,
    offset_start: f64,
    offset_end: f64,
    overscroll: f64,
) -> f64 {
    let max = -box_start + offset_start;
    let min = visible_size - (box_start + box_size) - offset_end;
    if min > max {
        let center = (min + max) / 2.0;
        return clamp(scroll, center - overscroll, center + overscroll);
    }
    clamp(scroll, min - overscroll, max + overscroll)
}

/// `constrainScrollState(state, overscroll)` (`viewport.ts`): the scroll and
/// zoom clamped into the scroll lock; unchanged without one. `overscroll`
/// (viewport px) is rubberband give past the resting clamp, 0 for a hard
/// clamp.
pub fn constrain_scroll_state(state: &ViewportState, overscroll: f64) -> Viewport {
    let Some(constraints) = &state.scroll_constraints else {
        return state.viewport();
    };
    let zoom = get_normalized_zoom(clamp(state.zoom, constraints.min_zoom(), MAX_ZOOM));
    if !constraints.lock_scroll {
        return Viewport {
            scroll_x: state.scroll_x,
            scroll_y: state.scroll_y,
            zoom,
        };
    }
    let give = js::max(overscroll, 0.0) / zoom;
    let offsets = &constraints.offsets;
    Viewport {
        scroll_x: constrain_scroll_axis(
            state.scroll_x,
            constraints.x,
            constraints.width,
            state.width / zoom,
            offsets.left / zoom,
            offsets.right / zoom,
            give,
        ),
        scroll_y: constrain_scroll_axis(
            state.scroll_y,
            constraints.y,
            constraints.height,
            state.height / zoom,
            offsets.top / zoom,
            offsets.bottom / zoom,
            give,
        ),
        zoom,
    }
}

/// `getViewportForZoom` (`viewport.ts`): the scroll that keeps the scene
/// point under viewport point `(viewport_x, viewport_y)` in place at
/// `next_zoom`.
fn get_viewport_for_zoom(
    viewport_x: f64,
    viewport_y: f64,
    next_zoom: f64,
    state: &ViewportState,
) -> Viewport {
    let app_layer_x = viewport_x - state.offset_left;
    let app_layer_y = viewport_y - state.offset_top;
    let current_zoom = state.zoom;
    // the scroll position without zoom
    let base_scroll_x = state.scroll_x + (app_layer_x - app_layer_x / current_zoom);
    let base_scroll_y = state.scroll_y + (app_layer_y - app_layer_y / current_zoom);
    // the scroll offsets at the target zoom
    let zoom_offset_scroll_x = -(app_layer_x - app_layer_x / next_zoom);
    let zoom_offset_scroll_y = -(app_layer_y - app_layer_y / next_zoom);
    Viewport {
        scroll_x: base_scroll_x + zoom_offset_scroll_x,
        scroll_y: base_scroll_y + zoom_offset_scroll_y,
        zoom: next_zoom,
    }
}

/// `getViewportForZoomWithScrollConstraints({viewportX, viewportY,
/// nextZoom}, state)` (`viewport.ts`): zooms to `next_zoom` around a
/// viewport point, clamped into the scroll lock, keeping a rubberband
/// displacement the same distance on screen.
pub fn get_viewport_for_zoom_with_scroll_constraints(
    viewport_x: f64,
    viewport_y: f64,
    next_zoom: f64,
    state: &ViewportState,
) -> Viewport {
    let resting = constrain_scroll_state(state, 0.0);
    let overscroll_x = (state.scroll_x - resting.scroll_x) * state.zoom;
    let overscroll_y = (state.scroll_y - resting.scroll_y) * state.zoom;

    let zoomed = constrain_scroll_state(
        &state.with_viewport(get_viewport_for_zoom(
            viewport_x, viewport_y, next_zoom, state,
        )),
        0.0,
    );

    let Some(constraints) = state.scroll_constraints.as_ref().filter(|c| c.lock_scroll) else {
        return zoomed;
    };
    if falsy(overscroll_x) && falsy(overscroll_y) {
        return zoomed;
    }
    let zoom = zoomed.zoom;
    constrain_scroll_state(
        &state.with_viewport(Viewport {
            scroll_x: zoomed.scroll_x + overscroll_x / zoom,
            scroll_y: zoomed.scroll_y + overscroll_y / zoom,
            zoom,
        }),
        constraints.overscroll,
    )
}

// -- translate ------------------------------------------------------------------------

/// The keys a viewport update sets (a `Partial` of scroll and zoom).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewportUpdate {
    pub scroll_x: Option<f64>,
    pub scroll_y: Option<f64>,
    pub zoom: Option<f64>,
}

impl From<Viewport> for ViewportUpdate {
    fn from(v: Viewport) -> ViewportUpdate {
        ViewportUpdate {
            scroll_x: Some(v.scroll_x),
            scroll_y: Some(v.scroll_y),
            zoom: Some(v.zoom),
        }
    }
}

/// `translate`'s options (`App.viewport.ts:771-785`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TranslateOptions {
    /// The update's zoom was clamped into the lock already (the wheel
    /// zoom), so a zoom change keeps the rubberband give.
    pub zoom_pre_constrained: bool,
    /// Leave a running snap-back animation alone.
    pub preserve_scroll_constraints_snap_back: bool,
}

/// What `AppViewport.translate` did: the viewport it committed, and the
/// side effects around it. Every translation also requests to stop
/// following a collaborator (`app.requestUnfollow()`), after cancelling
/// the snap-back animation and before the state update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Translation {
    /// The viewport after the update (the previous one for no update).
    pub viewport: Viewport,
    /// Cancel the running rubberband snap-back animation
    /// (`AnimationController.cancel(SCROLL_CONSTRAINTS_SNAP_BACK_ANIMATION_KEY)`).
    pub cancel_snap_back: bool,
    /// (Re)start the debounced rubberband snap-back (200 ms): the update
    /// was clamped with give past the lock.
    pub schedule_snap_back: bool,
}

/// `AppViewport.translate(update, opts)` (`App.viewport.ts:771-825`), for a
/// user pan or zoom: the update merged into the state and, under a scroll
/// lock, clamped with the lock's rubberband give, or hard-clamped when the
/// zoom changed (unless `zoom_pre_constrained`). `None` updates nothing.
pub fn translate(
    prev: &ViewportState,
    update: Option<ViewportUpdate>,
    opts: TranslateOptions,
) -> Translation {
    let cancel_snap_back = !opts.preserve_scroll_constraints_snap_back;
    let Some(update) = update else {
        return Translation {
            viewport: prev.viewport(),
            cancel_snap_back,
            schedule_snap_back: false,
        };
    };
    let next = prev.with_viewport(Viewport {
        scroll_x: update.scroll_x.unwrap_or(prev.scroll_x),
        scroll_y: update.scroll_y.unwrap_or(prev.scroll_y),
        zoom: update.zoom.unwrap_or(prev.zoom),
    });
    let Some(constraints) = &next.scroll_constraints else {
        return Translation {
            viewport: next.viewport(),
            cancel_snap_back,
            schedule_snap_back: false,
        };
    };
    let zoomed = !opts.zoom_pre_constrained && next.zoom != prev.zoom;
    let overscroll = if zoomed { 0.0 } else { constraints.overscroll };
    Translation {
        viewport: constrain_scroll_state(&next, overscroll),
        cancel_snap_back,
        schedule_snap_back: overscroll > 0.0,
    }
}

// -- zoom to fit -----------------------------------------------------------------------

/// `SetViewportOptions["fit"]` (`viewport.ts`), like CSS `object-fit`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Fit {
    /// Zoom out so the target fits, never past 100%.
    #[default]
    ScaleDown,
    /// Zoom so the target fills the viewport (may pass 100%).
    Contain,
    /// Keep the zoom, only centre the target.
    None,
}

/// `zoomToFitBounds`'s arguments.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomToFit {
    pub bounds: Bounds,
    /// UI covering the canvas edges, in viewport px (`canvasOffsets`).
    pub canvas_offsets: Offsets,
    pub fit: Fit,
    /// Default -infinity.
    pub min_zoom: f64,
    /// Default +infinity.
    pub max_zoom: f64,
    /// Round the fitted zoom down to a multiple of [`ZOOM_STEP`].
    pub stepped_zoom: bool,
}

impl ZoomToFit {
    /// Upstream's defaults: scale down, no offsets, no zoom range, not
    /// stepped.
    pub fn new(bounds: Bounds) -> ZoomToFit {
        ZoomToFit {
            bounds,
            canvas_offsets: Offsets::default(),
            fit: Fit::ScaleDown,
            min_zoom: f64::NEG_INFINITY,
            max_zoom: f64::INFINITY,
            stepped_zoom: false,
        }
    }
}

/// `zoomValueToFitBoundsOnViewport` (`viewport.ts`): the zoom that fits
/// the bounds, at most 1.
fn zoom_value_to_fit_bounds_on_viewport(bounds: Bounds, width: f64, height: f64) -> f64 {
    let [x1, y1, x2, y2] = bounds;
    let zoom_value_for_width = width / (x2 - x1);
    let zoom_value_for_height = height / (y2 - y1);
    js::min(js::min(zoom_value_for_width, zoom_value_for_height), 1.0)
}

/// `zoomToFitBounds({bounds, appState, canvasOffsets, fit, minZoom,
/// maxZoom, steppedZoom})` (`viewport.ts`): the zoom that fits the bounds
/// into the viewport less the offsets, with the bounds centred there.
/// (Upstream's result also carries `captureUpdate: EVENTUALLY`.)
pub fn zoom_to_fit_bounds(options: &ZoomToFit, state: &ViewportState) -> Viewport {
    let [x1, y1, x2, y2] = options.bounds;
    let center_x = (x1 + x2) / 2.0;
    let center_y = (y1 + y2) / 2.0;
    let o = &options.canvas_offsets;
    let effective_width = state.width - o.left - o.right;
    let effective_height = state.height - o.top - o.bottom;

    let adjusted = match options.fit {
        Fit::None => state.zoom,
        Fit::Contain => js::min(effective_width / (x2 - x1), effective_height / (y2 - y1)),
        Fit::ScaleDown => {
            zoom_value_to_fit_bounds_on_viewport(options.bounds, effective_width, effective_height)
        }
    };
    let target = if options.stepped_zoom && options.fit != Fit::None {
        round_to_step(adjusted, ZOOM_STEP, RoundingFn::Floor)
    } else {
        adjusted
    };
    let zoom = get_normalized_zoom(clamp(target, options.min_zoom, options.max_zoom));
    let (scroll_x, scroll_y) =
        center_scroll_on((center_x, center_y), state.width, state.height, zoom, o);
    Viewport {
        scroll_x,
        scroll_y,
        zoom,
    }
}

/// `centerScrollOn({scenePoint, viewportDimensions, zoom, offsets})`
/// (`viewport.ts`): the scroll that puts the scene point at the centre of
/// the viewport less the offsets.
pub fn center_scroll_on(
    scene_point: (f64, f64),
    width: f64,
    height: f64,
    zoom: f64,
    offsets: &Offsets,
) -> (f64, f64) {
    let mut scroll_x = (width - offsets.right) / 2.0 / zoom - scene_point.0;
    scroll_x += offsets.left / 2.0 / zoom;
    let mut scroll_y = (height - offsets.bottom) / 2.0 / zoom - scene_point.1;
    scroll_y += offsets.top / 2.0 / zoom;
    (scroll_x, scroll_y)
}

// -- setViewport ----------------------------------------------------------------------

/// `DEFAULT_SCROLL_ANIMATION_DURATION` (`App.viewport.ts:51`), in ms.
pub const DEFAULT_SCROLL_ANIMATION_DURATION: f64 = 500.0;

/// `easeOut(k)` (`packages/common/src/utils.ts:232-234`): `1 - (1 - k)^4`.
pub fn ease_out(k: f64) -> f64 {
    1.0 - js::pow(1.0 - k, 4.0)
}

/// `SetViewportOptions["lock"]["overscroll"]`: `true` or missing, `false`,
/// or a number.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Overscroll {
    /// [`DEFAULT_OVERSCROLL`].
    #[default]
    Default,
    /// A rigid lock.
    Off,
    /// The give in viewport px (negative is 0).
    Give(f64),
}

impl Overscroll {
    /// `resolveOverscroll` (`App.viewport.ts:173-183`).
    pub fn resolve(self) -> f64 {
        match self {
            Overscroll::Default => DEFAULT_OVERSCROLL,
            Overscroll::Off => 0.0,
            Overscroll::Give(give) => js::max(give, 0.0),
        }
    }
}

/// `SetViewportOptions["lock"]` (`viewport.ts:85-97`): the scroll and zoom
/// lock a navigation installs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewportLock {
    /// Constrain panning to the target box.
    pub scroll: bool,
    /// Make the resolved zoom the minimum zoom.
    pub zoom: bool,
    pub overscroll: Overscroll,
}

/// `SetViewportOptions["animation"]`: `false`, `true` or missing (or an
/// object without a duration), or a duration in ms.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ViewportAnimation {
    Off,
    #[default]
    Default,
    Duration(f64),
}

impl ViewportAnimation {
    /// `resolveAnimationDuration` (`App.viewport.ts:309-319`): `None` for
    /// no animation.
    pub fn duration(self) -> Option<f64> {
        match self {
            ViewportAnimation::Off => None,
            ViewportAnimation::Default => Some(DEFAULT_SCROLL_ANIMATION_DURATION),
            ViewportAnimation::Duration(ms) => Some(ms),
        }
    }
}

/// `SetViewportOptions` (`viewport.ts:59-110`) for a box target, the UI
/// offsets resolved by the caller (`resolveOffsets`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetViewportOptions {
    /// The box in scene coordinates; `None` for a target that did not
    /// resolve (an unknown id, deleted elements), which changes nothing.
    pub target: Option<Bounds>,
    pub fit: Fit,
    pub offsets: Option<Offsets>,
    pub lock: Option<ViewportLock>,
    pub animation: ViewportAnimation,
}

impl SetViewportOptions {
    /// Upstream's defaults for a box: scale down, no offsets, no lock, the
    /// default animation.
    pub fn new(target: Bounds) -> SetViewportOptions {
        SetViewportOptions {
            target: Some(target),
            fit: Fit::ScaleDown,
            offsets: None,
            lock: None,
            animation: ViewportAnimation::Default,
        }
    }
}

/// Where a navigation lands: the viewport and the scroll lock to install
/// (`None` clears the previous one).
#[derive(Debug, Clone, PartialEq)]
pub struct TargetViewport {
    pub viewport: Viewport,
    pub scroll_constraints: Option<ScrollConstraints>,
}

/// `getConstrainedTargetViewport(appState, bounds, {fit, offsets, lock})`
/// (`App.viewport.ts:185-244`): `zoomToFitBounds` (not stepped) for the
/// box, and with a scroll or zoom lock the constraints of the box, the
/// viewport clamped into them.
pub fn get_constrained_target_viewport(
    state: &ViewportState,
    bounds: Bounds,
    fit: Fit,
    offsets: Option<Offsets>,
    lock: Option<ViewportLock>,
) -> TargetViewport {
    let options = ZoomToFit {
        canvas_offsets: offsets.unwrap_or_default(),
        fit,
        ..ZoomToFit::new(bounds)
    };
    let viewport = zoom_to_fit_bounds(&options, state);
    let Some(lock) = lock.filter(|l| l.scroll || l.zoom) else {
        return TargetViewport {
            viewport,
            scroll_constraints: None,
        };
    };
    let [x1, y1, x2, y2] = bounds;
    let constraints = ScrollConstraints {
        x: x1,
        y: y1,
        width: x2 - x1,
        height: y2 - y1,
        lock_scroll: lock.scroll,
        lock_zoom: lock.zoom,
        zoom: viewport.zoom,
        overscroll: lock.overscroll.resolve(),
        offsets: offsets.unwrap_or_default(),
    };
    let locked = ViewportState {
        scroll_constraints: Some(constraints.clone()),
        ..state.with_viewport(viewport)
    };
    TargetViewport {
        viewport: constrain_scroll_state(&locked, 0.0),
        scroll_constraints: Some(constraints),
    }
}

/// `interpolateViewport({from, target, factor})` (`App.viewport.ts:246-
/// 301`): the zoom blended geometrically and `scroll × zoom` linearly by
/// the weight that zoom implies, so every scene point moves on a straight
/// screen line; `factor >= 1` lands on the target exactly.
pub fn interpolate_viewport(from: Viewport, target: Viewport, factor: f64) -> Viewport {
    if factor >= 1.0 {
        return target;
    }
    let zoom = from.zoom * js::pow(target.zoom / from.zoom, factor);
    let m = if target.zoom == from.zoom {
        factor
    } else {
        (zoom - from.zoom) / (target.zoom - from.zoom)
    };
    Viewport {
        scroll_x: ((1.0 - m) * from.scroll_x * from.zoom + m * target.scroll_x * target.zoom)
            / zoom,
        scroll_y: ((1.0 - m) * from.scroll_y * from.zoom + m * target.scroll_y * target.zoom)
            / zoom,
        zoom,
    }
}

/// The app state keys a navigation step sets: the viewport, the scroll
/// lock (`Some(None)` clears it) and `shouldCacheIgnoreZoom` (frames draw
/// from zoom-scaled bitmaps until the transition settles).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewportPatch {
    pub viewport: Option<Viewport>,
    pub scroll_constraints: Option<Option<ScrollConstraints>>,
    pub should_cache_ignore_zoom: Option<bool>,
}

impl ViewportPatch {
    /// This patch, then `next` over it (two `setState` calls in a row).
    fn then(self, next: ViewportPatch) -> ViewportPatch {
        ViewportPatch {
            viewport: next.viewport.or(self.viewport),
            scroll_constraints: next.scroll_constraints.or(self.scroll_constraints),
            should_cache_ignore_zoom: next
                .should_cache_ignore_zoom
                .or(self.should_cache_ignore_zoom),
        }
    }

    fn frame(viewport: Viewport) -> ViewportPatch {
        ViewportPatch {
            viewport: Some(viewport),
            scroll_constraints: None,
            should_cache_ignore_zoom: Some(true),
        }
    }

    fn settle(target: TargetViewport) -> ViewportPatch {
        ViewportPatch {
            viewport: Some(target.viewport),
            scroll_constraints: Some(target.scroll_constraints),
            should_cache_ignore_zoom: Some(false),
        }
    }

    /// The patch applied to a viewport state and the
    /// `shouldCacheIgnoreZoom` flag.
    pub fn apply(&self, state: &mut ViewportState, should_cache_ignore_zoom: &mut bool) {
        if let Some(viewport) = self.viewport {
            *state = state.with_viewport(viewport);
        }
        if let Some(constraints) = &self.scroll_constraints {
            state.scroll_constraints = constraints.clone();
        }
        if let Some(flag) = self.should_cache_ignore_zoom {
            *should_cache_ignore_zoom = flag;
        }
    }

    /// The patch as app state keys, as upstream names them.
    pub fn to_map(&self) -> Map<String, Value> {
        let mut map = Map::new();
        if let Some(v) = self.viewport {
            map.insert("scrollX".into(), json!(v.scroll_x));
            map.insert("scrollY".into(), json!(v.scroll_y));
            map.insert("zoom".into(), json!({ "value": v.zoom }));
        }
        if let Some(constraints) = &self.scroll_constraints {
            map.insert(
                "scrollConstraints".into(),
                constraints
                    .as_ref()
                    .map_or(Value::Null, ScrollConstraints::to_json),
            );
        }
        if let Some(flag) = self.should_cache_ignore_zoom {
            map.insert("shouldCacheIgnoreZoom".into(), json!(flag));
        }
        map
    }
}

/// An animated navigation (`animateToViewport`, `App.viewport.ts:321-351`,
/// run by `AnimationController`, `renderer/animation.ts`): eased from the
/// viewport it started at to the target over its duration.
#[derive(Debug, Clone, PartialEq)]
struct Transition {
    from: Viewport,
    target: TargetViewport,
    duration: f64,
    elapsed: f64,
    /// `AnimationRecord.lastTime`: 0 until the first frame ran.
    last_time: f64,
}

impl Transition {
    /// The animation callback: a frame, or the settled target once the
    /// duration has passed.
    fn step(&mut self, delta_time: f64) -> Result<Viewport, TargetViewport> {
        self.elapsed += delta_time;
        let progress = js::min(self.elapsed / self.duration, 1.0);
        let factor = ease_out(clamp(progress, 0.0, 1.0));
        if progress < 1.0 {
            Ok(interpolate_viewport(
                self.from,
                self.target.viewport,
                factor,
            ))
        } else {
            Err(self.target.clone())
        }
    }
}

/// `AppViewport`'s navigation (`App.viewport.ts:437-866`): the animated
/// `setViewport` transition, the frames that advance it and the user's
/// translations that interrupt it. Each method returns what to set in the
/// app state; the caller runs [`AppViewport::frame`] on every animation
/// frame while [`AppViewport::is_animating`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppViewport {
    transition: Option<Transition>,
}

/// What [`AppViewport::translate`] did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppTranslation {
    pub translation: Translation,
    /// A running navigation was interrupted: `shouldCacheIgnoreZoom` goes
    /// back to `false`.
    pub reset_should_cache_ignore_zoom: bool,
}

impl AppViewport {
    /// `isAnimating` for the navigation: a transition is running.
    pub fn is_animating(&self) -> bool {
        self.transition.is_some()
    }

    /// `isLockedTransitionPending` (`App.viewport.ts:461-465`): a
    /// transition into a locked viewport owns the user's pans and zooms.
    pub fn is_locked_transition_pending(&self) -> bool {
        self.transition
            .as_ref()
            .is_some_and(|t| t.target.scroll_constraints.is_some())
    }

    /// `cancelTransition` (`App.viewport.ts:860-863`).
    pub fn cancel(&mut self) {
        self.transition = None;
    }

    /// `setViewport(opts)` (`App.viewport.ts:672-760`) from `state`: `None`
    /// when the target did not resolve (nothing changes, a running
    /// transition goes on). Otherwise the running transition is replaced
    /// (upstream also stops following a collaborator and cancels the
    /// snap-back), and the patch is the target set at once without an
    /// animation, or the scroll lock cleared and the first frame.
    pub fn set_viewport(
        &mut self,
        state: &ViewportState,
        opts: &SetViewportOptions,
    ) -> Option<ViewportPatch> {
        let bounds = opts.target?;
        let target =
            get_constrained_target_viewport(state, bounds, opts.fit, opts.offsets, opts.lock);
        self.cancel();
        let Some(duration) = opts.animation.duration() else {
            return Some(ViewportPatch::settle(target));
        };
        // the old lock is superseded; the new one waits for the last frame
        let clear = ViewportPatch {
            scroll_constraints: state.scroll_constraints.as_ref().map(|_| None),
            ..ViewportPatch::default()
        };
        let mut transition = Transition {
            from: state.viewport(),
            target,
            duration,
            elapsed: 0.0,
            last_time: 0.0,
        };
        // AnimationController.start runs the first step at once
        Some(clear.then(match transition.step(0.0) {
            Ok(frame) => {
                self.transition = Some(transition);
                ViewportPatch::frame(frame)
            }
            Err(target) => ViewportPatch::settle(target),
        }))
    }

    /// An animation frame at `now` (ms, `performance.now()`): the next
    /// viewport of the running transition, or its target with the lock
    /// once it is over; `None` when none runs. The first frame after the
    /// start counts no time (`AnimationController.tick`).
    pub fn frame(&mut self, now: f64) -> Option<ViewportPatch> {
        let transition = self.transition.as_mut()?;
        let delta_time = if transition.last_time == 0.0 {
            0.0
        } else {
            now - transition.last_time
        };
        match transition.step(delta_time) {
            Ok(frame) => {
                transition.last_time = now;
                Some(ViewportPatch::frame(frame))
            }
            Err(target) => {
                self.transition = None;
                Some(ViewportPatch::settle(target))
            }
        }
    }

    /// `translate(update, opts)` (`App.viewport.ts:771-825`) for a user
    /// pan or zoom: `None` while a transition into a locked viewport is
    /// pending (upstream returns `false`); otherwise a running transition
    /// stops and [`translate`] runs.
    pub fn translate(
        &mut self,
        prev: &ViewportState,
        update: Option<ViewportUpdate>,
        opts: TranslateOptions,
    ) -> Option<AppTranslation> {
        let reset_should_cache_ignore_zoom = self.interrupt()?;
        Some(AppTranslation {
            translation: translate(prev, update, opts),
            reset_should_cache_ignore_zoom,
        })
    }

    /// The start of a user translation: `None` while a locked transition
    /// is pending (the translation is dropped), otherwise whether a
    /// running transition was stopped (`shouldCacheIgnoreZoom` goes back
    /// to `false`).
    pub fn interrupt(&mut self) -> Option<bool> {
        if self.is_locked_transition_pending() {
            return None;
        }
        let running = self.transition.is_some();
        self.cancel();
        Some(running)
    }
}

/// What [`scroll_bounds_into_view`] does along an axis where the bounds do
/// not fit (`tooLarge`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TooLarge {
    /// Bring their start (left / top) in.
    #[default]
    AlignStart,
    /// Keep that axis's scroll.
    Leave,
}

/// `scrollBoundsIntoView({bounds, appState, offsets, tooLarge})`
/// (`viewport.ts`): the scroll that brings the bounds into the viewport
/// less the offsets by the least movement, keeping the zoom; `None` when
/// they are in view already or the offsets leave no room.
pub fn scroll_bounds_into_view(
    bounds: Bounds,
    state: &ViewportState,
    offsets: &Offsets,
    too_large: TooLarge,
) -> Option<(f64, f64)> {
    let [x1, y1, x2, y2] = bounds;
    let view_left = offsets.left;
    let view_right = state.width - offsets.right;
    let view_top = offsets.top;
    let view_bottom = state.height - offsets.bottom;
    if view_right <= view_left || view_bottom <= view_top {
        return None;
    }
    let zoom = state.zoom;
    // how far (screen px) to move a span to bring it within [start, end]
    let shift = |span_start: f64, span_end: f64, scroll: f64, start: f64, end: f64| {
        let screen_start = (span_start + scroll) * zoom;
        let screen_end = (span_end + scroll) * zoom;
        if screen_end - screen_start > end - start {
            return match too_large {
                TooLarge::AlignStart => start - screen_start,
                TooLarge::Leave => 0.0,
            };
        }
        if screen_end > end {
            return end - screen_end;
        }
        if screen_start < start {
            return start - screen_start;
        }
        0.0
    };
    let dx = shift(x1, x2, state.scroll_x, view_left, view_right);
    let dy = shift(y1, y2, state.scroll_y, view_top, view_bottom);
    if falsy(dx) && falsy(dy) {
        return None;
    }
    Some((state.scroll_x + dx / zoom, state.scroll_y + dy / zoom))
}

/// `getVisibleElements(elements)` (`element/src/index.ts:47-53`): the
/// elements neither deleted nor invisibly small.
pub fn get_visible_elements<'a>(elements: &[&'a Element]) -> Vec<&'a Element> {
    elements
        .iter()
        .copied()
        .filter(|e| !e.base.is_deleted && !is_invisibly_small_element(e))
        .collect()
}

/// `getClosestElementBounds(elements, from)` (`viewport.ts`): the bounds of
/// the element whose bounds' centre is closest to `from` (the first on a
/// tie); `[0, 0, 0, 0]` for no elements.
pub fn get_closest_element_bounds(elements: &[&Element], from: (f64, f64)) -> Bounds {
    let Some(first) = elements.first() else {
        return [0.0, 0.0, 0.0, 0.0];
    };
    let elements_map = ElementsMap::new(elements.iter().copied());
    let mut min_distance = f64::INFINITY;
    let mut closest = *first;
    for element in elements {
        let [x1, y1, x2, y2] = get_element_bounds(element, &elements_map);
        let distance = js::hypot(from.0 - (x1 + x2) / 2.0, from.1 - (y1 + y2) / 2.0);
        if distance < min_distance {
            min_distance = distance;
            closest = element;
        }
    }
    get_element_bounds(closest, &elements_map)
}

/// `getScrollToContentState(elements, appState)` (`viewport.ts`): the
/// scroll that centres the visible elements, or, when they do not fit at
/// the current zoom, the one closest to the viewport centre; `(0, 0)` for
/// none.
pub fn get_scroll_to_content_state(elements: &[&Element], state: &ViewportState) -> (f64, f64) {
    let elements = get_visible_elements(elements);
    if elements.is_empty() {
        return (0.0, 0.0);
    }
    let mut bounds = get_common_bounds(&elements);
    let [x1, y1, x2, y2] = bounds;
    // doBoundsExceedViewport
    if (x2 - x1) * state.zoom > state.width || (y2 - y1) * state.zoom > state.height {
        let center = viewport_coords_to_scene_coords(
            state.offset_left + state.width / 2.0,
            state.offset_top + state.height / 2.0,
            state,
        );
        bounds = get_closest_element_bounds(&elements, center);
    }
    let [x1, y1, x2, y2] = bounds;
    center_scroll_on(
        ((x1 + x2) / 2.0, (y1 + y2) / 2.0),
        state.width,
        state.height,
        state.zoom,
        &Offsets::default(),
    )
}

// -- the wheel -------------------------------------------------------------------------

/// `InputDevice` (`types.ts:288`): `appState.inputDevice`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InputDevice {
    #[default]
    Auto,
    Mouse,
    Trackpad,
}

impl InputDevice {
    /// `"auto"`, `"mouse"` or `"trackpad"`.
    pub fn from_name(name: &str) -> Option<InputDevice> {
        match name {
            "auto" => Some(InputDevice::Auto),
            "mouse" => Some(InputDevice::Mouse),
            "trackpad" => Some(InputDevice::Trackpad),
            _ => None,
        }
    }

    /// The name upstream stores.
    pub fn name(self) -> &'static str {
        match self {
            InputDevice::Auto => "auto",
            InputDevice::Mouse => "mouse",
            InputDevice::Trackpad => "trackpad",
        }
    }

    /// `appState.inputDevice`; [`InputDevice::Auto`] when missing or unknown.
    pub fn from_app_state(app_state: &AppState) -> InputDevice {
        app_state
            .get("inputDevice")
            .and_then(Value::as_str)
            .and_then(InputDevice::from_name)
            .unwrap_or_default()
    }

    /// `resolveInputDevice` (`appState.ts:352-355`): `auto` is a trackpad
    /// until detection exists.
    pub fn resolve(self) -> InputDevice {
        match self {
            InputDevice::Auto => InputDevice::Trackpad,
            other => other,
        }
    }
}

/// What the wheel event is over (`event.target`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WheelTarget {
    /// One of the editor's canvases.
    #[default]
    Canvas,
    /// A text editor's textarea.
    TextArea,
    /// An embed's iframe.
    IFrame,
    /// A frame's name label (`CLASSES.FRAME_NAME`).
    FrameName,
    /// Anything else: menus, sidebars, dialogs.
    Other,
}

impl WheelTarget {
    /// `isOverEditorSurface` (`App.wheel.ts`).
    fn is_editor_surface(self) -> bool {
        !matches!(self, WheelTarget::Other)
    }
}

/// A wheel event, as the handler reads it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WheelEvent {
    pub delta_x: f64,
    pub delta_y: f64,
    pub ctrl_key: bool,
    pub meta_key: bool,
    pub shift_key: bool,
    /// `MouseEvent.buttons`.
    pub buttons: u16,
    pub target: WheelTarget,
}

/// The rest of the editor the wheel reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelContext {
    /// `app.isNavigationEnabled()`.
    pub navigation_enabled: bool,
    /// A drag-pan (wheel button, space+drag, hand tool) is in progress
    /// (`app.pan.isActive()`).
    pub pan_active: bool,
    /// `appState.inputDevice`.
    pub input_device: InputDevice,
    /// The last pointer position in the page (`viewport.lastPosition`):
    /// where a wheel zoom zooms around.
    pub last_position: (f64, f64),
    /// macOS / iOS (`isDarwin`): `KEYS.CTRL_OR_CMD` is `metaKey` there,
    /// `ctrlKey` elsewhere.
    pub is_darwin: bool,
}

impl Default for WheelContext {
    fn default() -> WheelContext {
        WheelContext {
            navigation_enabled: true,
            pan_active: false,
            input_device: InputDevice::Auto,
            last_position: (0.0, 0.0),
            is_darwin: false,
        }
    }
}

/// What a wheel event did.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WheelOutcome {
    /// `event.preventDefault()`.
    pub prevent_default: bool,
    /// Apply the drag-pan's pending pointer move before the zoom
    /// (`app.pan.flushMove()`).
    pub flush_pan_move: bool,
    /// The viewport translation, when the event panned or zoomed.
    pub translation: Option<Translation>,
    /// The zoom set `shouldCacheIgnoreZoom: true`: draw from zoom-scaled
    /// bitmaps while the gesture lasts.
    pub should_cache_ignore_zoom: bool,
    /// (Re)start the debounced reset of `shouldCacheIgnoreZoom`
    /// (`app.resetShouldCacheIgnoreZoomDebounced()`), after every zoom
    /// tick, the ones at a zoom limit included.
    pub reset_should_cache_ignore_zoom: bool,
}

/// The zoom a wheel tick of `delta_y` takes `zoom` to (`zoomBy`,
/// `App.wheel.ts`): `zoom - delta / 100 + log10(max(1, zoom)) * -sign(Δ) *
/// min(1, |Δ| / 20)`, with `delta` capped at `ZOOM_STEP * 100`, then
/// `getNormalizedZoom(max(newZoom, min_zoom))`. Positive deltas zoom out.
/// `min_zoom` is [`MIN_ZOOM`], or the zoom lock's zoom.
pub fn wheel_zoom_value(zoom: f64, delta_y: f64, min_zoom: f64) -> f64 {
    let sign = sign(delta_y);
    let max_step = ZOOM_STEP * 100.0;
    let abs_delta = delta_y.abs();
    let delta = if abs_delta > max_step {
        max_step * sign
    } else {
        delta_y
    };
    let mut new_zoom = zoom - delta / 100.0;
    // bigger steps the more zoomed in (above 100% only), less for the small
    // deltas of a trackpad
    new_zoom += js::log10(js::max(1.0, zoom)) * -sign * js::min(1.0, abs_delta / 20.0);
    get_normalized_zoom(js::max(new_zoom, min_zoom))
}

/// `AppWheel.handle(event)` (`App.wheel.ts`): pans the canvas, or zooms it
/// around the pointer on ctrl/cmd+wheel (a trackpad pinch), with the wheel
/// button held, or on a plain wheel from a mouse. Shift+wheel pans
/// horizontally, ctrl/cmd+shift+wheel vertically. Over anything but the
/// editor's surfaces only the browser's zoom is prevented.
pub fn handle_wheel(state: &ViewportState, ctx: &WheelContext, event: &WheelEvent) -> WheelOutcome {
    let mut outcome = WheelOutcome::default();
    if !ctx.navigation_enabled {
        return outcome;
    }
    if !event.target.is_editor_surface() {
        // prevent zooming the browser, but let the DOM scroll
        outcome.prevent_default = if ctx.is_darwin {
            event.meta_key
        } else {
            event.ctrl_key
        };
        return outcome;
    }
    outcome.prevent_default = true;

    let wheel_button_held = event.buttons & WHEEL_BUTTON_MASK != 0;
    // a drag-pan in progress owns the viewport, except for the wheel
    // button's own zoom, which lands after the pan's pending move
    if ctx.pan_active {
        if !wheel_button_held {
            return outcome;
        }
        outcome.flush_pan_move = true;
    }

    let (delta_x, delta_y) = (event.delta_x, event.delta_y);
    if falsy(delta_x) && falsy(delta_y) {
        return outcome;
    }
    // ctrlKey is how a pinch arrives
    let has_zoom_modifier = event.meta_key || event.ctrl_key;
    let should_zoom = delta_y != 0.0
        && (wheel_button_held
            || (!event.shift_key
                && (has_zoom_modifier || ctx.input_device.resolve() == InputDevice::Mouse)));
    if should_zoom {
        let min_zoom = state
            .scroll_constraints
            .as_ref()
            .map_or(MIN_ZOOM, ScrollConstraints::min_zoom);
        let next_zoom = wheel_zoom_value(state.zoom, delta_y, min_zoom);
        let update = if next_zoom == state.zoom {
            // at a zoom limit: nothing to do
            None
        } else {
            let (x, y) = ctx.last_position;
            outcome.should_cache_ignore_zoom = true;
            Some(get_viewport_for_zoom_with_scroll_constraints(x, y, next_zoom, state).into())
        };
        outcome.translation = Some(translate(
            state,
            update,
            TranslateOptions {
                zoom_pre_constrained: true,
                preserve_scroll_constraints_snap_back: true,
            },
        ));
        outcome.reset_should_cache_ignore_zoom = true;
        return outcome;
    }

    let zoom = state.zoom;
    let update = if event.shift_key {
        // on a Mac, shift+wheel tends to arrive as deltaX
        let delta = if falsy(delta_y) { delta_x } else { delta_y };
        ViewportUpdate {
            scroll_x: Some(if has_zoom_modifier {
                state.scroll_x
            } else {
                state.scroll_x - delta / zoom
            }),
            scroll_y: Some(if has_zoom_modifier {
                state.scroll_y - delta / zoom
            } else {
                state.scroll_y
            }),
            zoom: None,
        }
    } else {
        ViewportUpdate {
            scroll_x: Some(state.scroll_x - delta_x / zoom),
            scroll_y: Some(state.scroll_y - delta_y / zoom),
            zoom: None,
        }
    };
    outcome.translation = Some(translate(state, Some(update), TranslateOptions::default()));
    outcome
}

// -- zoom actions -------------------------------------------------------------------------

/// The zoom actions of `actions/actionCanvas.tsx`, in declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZoomAction {
    /// `actionZoomIn`: + [`ZOOM_STEP`] around the viewport centre.
    ZoomIn,
    /// `actionZoomOut`: - [`ZOOM_STEP`] around the viewport centre.
    ZoomOut,
    /// `actionResetZoom`: 100% (or a zoom lock's zoom) around the centre.
    ResetZoom,
    /// `actionZoomToFit`: every element (or the locked box), never past
    /// 100%.
    ZoomToFit,
    /// `actionZoomToFitSelection`: the selection (else every element, or
    /// the locked box), filling the viewport.
    ZoomToFitSelection,
    /// `actionZoomToFitSelectionInViewport`: the selection (else every
    /// element, or the locked box), never past 100%.
    ZoomToFitSelectionInViewport,
}

/// A keydown, as the zoom actions' `keyTest`s read it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZoomKeyEvent<'a> {
    /// `event.code`.
    pub code: &'a str,
    pub shift_key: bool,
    pub alt_key: bool,
    /// `event[KEYS.CTRL_OR_CMD]`: metaKey on a Mac, ctrlKey elsewhere.
    pub ctrl_or_cmd: bool,
}

impl ZoomAction {
    /// Every zoom action, in upstream's declaration order.
    pub const ALL: [ZoomAction; 6] = [
        ZoomAction::ZoomIn,
        ZoomAction::ZoomOut,
        ZoomAction::ResetZoom,
        ZoomAction::ZoomToFit,
        ZoomAction::ZoomToFitSelection,
        ZoomAction::ZoomToFitSelectionInViewport,
    ];

    /// The action's `name`.
    pub fn name(self) -> &'static str {
        match self {
            ZoomAction::ZoomIn => "zoomIn",
            ZoomAction::ZoomOut => "zoomOut",
            ZoomAction::ResetZoom => "resetZoom",
            ZoomAction::ZoomToFit => "zoomToFit",
            ZoomAction::ZoomToFitSelection => "zoomToFitSelection",
            ZoomAction::ZoomToFitSelectionInViewport => "zoomToFitSelectionInViewport",
        }
    }

    /// The action named `name`.
    pub fn from_name(name: &str) -> Option<ZoomAction> {
        ZoomAction::ALL.into_iter().find(|a| a.name() == name)
    }

    /// The action's `keyTest`: ctrl/cmd or shift with `=` / numpad `+`,
    /// `-` / numpad `-`, `0` / numpad `0`; shift+1, shift+3 and shift+2
    /// (without alt or ctrl/cmd) for the fits.
    pub fn key_test(self, event: &ZoomKeyEvent<'_>) -> bool {
        let code = event.code;
        let ctrl_or_shift = event.ctrl_or_cmd || event.shift_key;
        let shift_only = event.shift_key && !event.alt_key && !event.ctrl_or_cmd;
        match self {
            ZoomAction::ZoomIn => (code == "Equal" || code == "NumpadAdd") && ctrl_or_shift,
            ZoomAction::ZoomOut => (code == "Minus" || code == "NumpadSubtract") && ctrl_or_shift,
            ZoomAction::ResetZoom => (code == "Digit0" || code == "Numpad0") && ctrl_or_shift,
            ZoomAction::ZoomToFit => code == "Digit1" && shift_only,
            ZoomAction::ZoomToFitSelectionInViewport => code == "Digit2" && shift_only,
            ZoomAction::ZoomToFitSelection => code == "Digit3" && shift_only,
        }
    }

    /// The first action whose `keyTest` passes.
    pub fn for_key(event: &ZoomKeyEvent<'_>) -> Option<ZoomAction> {
        ZoomAction::ALL.into_iter().find(|a| a.key_test(event))
    }
}

/// JavaScript truthiness of a JSON value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| !falsy(x)),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// The action's `perform` (`actionCanvas.tsx`): the viewport it commits
/// (with `captureUpdate: EVENTUALLY`; every zoom action also requests to
/// stop following a collaborator). `elements` are the scene's elements,
/// `selected_element_ids` is `appState.selectedElementIds` and
/// `canvas_offsets` the UI covering the canvas edges
/// (`app.viewport.getOffsets()`). The actions are enabled only while
/// navigation is (`app.isNavigationEnabled()`); that is the caller's check.
pub fn perform_zoom_action(
    action: ZoomAction,
    state: &ViewportState,
    elements: &[&Element],
    selected_element_ids: &Map<String, Value>,
    canvas_offsets: &Offsets,
) -> Viewport {
    let center_x = state.width / 2.0 + state.offset_left;
    let center_y = state.height / 2.0 + state.offset_top;
    let zoom_around_center = |zoom: f64| {
        get_viewport_for_zoom_with_scroll_constraints(
            center_x,
            center_y,
            get_normalized_zoom(zoom),
            state,
        )
    };
    let non_deleted: Vec<&Element> = elements
        .iter()
        .copied()
        .filter(|e| !e.base.is_deleted)
        .collect();
    let everything = || match &state.scroll_constraints {
        Some(constraints) => constraints.bounds(),
        None => get_common_bounds(&non_deleted),
    };
    let selection_or_everything = || {
        let selected: Vec<&Element> = non_deleted
            .iter()
            .copied()
            .filter(|e| truthy(selected_element_ids.get(&e.base.id)))
            .collect();
        if selected.is_empty() {
            everything()
        } else {
            get_common_bounds(&selected)
        }
    };
    let (bounds, fit) = match action {
        ZoomAction::ZoomIn => return zoom_around_center(state.zoom + ZOOM_STEP),
        ZoomAction::ZoomOut => return zoom_around_center(state.zoom - ZOOM_STEP),
        // 100%, unless a zoom lock floors the zoom higher: then its zoom
        ZoomAction::ResetZoom => {
            let zoom = match &state.scroll_constraints {
                Some(c) if c.lock_zoom => c.zoom,
                _ => 1.0,
            };
            return zoom_around_center(zoom);
        }
        ZoomAction::ZoomToFit => (everything(), Fit::ScaleDown),
        ZoomAction::ZoomToFitSelection => (selection_or_everything(), Fit::Contain),
        ZoomAction::ZoomToFitSelectionInViewport => (selection_or_everything(), Fit::ScaleDown),
    };
    let fitted = zoom_to_fit_bounds(
        &ZoomToFit {
            fit,
            canvas_offsets: *canvas_offsets,
            ..ZoomToFit::new(bounds)
        },
        state,
    );
    // re-clamped so the fit cannot escape a scroll or zoom lock
    constrain_scroll_state(&state.with_viewport(fitted), 0.0)
}
