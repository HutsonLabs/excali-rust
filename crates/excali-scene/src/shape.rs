//! Shape construction for the box-like elements: rectangles, iframes and
//! embeddables, diamonds and ellipses (`_generateElementShape`,
//! `packages/element/src/shape.ts:761-889`), with the iframe-like colour
//! handling of `modifyIframeLikeForRoughOptions` (`shape.ts:262-293`); and
//! the body of lines and non-elbow arrows (`shape.ts:890-935`).
//!
//! See `site/content/research/rendering.md` section 2.
//!
//! - Rectangle, iframe, embeddable: rounded ones are an SVG path with
//!   quadratic `Q` corners of radius [`get_corner_radius`]`(min(w, h))`,
//!   drawn with `generator.path` as a continuous path; sharp ones are
//!   `generator.rectangle(0, 0, w, h)`.
//! - Diamond: rounded ones are a path with cubic `C` corners whose vertical
//!   and horizontal radii come from [`get_corner_radius`] of the diamond's
//!   half extents; sharp ones are `generator.polygon` of
//!   [`get_diamond_points`].
//! - Ellipse: `generator.ellipse(w / 2, h / 2, w, h)`; its options carry
//!   `curveFitting: 1` ([`generate_rough_options`]).
//! - Line, arrow ([`generate_linear_shape`]): sharp ones are
//!   `generator.polygon(points)` when the options carry a fill (a line whose
//!   points close into a loop, [`is_path_a_loop`](crate::utils::is_path_a_loop))
//!   and `generator.linearPath(points)` otherwise; round ones are
//!   `generator.curve(points)`. Empty points draw the point `[0, 0]`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use excali_core::color::is_transparent;
use excali_core::element::{Element, ElementKind, ElementType, FillStyle, LocalPoint};
use excali_core::json::number_to_string;
use excali_math::js;
use excali_rough::path_data::PathError;
use excali_rough::{Drawable, RoughGenerator};

use crate::bounds::get_diamond_points;
use crate::rough_options::{generate_rough_options, UnimplementedType};
use crate::utils::get_corner_radius;

/// `EmbedsValidationStatus` (`packages/element/src/types.ts`): whether each
/// embeddable's link, by element id, passed validation.
pub type EmbedsValidationStatus = HashMap<String, bool>;

/// `AppState["theme"]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

/// The render config `ShapeCache.generateElementShape` takes
/// (`shape.ts:120-128`). [`RenderConfig::default`] is the config upstream
/// uses when none is given (`shape.ts:145-150`): not exporting, a white
/// canvas, no validation status, light theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderConfig<'a> {
    pub is_exporting: bool,
    /// `canvasBackgroundColor`: the fill of outline arrowheads; the box
    /// shapes do not read it.
    pub canvas_background_color: &'a str,
    pub embeds_validation_status: Option<&'a EmbedsValidationStatus>,
    pub theme: Theme,
}

impl Default for RenderConfig<'_> {
    fn default() -> Self {
        RenderConfig {
            is_exporting: false,
            // COLOR_PALETTE.white
            canvas_background_color: "#ffffff",
            embeds_validation_status: None,
            theme: Theme::Light,
        }
    }
}

/// Why [`generate_element_shape`] or [`generate_linear_shape`] returned no
/// drawable.
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeError {
    /// The element is not a rectangle, iframe, embeddable, diamond or
    /// ellipse; the other types have their own builders (lines, arrows,
    /// freedraw) or no rough shape at all (text, image, frames).
    NotABoxShape(ElementType),
    /// [`generate_linear_shape`] was given an element that is not a line or
    /// an arrow.
    NotALinearShape(ElementType),
    /// [`generate_linear_shape`] was given an elbow arrow, whose body is the
    /// rounded path of `generateElbowArrowShape` (`shape.ts:900-920`), not a
    /// rough.js polyline or curve.
    ElbowArrow,
    /// rough.js rejected the path data (only reachable with non-finite
    /// sizes, which write `NaN` or `Infinity` into the path).
    Path(PathError),
    /// `generateRoughOptions` threw.
    Options(UnimplementedType),
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShapeError::NotABoxShape(ty) => write!(f, "{ty} is not a box shape"),
            ShapeError::NotALinearShape(ty) => write!(f, "{ty} is not a line or an arrow"),
            ShapeError::ElbowArrow => f.write_str("an elbow arrow is not a polyline or curve"),
            ShapeError::Path(e) => write!(f, "path data: {e}"),
            ShapeError::Options(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ShapeError {}

impl From<PathError> for ShapeError {
    fn from(e: PathError) -> Self {
        ShapeError::Path(e)
    }
}

impl From<UnimplementedType> for ShapeError {
    fn from(e: UnimplementedType) -> Self {
        ShapeError::Options(e)
    }
}

/// `modifyIframeLikeForRoughOptions(element, isExporting,
/// embedsValidationStatus)` (`shape.ts:262-293`).
///
/// - An iframe or embeddable whose stroke and background are both
///   transparent becomes a grey placeholder (roughness 0, solid `#d3d3d3`
///   fill) when exporting, and in the editor too for an embeddable whose
///   link has not been validated.
/// - Otherwise an iframe's transparent stroke becomes `#000000` and its
///   transparent background `#f4f4f6`.
/// - Every other element is returned as is.
pub fn modify_iframe_like_for_rough_options<'e>(
    element: &'e Element,
    is_exporting: bool,
    embeds_validation_status: Option<&EmbedsValidationStatus>,
) -> Cow<'e, Element> {
    let base = &element.base;
    let is_embeddable = matches!(element.kind, ElementKind::Embeddable);
    let is_iframe = matches!(element.kind, ElementKind::Iframe);
    let validated = || {
        embeds_validation_status
            .and_then(|status| status.get(&base.id))
            .copied()
            == Some(true)
    };
    if (is_iframe || is_embeddable)
        && (is_exporting || (is_embeddable && !validated()))
        && is_transparent(&base.background_color)
        && is_transparent(&base.stroke_color)
    {
        let mut placeholder = element.clone();
        placeholder.base.roughness = 0.0;
        placeholder.base.background_color = "#d3d3d3".to_owned();
        placeholder.base.fill_style = FillStyle::Solid;
        Cow::Owned(placeholder)
    } else if is_iframe {
        let mut iframe = element.clone();
        if is_transparent(&base.stroke_color) {
            iframe.base.stroke_color = "#000000".to_owned();
        }
        if is_transparent(&base.background_color) {
            iframe.base.background_color = "#f4f4f6".to_owned();
        }
        Cow::Owned(iframe)
    } else {
        Cow::Borrowed(element)
    }
}

/// The rounded rectangle path of `shape.ts:806-812`, numbers written as
/// JavaScript writes them in a template literal.
pub fn rectangle_path(w: f64, h: f64, r: f64) -> String {
    let n = number_to_string;
    format!(
        "M {r} 0 L {w_r} 0 Q {w} 0, {w} {r} L {w} {h_r} Q {w} {h}, {w_r} {h} \
         L {r} {h} Q 0 {h}, 0 {h_r} L 0 {r} Q 0 0, {r} 0",
        r = n(r),
        w = n(w),
        h = n(h),
        w_r = n(w - r),
        h_r = n(h - r),
    )
}

/// The rounded diamond path of `shape.ts:841-867` for the
/// [`get_diamond_points`] `points` and the corner radii, with the
/// template's line breaks and indentation.
pub fn diamond_path(points: [f64; 8], vertical_radius: f64, horizontal_radius: f64) -> String {
    let [top_x, top_y, right_x, right_y, bottom_x, bottom_y, left_x, left_y] = points;
    let (vr, hr) = (vertical_radius, horizontal_radius);
    let n = number_to_string;
    const BREAK: &str = "\n            ";
    [
        format!(
            "M {} {} L {} {}",
            n(top_x + vr),
            n(top_y + hr),
            n(right_x - vr),
            n(right_y - hr)
        ),
        format!(
            "C {x} {y}, {x} {y}, {} {}",
            n(right_x - vr),
            n(right_y + hr),
            x = n(right_x),
            y = n(right_y)
        ),
        format!("L {} {}", n(bottom_x + vr), n(bottom_y - hr)),
        format!(
            "C {x} {y}, {x} {y}, {} {}",
            n(bottom_x - vr),
            n(bottom_y - hr),
            x = n(bottom_x),
            y = n(bottom_y)
        ),
        format!("L {} {}", n(left_x + vr), n(left_y + hr)),
        format!(
            "C {x} {y}, {x} {y}, {} {}",
            n(left_x + vr),
            n(left_y - hr),
            x = n(left_x),
            y = n(left_y)
        ),
        format!("L {} {}", n(top_x - vr), n(top_y + hr)),
        format!(
            "C {x} {y}, {x} {y}, {} {}",
            n(top_x + vr),
            n(top_y + hr),
            x = n(top_x),
            y = n(top_y)
        ),
    ]
    .join(BREAK)
}

/// `_generateElementShape` (`shape.ts:761-889`) for rectangles, iframes,
/// embeddables, diamonds and ellipses: the rough.js drawable upstream's
/// `ShapeCache.generateElementShape(element, renderConfig)` returns,
/// computed afresh (the port's shape cache sits above this function).
pub fn generate_element_shape(
    element: &Element,
    generator: &RoughGenerator,
    config: &RenderConfig<'_>,
) -> Result<Drawable, ShapeError> {
    let is_dark_mode = config.theme == Theme::Dark;
    let defaults = generator.default_options();
    let base = &element.base;
    match element.kind {
        ElementKind::Rectangle | ElementKind::Iframe | ElementKind::Embeddable => {
            // this is for rendering the stroke/bg of the embeddable,
            // especially when the src url is not set
            let styled = modify_iframe_like_for_rough_options(
                element,
                config.is_exporting,
                config.embeds_validation_status,
            );
            if base.roundness.is_some() {
                let (w, h) = (base.width, base.height);
                let r = get_corner_radius(js::min(w, h), element);
                let o = generate_rough_options(&styled, true, is_dark_mode)?.to_rough(defaults);
                Ok(generator.path(&rectangle_path(w, h, r), &o)?)
            } else {
                let o = generate_rough_options(&styled, false, is_dark_mode)?.to_rough(defaults);
                Ok(generator.rectangle(0.0, 0.0, base.width, base.height, &o))
            }
        }
        ElementKind::Diamond => {
            let points = get_diamond_points(element);
            let [top_x, top_y, right_x, right_y, bottom_x, bottom_y, left_x, left_y] = points;
            if base.roundness.is_some() {
                let vertical_radius = get_corner_radius((top_x - left_x).abs(), element);
                let horizontal_radius = get_corner_radius((right_y - top_y).abs(), element);
                let o = generate_rough_options(element, true, is_dark_mode)?.to_rough(defaults);
                let d = diamond_path(points, vertical_radius, horizontal_radius);
                Ok(generator.path(&d, &o)?)
            } else {
                let o = generate_rough_options(element, false, is_dark_mode)?.to_rough(defaults);
                Ok(generator.polygon(
                    &[
                        [top_x, top_y],
                        [right_x, right_y],
                        [bottom_x, bottom_y],
                        [left_x, left_y],
                    ],
                    &o,
                ))
            }
        }
        ElementKind::Ellipse => {
            let o = generate_rough_options(element, false, is_dark_mode)?.to_rough(defaults);
            Ok(generator.ellipse(
                base.width / 2.0,
                base.height / 2.0,
                base.width,
                base.height,
                &o,
            ))
        }
        _ => Err(ShapeError::NotABoxShape(element.element_type())),
    }
}

/// The body `_generateElementShape` builds for a line or a non-elbow arrow
/// (`shape.ts:890-935`): the first shape of the element, which upstream
/// keeps first so the curve is easy to find ("curve is always the first
/// element"). An arrow's heads are pushed after it by
/// `getArrowheadShapes`.
///
/// - The options are `generateRoughOptions(element, false, isDarkMode)`:
///   a line fills only when its points close into a loop
///   ([`is_path_a_loop`](crate::utils::is_path_a_loop)), an arrow never.
/// - No roundness: `generator.polygon(points)` when `options.fill` is
///   truthy (so an empty background string draws no fill) and
///   `generator.linearPath(points)` otherwise.
/// - Roundness of any type: `generator.curve(points)`, which fills when the
///   options do.
/// - "points array can be empty in the beginning": empty points draw the
///   single point `[0, 0]`.
///
/// Errors with [`ShapeError::ElbowArrow`] for an elbow arrow and
/// [`ShapeError::NotALinearShape`] for any other type.
pub fn generate_linear_shape(
    element: &Element,
    generator: &RoughGenerator,
    config: &RenderConfig<'_>,
) -> Result<Drawable, ShapeError> {
    let linear = match &element.kind {
        ElementKind::Line(line) => &line.linear,
        ElementKind::Arrow(arrow) if arrow.elbowed => return Err(ShapeError::ElbowArrow),
        ElementKind::Arrow(arrow) => &arrow.linear,
        _ => return Err(ShapeError::NotALinearShape(element.element_type())),
    };
    let is_dark_mode = config.theme == Theme::Dark;
    let options =
        generate_rough_options(element, false, is_dark_mode)?.to_rough(generator.default_options());

    // points array can be empty in the beginning, so it is important to add
    // initial position to it
    const ORIGIN: [LocalPoint; 1] = [[0.0, 0.0]];
    let points: &[LocalPoint] = if linear.points.is_empty() {
        &ORIGIN
    } else {
        &linear.points
    };

    if element.base.roundness.is_some() {
        Ok(generator.curve(points, &options)?)
    } else if options.fill.as_deref().is_some_and(|fill| !fill.is_empty()) {
        Ok(generator.polygon(points, &options))
    } else {
        Ok(generator.linear_path(points, &options))
    }
}
