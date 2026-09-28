//! Shape construction for the box-like elements: rectangles, iframes and
//! embeddables, diamonds and ellipses (`_generateElementShape`,
//! `packages/element/src/shape.ts:761-889`), with the iframe-like colour
//! handling of `modifyIframeLikeForRoughOptions` (`shape.ts:262-293`); and
//! the body of lines and arrows (`shape.ts:890-935`) and the arrowheads
//! after it (`shape.ts:295-577, 936-975`).
//!
//! The freedraw shapes ([`generate_freedraw_shapes`], `shape.ts:976-994`)
//! are the rough.js background fill of a loop, then the stroke path.
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
//!   points close into a loop, [`is_path_a_loop`])
//!   and `generator.linearPath(points)` otherwise; round ones are
//!   `generator.curve(points)`. Empty points draw the point `[0, 0]`.
//! - Elbow arrow ([`generate_elbow_arrow_shape`]): `generator.path` of
//!   [`elbow_arrow_path`] with corner radius
//!   [`ELBOW_ARROW_CORNER_RADIUS`], as a continuous path; nothing when a
//!   coordinate is beyond [`ELBOW_ARROW_MAX_COORDINATE`](crate::elbow_arrow::ELBOW_ARROW_MAX_COORDINATE).
//! - Arrowheads ([`generate_linear_element_shapes`],
//!   [`get_arrowhead_shapes`], `shape.ts:295-577, 936-975`): an arrow's
//!   start and end heads are pushed after its body, placed by
//!   [`get_arrowhead_points`](crate::bounds::get_arrowhead_points). Line
//!   heads cap roughness at 1 and are solid (dotted arrows excepted),
//!   circles cap it at 0.5, and outline variants fill with the canvas
//!   background.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use excali_core::color::{apply_dark_mode_filter, is_transparent};
use excali_core::element::{
    Arrowhead, Element, ElementKind, ElementType, FillStyle, LocalPoint, StrokeStyle,
};
use excali_core::json::number_to_string;
use excali_math::js;
use excali_rough::path_data::PathError;
use excali_rough::{Drawable, Options as RoughJsOptions, RoughGenerator};

use crate::bounds::{
    get_arrowhead_points, get_diamond_points, ArrowheadPoints, ArrowheadPosition,
    InvalidArrowheadOp,
};
use crate::elbow_arrow::{elbow_arrow_is_drawable, elbow_arrow_path, ELBOW_ARROW_CORNER_RADIUS};
use crate::freedraw::{get_free_draw_svg_path, get_freedraw_fill_curve_points};
use crate::rough_options::{dash_array_dotted, generate_rough_options, UnimplementedType};
use crate::utils::{get_corner_radius, is_path_a_loop};

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
    /// rough.js polyline or curve; [`generate_elbow_arrow_shape`] builds it.
    ElbowArrow,
    /// [`generate_elbow_arrow_shape`] was given an element that is not an
    /// elbow arrow (a plain arrow, a line, or another type).
    NotAnElbowArrow(ElementType),
    /// [`generate_freedraw_shapes`] was given an element that is not a
    /// freedraw.
    NotAFreedraw(ElementType),
    /// rough.js rejected the path data (only reachable with non-finite
    /// sizes, which write `NaN` or `Infinity` into the path).
    Path(PathError),
    /// `generateRoughOptions` threw.
    Options(UnimplementedType),
    /// `getArrowheadPoints` threw: the body op it reads is not a bezier
    /// curve.
    Arrowhead(InvalidArrowheadOp),
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShapeError::NotABoxShape(ty) => write!(f, "{ty} is not a box shape"),
            ShapeError::NotALinearShape(ty) => write!(f, "{ty} is not a line or an arrow"),
            ShapeError::ElbowArrow => f.write_str("an elbow arrow is not a polyline or curve"),
            ShapeError::NotAnElbowArrow(ty) => write!(f, "{ty} is not an elbow arrow"),
            ShapeError::NotAFreedraw(ty) => write!(f, "{ty} is not a freedraw"),
            ShapeError::Path(e) => write!(f, "path data: {e}"),
            ShapeError::Options(e) => e.fmt(f),
            ShapeError::Arrowhead(e) => write!(f, "arrowhead: {e}"),
        }
    }
}

impl std::error::Error for ShapeError {}

impl From<PathError> for ShapeError {
    fn from(e: PathError) -> Self {
        ShapeError::Path(e)
    }
}

impl From<InvalidArrowheadOp> for ShapeError {
    fn from(e: InvalidArrowheadOp) -> Self {
        ShapeError::Arrowhead(e)
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
/// [`get_arrowhead_shapes`] ([`generate_linear_element_shapes`]).
///
/// - The options are `generateRoughOptions(element, false, isDarkMode)`:
///   a line fills only when its points close into a loop
///   ([`is_path_a_loop`]), an arrow never.
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
    Ok(linear_body(element, generator, config)?.0)
}

/// [`generate_linear_shape`], and the rough.js options (over the
/// generator's defaults) the body was drawn with, which the heads start
/// from.
fn linear_body(
    element: &Element,
    generator: &RoughGenerator,
    config: &RenderConfig<'_>,
) -> Result<(Drawable, RoughJsOptions), ShapeError> {
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

    let body = if element.base.roundness.is_some() {
        generator.curve(points, &options)?
    } else if options.fill.as_deref().is_some_and(|fill| !fill.is_empty()) {
        generator.polygon(points, &options)
    } else {
        generator.linear_path(points, &options)
    };
    Ok((body, options))
}

/// The shapes `_generateElementShape` builds for a line or an arrow
/// (`shape.ts:890-975`): the body first, then for an arrow the start
/// head's shapes and the end head's ([`get_arrowhead_shapes`]). Lines never
/// get heads, whatever their `startArrowhead` and `endArrowhead` say.
///
/// The body is [`generate_linear_shape`]'s, or for an elbow arrow
/// [`generate_elbow_arrow_shape`]'s; an elbow arrow too far out to draw has
/// no body and so no heads either (`getArrowheadPoints` finds no curve).
/// The heads start from `generateRoughOptions(element, false, isDarkMode)`
/// for every arrow, elbowed or not.
///
/// Errors as those builders do, and with [`ShapeError::Arrowhead`] where
/// `getArrowheadPoints` would throw.
pub fn generate_linear_element_shapes(
    element: &Element,
    generator: &RoughGenerator,
    config: &RenderConfig<'_>,
) -> Result<Vec<Drawable>, ShapeError> {
    let (mut shape, options) = match &element.kind {
        ElementKind::Arrow(arrow) if arrow.elbowed => {
            let options = generate_rough_options(element, false, config.theme == Theme::Dark)?
                .to_rough(generator.default_options());
            let body = generate_elbow_arrow_shape(element, generator, config)?;
            (body.into_iter().collect::<Vec<_>>(), options)
        }
        _ => {
            let (body, options) = linear_body(element, generator, config)?;
            (vec![body], options)
        }
    };

    // add lines only in arrow
    if let ElementKind::Arrow(arrow) = &element.kind {
        // `const { startArrowhead = null, endArrowhead = "arrow" } =
        // element`: an arrow without the key ("Hey, we have an old arrow
        // here!") gets the default arrow head; null means none
        let end_arrowhead = match arrow.linear.end_arrowhead {
            None if !element.has_key("endArrowhead") => Some(Arrowhead::Arrow),
            end => end,
        };
        let heads = [
            (ArrowheadPosition::Start, arrow.linear.start_arrowhead),
            (ArrowheadPosition::End, end_arrowhead),
        ];
        for (position, arrowhead) in heads {
            if let Some(arrowhead) = arrowhead {
                let shapes = get_arrowhead_shapes(
                    element, &shape, position, arrowhead, generator, &options, config,
                )?;
                shape.extend(shapes);
            }
        }
    }
    Ok(shape)
}

// ---------------------------------------------------------------------------
// Freedraw (`shape.ts:976-994`)

/// One of a freedraw element's shapes (`ElementShapes["freedraw"]`,
/// `(Drawable | SVGPathString)[]`).
#[derive(Clone, Debug, PartialEq)]
pub enum FreedrawShape {
    /// The rough.js background fill of a loop (boxed: a drawable is
    /// several times the size of a path string).
    Rough(Box<Drawable>),
    /// The stroke: the outline as an SVG path the renderer fills with the
    /// stroke colour ([`get_free_draw_svg_path`]).
    SvgPath(String),
}

/// The shapes `_generateElementShape` builds for a freedraw
/// (`shape.ts:976-994`), "oredered in terms of z-index [background,
/// stroke]":
///
/// 1. the background fill, only when the points close into a loop
///    ([`is_path_a_loop`] at zoom 1): `generator.curve` over
///    [`get_freedraw_fill_curve_points`] (the points simplified to 0.75)
///    with `generateRoughOptions(element, false, isDarkMode)` (which carry
///    the fill style and fill for loops) and `stroke: "none"`, so the curve
///    draws its fill and no outline;
/// 2. the stroke, [`get_free_draw_svg_path`].
///
/// Errors with [`ShapeError::NotAFreedraw`] for any other element type,
/// and with [`ShapeError::Path`] where rough.js throws.
pub fn generate_freedraw_shapes(
    element: &Element,
    generator: &RoughGenerator,
    config: &RenderConfig<'_>,
) -> Result<Vec<FreedrawShape>, ShapeError> {
    let (ElementKind::Freedraw(fields), Some(fill_points), Ok(stroke)) = (
        &element.kind,
        get_freedraw_fill_curve_points(element),
        get_free_draw_svg_path(element),
    ) else {
        return Err(ShapeError::NotAFreedraw(element.element_type()));
    };
    let mut shapes = Vec::with_capacity(2);

    // (1) background fill (rc shape), optional
    if is_path_a_loop(&fields.points, 1.0) {
        // generate rough polygon to fill freedraw shape
        let is_dark_mode = config.theme == Theme::Dark;
        let options = RoughJsOptions {
            stroke: "none".to_owned(),
            ..generate_rough_options(element, false, is_dark_mode)?
                .to_rough(generator.default_options())
        };
        shapes.push(FreedrawShape::Rough(Box::new(
            generator.curve(&fill_points, &options)?,
        )));
    }

    // (2) stroke
    shapes.push(FreedrawShape::SvgPath(stroke));
    Ok(shapes)
}

// ---------------------------------------------------------------------------
// Arrowheads (`shape.ts:295-577`)

/// `options.roughness || 0`: NaN and zero read as 0.
fn roughness_or_zero(options: &RoughJsOptions) -> f64 {
    let r = options.roughness;
    if r == 0.0 || r.is_nan() {
        0.0
    } else {
        r
    }
}

/// The body `_generateElementShape` builds for an elbow arrow
/// (`shape.ts:900-920`): `generator.path(generateElbowArrowShape(points,
/// 16), generateRoughOptions(element, true, isDarkMode))`, a continuous
/// path (so `preserveVertices` holds at any roughness) that never fills.
/// Its heads are pushed after it by `getArrowheadShapes`, as for any arrow.
///
/// - "points array can be empty in the beginning": empty points draw the
///   path of the single point `[0, 0]`.
/// - `Ok(None)` when a coordinate's absolute value is above
///   [`ELBOW_ARROW_MAX_COORDINATE`](crate::elbow_arrow::ELBOW_ARROW_MAX_COORDINATE) or NaN: upstream logs "Elbow arrow with
///   extreme point positions detected. Arrow not rendered." and gives the
///   element no shapes at all.
///
/// Errors with [`ShapeError::NotAnElbowArrow`] for anything but an elbow
/// arrow.
pub fn generate_elbow_arrow_shape(
    element: &Element,
    generator: &RoughGenerator,
    config: &RenderConfig<'_>,
) -> Result<Option<Drawable>, ShapeError> {
    let linear = match &element.kind {
        ElementKind::Arrow(arrow) if arrow.elbowed => &arrow.linear,
        _ => return Err(ShapeError::NotAnElbowArrow(element.element_type())),
    };
    const ORIGIN: [LocalPoint; 1] = [[0.0, 0.0]];
    let points: &[LocalPoint] = if linear.points.is_empty() {
        &ORIGIN
    } else {
        &linear.points
    };
    // NOTE (mtolmacs): Temporary fix for extremely big arrow shapes
    if !elbow_arrow_is_drawable(points) {
        return Ok(None);
    }
    let is_dark_mode = config.theme == Theme::Dark;
    let options =
        generate_rough_options(element, true, is_dark_mode)?.to_rough(generator.default_options());
    let d = elbow_arrow_path(points, ELBOW_ARROW_CORNER_RADIUS);
    Ok(Some(generator.path(&d, &options)?))
}

/// `generateArrowheadCardinalityOne` (`shape.ts:295-306`): one line across
/// the shaft, between the two wing ends.
fn arrowhead_cardinality_one(
    generator: &RoughGenerator,
    arrowhead_points: Option<ArrowheadPoints>,
    line_options: &RoughJsOptions,
) -> Vec<Drawable> {
    let Some(points) = arrowhead_points else {
        return Vec::new();
    };
    let [_, _, x3, y3, x4, y4, ..] = *points.as_slice() else {
        return Vec::new();
    };
    vec![generator.line(x3, y3, x4, y4, line_options)]
}

/// `generateArrowheadLinesToTip` (`shape.ts:308-323`): a line from each
/// wing end to the tip.
fn arrowhead_lines_to_tip(
    generator: &RoughGenerator,
    arrowhead_points: Option<ArrowheadPoints>,
    line_options: &RoughJsOptions,
) -> Vec<Drawable> {
    let Some(points) = arrowhead_points else {
        return Vec::new();
    };
    let [x2, y2, x3, y3, x4, y4, ..] = *points.as_slice() else {
        return Vec::new();
    };
    vec![
        generator.line(x3, y3, x2, y2, line_options),
        generator.line(x4, y4, x2, y2, line_options),
    ]
}

/// `getArrowheadLineOptions(element, options)` (`shape.ts:325-343`): the
/// arrow's options with roughness at most 1 and a solid dash, except for a
/// dotted arrow, whose line heads use `[d0, d1 - 1]` of
/// `getDashArrayDotted(strokeWidth - 1)` ("reduce gap to make it more
/// legible").
pub fn arrowhead_line_options(element: &Element, options: &RoughJsOptions) -> RoughJsOptions {
    let mut line_options = options.clone();
    if element.base.stroke_style == StrokeStyle::Dotted {
        let dash = dash_array_dotted(element.base.stroke_width - 1.0);
        line_options.stroke_line_dash = Some(vec![dash[0], dash[1] - 1.0]);
    } else {
        // for solid/dashed, keep solid arrow cap
        line_options.stroke_line_dash = None;
    }
    line_options.roughness = js::min(1.0, roughness_or_zero(options));
    line_options
}

/// `generateArrowheadOutlineCircle` (`shape.ts:345-369`): a solid-filled
/// circle of `diameter * diameter_scale` stroked with the stroke colour,
/// roughness at most 0.5, never dashed.
fn arrowhead_outline_circle(
    generator: &RoughGenerator,
    options: &RoughJsOptions,
    stroke_color: &str,
    arrowhead_points: Option<ArrowheadPoints>,
    fill: &str,
    diameter_scale: f64,
) -> Vec<Drawable> {
    let Some(points) = arrowhead_points else {
        return Vec::new();
    };
    let [x, y, diameter, ..] = *points.as_slice() else {
        return Vec::new();
    };
    let mut circle_options = options.clone();
    circle_options.fill = Some(fill.to_owned());
    circle_options.fill_style = "solid".to_owned();
    circle_options.stroke = stroke_color.to_owned();
    circle_options.roughness = js::min(0.5, roughness_or_zero(options));
    circle_options.stroke_line_dash = None;
    vec![generator.circle(x, y, diameter * diameter_scale, &circle_options)]
}

/// The options of a solid polygon head (`shape.ts:421-431, 453-463`):
/// filled solid with `fill`, roughness at most 1, always a solid stroke.
fn arrowhead_polygon_options(options: &RoughJsOptions, fill: &str) -> RoughJsOptions {
    let mut polygon_options = options.clone();
    polygon_options.fill = Some(fill.to_owned());
    polygon_options.fill_style = "solid".to_owned();
    polygon_options.roughness = js::min(1.0, roughness_or_zero(options));
    // always use solid stroke for arrowhead
    polygon_options.stroke_line_dash = None;
    polygon_options
}

/// `getArrowheadShapes(element, shape, position, arrowhead, generator,
/// options, canvasBackgroundColor, isDarkMode)` (`shape.ts:371-577`): the
/// rough.js shapes of the head of kind `arrowhead` at `position` of the
/// linear `element`, whose shapes so far are `shape` (the body first) and
/// whose body was drawn with `options`.
///
/// | kind | shapes |
/// |---|---|
/// | `arrow`, `bar` | two lines from the wing ends to the tip |
/// | `circle`, `circle_outline` | a circle on the tip |
/// | `triangle`, `triangle_outline` | the polygon tip, wing, wing, tip |
/// | `diamond`, `diamond_outline` | the polygon tip, wing, far vertex, wing, tip |
/// | `cardinality_one` | one line across the shaft |
/// | `cardinality_many` | a crowfoot: two lines |
/// | `cardinality_one_or_many` | a crowfoot, and a line across at offset -0.25 |
/// | `cardinality_exactly_one` | lines across at offsets -0.5 and 0 |
/// | `cardinality_zero_or_one` | a circle at offset 1.5 scaled 0.8, a line across at -0.5 |
/// | `cardinality_zero_or_many` | a crowfoot, a circle at offset 1.5 scaled 0.8 |
///
/// The stroke colour and the canvas background go through the dark mode
/// filter when the theme is dark. Filled heads (circle, triangle, diamond)
/// fill with the stroke colour; the `_outline` kinds, and the circles of
/// the `zero` cardinalities, fill with the canvas background. Line heads
/// use [`arrowhead_line_options`].
pub fn get_arrowhead_shapes(
    element: &Element,
    shape: &[Drawable],
    position: ArrowheadPosition,
    arrowhead: Arrowhead,
    generator: &RoughGenerator,
    options: &RoughJsOptions,
    config: &RenderConfig<'_>,
) -> Result<Vec<Drawable>, ShapeError> {
    let is_dark_mode = config.theme == Theme::Dark;
    let stroke_color = apply_dark_mode_filter(&element.base.stroke_color, is_dark_mode);
    let background_fill_color =
        apply_dark_mode_filter(config.canvas_background_color, is_dark_mode);
    const CARDINALITY_ONE_OR_MANY_OFFSET: f64 = -0.25;
    const CARDINALITY_ZERO_CIRCLE_SCALE: f64 = 0.8;

    let points =
        |kind: Arrowhead, offset: f64| get_arrowhead_points(element, shape, position, kind, offset);
    let outline_fill = |outline: bool| {
        if outline {
            background_fill_color.as_str()
        } else {
            stroke_color.as_str()
        }
    };

    Ok(match arrowhead {
        Arrowhead::Circle | Arrowhead::CircleOutline => arrowhead_outline_circle(
            generator,
            options,
            &stroke_color,
            points(arrowhead, 0.0)?,
            outline_fill(arrowhead == Arrowhead::CircleOutline),
            1.0,
        ),
        Arrowhead::Triangle | Arrowhead::TriangleOutline => {
            let Some(p) = points(arrowhead, 0.0)? else {
                return Ok(Vec::new());
            };
            let [x, y, x2, y2, x3, y3, ..] = *p.as_slice() else {
                return Ok(Vec::new());
            };
            let triangle_options = arrowhead_polygon_options(
                options,
                outline_fill(arrowhead == Arrowhead::TriangleOutline),
            );
            vec![generator.polygon(&[[x, y], [x2, y2], [x3, y3], [x, y]], &triangle_options)]
        }
        Arrowhead::Diamond | Arrowhead::DiamondOutline => {
            let Some(p) = points(arrowhead, 0.0)? else {
                return Ok(Vec::new());
            };
            let [x, y, x2, y2, x3, y3, x4, y4] = *p.as_slice() else {
                return Ok(Vec::new());
            };
            let diamond_options = arrowhead_polygon_options(
                options,
                outline_fill(arrowhead == Arrowhead::DiamondOutline),
            );
            vec![generator.polygon(
                &[[x, y], [x2, y2], [x3, y3], [x4, y4], [x, y]],
                &diamond_options,
            )]
        }
        Arrowhead::CardinalityOne => arrowhead_cardinality_one(
            generator,
            points(arrowhead, 0.0)?,
            &arrowhead_line_options(element, options),
        ),
        Arrowhead::CardinalityMany => arrowhead_lines_to_tip(
            generator,
            points(arrowhead, 0.0)?,
            &arrowhead_line_options(element, options),
        ),
        Arrowhead::CardinalityOneOrMany => {
            let line_options = arrowhead_line_options(element, options);
            let mut shapes = arrowhead_lines_to_tip(
                generator,
                points(Arrowhead::CardinalityMany, 0.0)?,
                &line_options,
            );
            shapes.extend(arrowhead_cardinality_one(
                generator,
                points(Arrowhead::CardinalityOne, CARDINALITY_ONE_OR_MANY_OFFSET)?,
                &line_options,
            ));
            shapes
        }
        Arrowhead::CardinalityExactlyOne => {
            let line_options = arrowhead_line_options(element, options);
            let mut shapes = arrowhead_cardinality_one(
                generator,
                points(Arrowhead::CardinalityOne, -0.5)?,
                &line_options,
            );
            shapes.extend(arrowhead_cardinality_one(
                generator,
                points(Arrowhead::CardinalityOne, 0.0)?,
                &line_options,
            ));
            shapes
        }
        Arrowhead::CardinalityZeroOrOne => {
            let line_options = arrowhead_line_options(element, options);
            let mut shapes = arrowhead_outline_circle(
                generator,
                options,
                &stroke_color,
                points(Arrowhead::CircleOutline, 1.5)?,
                &background_fill_color,
                CARDINALITY_ZERO_CIRCLE_SCALE,
            );
            shapes.extend(arrowhead_cardinality_one(
                generator,
                points(Arrowhead::CardinalityOne, -0.5)?,
                &line_options,
            ));
            shapes
        }
        Arrowhead::CardinalityZeroOrMany => {
            let line_options = arrowhead_line_options(element, options);
            let mut shapes = arrowhead_lines_to_tip(
                generator,
                points(Arrowhead::CardinalityMany, 0.0)?,
                &line_options,
            );
            shapes.extend(arrowhead_outline_circle(
                generator,
                options,
                &stroke_color,
                points(Arrowhead::CircleOutline, 1.5)?,
                &background_fill_color,
                CARDINALITY_ZERO_CIRCLE_SCALE,
            ));
            shapes
        }
        Arrowhead::Bar | Arrowhead::Arrow => arrowhead_lines_to_tip(
            generator,
            points(arrowhead, 0.0)?,
            &arrowhead_line_options(element, options),
        ),
    })
}
