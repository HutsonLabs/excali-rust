//! The binding highlight: the outline the interactive canvas strokes
//! around the element an arrow end would bind to (`suggestedBinding`), and
//! the side midpoints the end can snap to, as
//! `renderBindingHighlightForBindableElement_simple`
//! (`packages/excalidraw/renderer/interactiveScene.ts:258-523`) draws them.
//!
//! [`binding_highlight`] answers what to draw; a canvas draws it in scene
//! coordinates (upstream has translated by the scroll):
//!
//! 1. an element in a frame is clipped to the frame: `translate(frame.x,
//!    frame.y)`, `roundRect(-1, -1, width + 1, height + 1, radius)`,
//!    `clip()`, `translate(-frame.x, -frame.y)` ([`FrameClip`]);
//! 2. the outline, `lineWidth` [`BindingHighlight::line_width`] in
//!    [`BindingHighlight::color`] ([`HighlightOutline`]);
//! 3. each of [`BindingHighlight::midpoints`] a disc of
//!    [`BindingHighlight::midpoint_radius`] in
//!    [`BindingHighlight::midpoint_color`], then the highlighted midpoint in
//!    the highlight colour.
//!
//! Which element is highlighted is the binding strategy's target (see
//! [`crate::binding`] and [`crate::collision::get_hovered_element_for_binding`]).

use excali_core::element::ElementKind;
use excali_math::{js, point_from, points_equal, GlobalPoint};
use excali_scene::bounds::{element_center_point, ElementsMap};
use excali_scene::frame::is_frame_like;
use excali_scene::shape::Theme;
use excali_scene::utils::{deconstruct_diamond_element, deconstruct_rectanguloid_element};

use crate::binding::{get_all_midpoints, max_binding_distance_simple};
use crate::collision::{hit_element_itself, HitTestArgs, HitTestCache};

type P = [f64; 2];

/// `FRAME_STYLE.radius` (`common/src/constants.ts:214`).
const FRAME_RADIUS: f64 = 8.0;

/// `FRAME_STYLE.strokeWidth` (`common/src/constants.ts:208`).
const FRAME_STROKE_WIDTH: f64 = 2.0;

/// `BINDING_HIGHLIGHT_RGB` (`interactiveScene.ts:124-127`).
pub fn binding_highlight_rgb(theme: Theme) -> &'static str {
    match theme {
        Theme::Light => "106, 189, 252",
        Theme::Dark => "104, 182, 240",
    }
}

/// `BINDING_MIDPOINT_COLOR` (`interactiveScene.ts:129-132`).
pub fn binding_midpoint_color(theme: Theme) -> &'static str {
    match theme {
        Theme::Light => "rgba(65, 65, 65, 0.5)",
        Theme::Dark => "rgba(237, 237, 237, 0.8)",
    }
}

/// `AppState["suggestedBinding"]`: the element to highlight and, when the
/// end snapped to one, the midpoint it snapped to.
#[derive(Debug, Clone, PartialEq)]
pub struct SuggestedBinding {
    pub element_id: String,
    pub mid_point: Option<P>,
}

/// The app state the highlight reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighlightAppState {
    /// `zoom.value`.
    pub zoom: f64,
    pub theme: Theme,
    pub is_midpoint_snapping_enabled: bool,
    pub grid_mode_enabled: bool,
    /// The arrow is an elbow arrow: the selected linear element is one, or
    /// the arrow tool is active with the elbow arrow type
    /// (`currentItemArrowType === "elbow"`).
    pub elbow: bool,
}

/// The clip to an element's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameClip {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// `FRAME_STYLE.radius / zoom`.
    pub radius: f64,
}

/// The outline stroked around the element.
#[derive(Debug, Clone, PartialEq)]
pub enum HighlightOutline {
    /// A frame: `translate(x, y)`, then `roundRect(0, 0, width, height,
    /// radius)`.
    Frame {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        radius: f64,
    },
    /// An ellipse: `translate(center)`, `rotate(angle)`,
    /// `translate(-center)`, `translate(x, y)`, then `ellipse(width / 2,
    /// height / 2, width / 2, height / 2, 0, 0, 2π)`.
    Ellipse {
        center: P,
        angle: f64,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    /// Anything else, under the ellipse's transform: each side stroked on
    /// its own (`moveTo`, `lineTo`), then each corner (`moveTo`,
    /// `bezierCurveTo`), relative to `(x, y)`.
    Outline {
        center: P,
        angle: f64,
        x: f64,
        y: f64,
        sides: Vec<[P; 2]>,
        corners: Vec<[P; 4]>,
    },
}

/// What the highlight draws.
#[derive(Debug, Clone, PartialEq)]
pub struct BindingHighlight {
    pub frame_clip: Option<FrameClip>,
    pub outline: HighlightOutline,
    /// `rgba(<BINDING_HIGHLIGHT_RGB>, 1)`.
    pub color: String,
    pub line_width: f64,
    /// Whether the midpoints are drawn at all (midpoint snapping on, no
    /// grid, no angle lock, and the pointer outside the element or an
    /// elbow arrow): a `save()`/`restore()` pair even with none.
    pub draws_midpoints: bool,
    /// The midpoints drawn in [`Self::midpoint_color`], in order.
    pub midpoints: Vec<P>,
    /// The midpoint the end snapped to, drawn last in [`Self::color`].
    pub highlighted_midpoint: Option<P>,
    /// `4 / zoom`.
    pub midpoint_radius: f64,
    pub midpoint_color: &'static str,
}

fn gp(p: P) -> GlobalPoint {
    point_from(p[0], p[1])
}

fn xy(p: GlobalPoint) -> P {
    [p.x, p.y]
}

fn distance(a: P, b: P) -> f64 {
    js::hypot(b[0] - a[0], b[1] - a[1])
}

/// `renderBindingHighlightForBindableElement_simple(context,
/// suggestedBinding, elementsMap, appState, pointerCoords, angleLocked)`
/// (`interactiveScene.ts:258-523`). `None` when the suggested element is
/// not in the map.
pub fn binding_highlight(
    suggested: &SuggestedBinding,
    elements_map: &ElementsMap<'_>,
    app_state: &HighlightAppState,
    pointer: Option<P>,
    angle_locked: bool,
) -> Option<BindingHighlight> {
    let element = elements_map.get(&suggested.element_id)?;
    let b = &element.base;
    let zoom = app_state.zoom;

    let frame_clip = b
        .frame_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .and_then(|id| elements_map.get(id))
        .filter(|frame| is_frame_like(frame))
        .map(|frame| FrameClip {
            x: frame.base.x,
            y: frame.base.y,
            width: frame.base.width,
            height: frame.base.height,
            radius: FRAME_RADIUS / zoom,
        });

    let color = format!("rgba({}, 1)", binding_highlight_rgb(app_state.theme));
    let (outline, line_width) = if is_frame_like(element) {
        (
            HighlightOutline::Frame {
                x: b.x,
                y: b.y,
                width: b.width,
                height: b.height,
                radius: FRAME_RADIUS / zoom,
            },
            FRAME_STROKE_WIDTH / zoom,
        )
    } else {
        let center = element_center_point(element, elements_map);
        // clamp(1.75, strokeWidth, 4): upstream's argument order, so at
        // least the stroke width, at most 4
        let line_width = excali_math::clamp(1.75, b.stroke_width, 4.0) / js::max(0.25, zoom);
        let outline = match element.kind {
            ElementKind::Ellipse => HighlightOutline::Ellipse {
                center,
                angle: b.angle.0,
                x: b.x,
                y: b.y,
                width: b.width,
                height: b.height,
            },
            _ => {
                let shape = if matches!(element.kind, ElementKind::Diamond) {
                    deconstruct_diamond_element(element)
                } else {
                    deconstruct_rectanguloid_element(element)
                };
                HighlightOutline::Outline {
                    center,
                    angle: b.angle.0,
                    x: b.x,
                    y: b.y,
                    sides: shape.sides.iter().map(|s| [xy(s.0), xy(s.1)]).collect(),
                    corners: shape
                        .corners
                        .iter()
                        .map(|c| [xy(c.0), xy(c.1), xy(c.2), xy(c.3)])
                        .collect(),
                }
            }
        };
        (outline, line_width)
    };

    // Draw midpoint indicators
    let mut draws_midpoints = false;
    let mut midpoints = Vec::new();
    let mut highlighted_midpoint = None;
    if app_state.is_midpoint_snapping_enabled
        && !app_state.grid_mode_enabled
        && !angle_locked
        && (is_frame_like(element) || element.is_bindable())
    {
        let cursor_is_inside_bindable = pointer.is_some_and(|p| {
            hit_element_itself(
                &mut HitTestCache::new(),
                &HitTestArgs {
                    point: p,
                    element,
                    threshold: 0.0,
                    elements_map,
                    frame_name_bound: None,
                    override_should_test_inside: true,
                },
            )
        });
        // Simple arrows only snap to midpoints from outside the element
        if !cursor_is_inside_bindable || app_state.elbow {
            draws_midpoints = true;
            let all = get_all_midpoints(element, elements_map);
            // Elbow arrows show all midpoints, simple arrows only the one
            // closest to the pointer, once the pointer gets near it
            let shown = if app_state.elbow {
                all
            } else {
                let threshold = max_binding_distance_simple(zoom) + b.stroke_width / 2.0;
                let closest = pointer.and_then(|p| {
                    all.iter()
                        .copied()
                        .reduce(|a, c| {
                            if distance(a, p) <= distance(c, p) {
                                a
                            } else {
                                c
                            }
                        })
                        .filter(|&c| distance(c, p) <= threshold * 2.0)
                });
                closest.into_iter().collect()
            };
            let highlighted = suggested.mid_point;
            midpoints = shown
                .into_iter()
                .filter(|&m| highlighted.is_none_or(|h| !points_equal(gp(m), gp(h))))
                .collect();
            highlighted_midpoint = highlighted;
        }
    }

    Some(BindingHighlight {
        frame_clip,
        outline,
        color,
        line_width,
        draws_midpoints,
        midpoints,
        highlighted_midpoint,
        midpoint_radius: 4.0 / zoom,
        midpoint_color: binding_midpoint_color(app_state.theme),
    })
}
