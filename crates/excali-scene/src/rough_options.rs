//! The rough.js options Excalidraw derives from an element:
//! `generateRoughOptions` and `adjustRoughness`
//! (`packages/element/src/shape.ts:166-260`).
//!
//! See the option table in `site/content/research/rendering.md` section 1.
//! [`RoughOptions`] is the object `generateRoughOptions` returns, a partial
//! rough.js `Options` that the shape builders merge over the generator's
//! defaults (`Object.assign`, rough.js `_o`).

use std::fmt;

use excali_core::color::{apply_dark_mode_filter, is_transparent};
use excali_core::constants::{COLOR_TRANSPARENT, ROUGHNESS};
use excali_core::element::{Element, ElementKind, ElementType, FillStyle, StrokeStyle};
use excali_math::js;

use crate::utils::is_path_a_loop;

/// The options `generateRoughOptions` returns, field for field.
///
/// Every element gets the nine fields from `seed` to `preserve_vertices`
/// (`strokeLineDash` is set to `undefined` for solid strokes, which rough.js
/// reads as its default). `fill_style` is set, with `fill`, for rectangles,
/// iframes, embeddables, diamonds and ellipses, and for lines and freedraw
/// that close into a loop; `fill` is `None` (`undefined`, no fill) when the
/// background is transparent. `curve_fitting` is set for ellipses only.
/// [`RoughOptions::keys`] lists the keys upstream's object has.
#[derive(Clone, Debug, PartialEq)]
pub struct RoughOptions {
    /// `seed`: the element's seed as stored; rough.js reads it as a 32-bit
    /// integer (`Math.imul`).
    pub seed: f64,
    /// `strokeLineDash`: [`dash_array_dashed`] or [`dash_array_dotted`],
    /// `None` for solid strokes.
    pub stroke_line_dash: Option<[f64; 2]>,
    /// `disableMultiStroke`: set for non-solid strokes, whose dashes would
    /// otherwise overlay each other.
    pub disable_multi_stroke: bool,
    /// `strokeWidth`: half a pixel wider for non-solid strokes.
    pub stroke_width: f64,
    /// `fillWeight`: half the element's stroke width.
    pub fill_weight: f64,
    /// `hachureGap`: four times the element's stroke width.
    pub hachure_gap: f64,
    /// `roughness`: [`adjust_roughness`].
    pub roughness: f64,
    /// `stroke`: the stroke colour, through the dark mode filter when
    /// drawing dark.
    pub stroke: String,
    /// `preserveVertices`: for continuous paths and roughness below
    /// cartoonist.
    pub preserve_vertices: bool,
    /// `fillStyle`, when the element fills.
    pub fill_style: Option<FillStyle>,
    /// `fill`: the background colour (dark-filtered when drawing dark);
    /// `None` when the element does not fill or its background is
    /// transparent.
    pub fill: Option<String>,
    /// `curveFitting`: 1 for ellipses.
    pub curve_fitting: Option<f64>,
}

impl RoughOptions {
    /// The own keys of upstream's options object, in insertion order
    /// (`Object.keys`), including keys whose value is `undefined`.
    pub fn keys(&self) -> Vec<&'static str> {
        let mut keys = vec![
            "seed",
            "strokeLineDash",
            "disableMultiStroke",
            "strokeWidth",
            "fillWeight",
            "hachureGap",
            "roughness",
            "stroke",
            "preserveVertices",
        ];
        if self.fill_style.is_some() {
            keys.extend(["fillStyle", "fill"]);
        }
        if self.curve_fitting.is_some() {
            keys.push("curveFitting");
        }
        keys
    }
}

/// `generateRoughOptions` throws `Unimplemented type <type>` for element
/// types it has no options for (`shape.ts:256-258`): selection, sticky
/// note, image, frame, magic frame and text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnimplementedType(pub ElementType);

impl fmt::Display for UnimplementedType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unimplemented type {}", self.0)
    }
}

impl std::error::Error for UnimplementedType {}

/// `getDashArrayDashed(strokeWidth)` (`shape.ts:168`).
pub fn dash_array_dashed(stroke_width: f64) -> [f64; 2] {
    [8.0, 8.0 + stroke_width]
}

/// `getDashArrayDotted(strokeWidth)` (`shape.ts:170`).
pub fn dash_array_dotted(stroke_width: f64) -> [f64; 2] {
    [1.5, 6.0 + stroke_width]
}

/// `adjustRoughness(element)` (`shape.ts:172-193`): the element's
/// roughness, unchanged when both sides are relatively big (min ≥ 20 and
/// max ≥ 50), when it is rounded, can change roundness and both sides are
/// at least 15, or when it is a line or arrow at least 50 long; otherwise
/// `min(roughness / (max < 10 ? 3 : 2), 2.5)`.
pub fn adjust_roughness(element: &Element) -> f64 {
    let base = &element.base;
    let roughness = base.roughness;
    let max_size = js::max(base.width, base.height);
    let min_size = js::min(base.width, base.height);
    let ty = element.element_type();

    // don't reduce roughness if
    if
    // both sides relatively big
    (min_size >= 20.0 && max_size >= 50.0)
        // is round & both sides above 15px
        || (min_size >= 15.0 && base.roundness.is_some() && ty.can_change_roundness())
        // relatively long linear element
        || (ty.is_linear() && max_size >= 50.0)
    {
        return roughness;
    }

    js::min(roughness / if max_size < 10.0 { 3.0 } else { 2.0 }, 2.5)
}

/// `generateRoughOptions(element, continuousPath, isDarkMode)`
/// (`shape.ts:195-260`).
pub fn generate_rough_options(
    element: &Element,
    continuous_path: bool,
    is_dark_mode: bool,
) -> Result<RoughOptions, UnimplementedType> {
    let base = &element.base;
    let stroke_width = base.stroke_width;
    let solid = base.stroke_style == StrokeStyle::Solid;
    let mut options = RoughOptions {
        seed: base.seed,
        stroke_line_dash: match base.stroke_style {
            StrokeStyle::Dashed => Some(dash_array_dashed(stroke_width)),
            StrokeStyle::Dotted => Some(dash_array_dotted(stroke_width)),
            StrokeStyle::Solid => None,
        },
        // for non-solid strokes, disable multiStroke because it tends to
        // make dashes/dots overlay each other
        disable_multi_stroke: !solid,
        // for non-solid strokes, increase the width a bit to make it
        // visually similar to solid strokes, because we're also disabling
        // multiStroke
        stroke_width: if solid {
            stroke_width
        } else {
            stroke_width + 0.5
        },
        // when increasing strokeWidth, we must explicitly set fillWeight and
        // hachureGap because if not specified, roughjs uses strokeWidth to
        // calculate them (and we don't want the fills to be modified)
        fill_weight: stroke_width / 2.0,
        hachure_gap: stroke_width * 4.0,
        roughness: adjust_roughness(element),
        stroke: apply_dark_mode_filter(&base.stroke_color, is_dark_mode),
        preserve_vertices: continuous_path || base.roughness < ROUGHNESS.cartoonist,
        fill_style: None,
        fill: None,
        curve_fitting: None,
    };

    match &element.kind {
        ElementKind::Rectangle
        | ElementKind::Iframe
        | ElementKind::Embeddable
        | ElementKind::Diamond
        | ElementKind::Ellipse => {
            options.fill_style = Some(base.fill_style);
            options.fill = if is_transparent(&base.background_color) {
                None
            } else {
                Some(apply_dark_mode_filter(&base.background_color, is_dark_mode))
            };
            if matches!(element.kind, ElementKind::Ellipse) {
                options.curve_fitting = Some(1.0);
            }
            Ok(options)
        }
        ElementKind::Line(_) | ElementKind::Freedraw(_) => {
            let points = element.kind.points().unwrap_or_default();
            if is_path_a_loop(points, 1.0) {
                options.fill_style = Some(base.fill_style);
                // Upstream compares with the string "transparent" here, not
                // isTransparent.
                options.fill = if base.background_color == COLOR_TRANSPARENT {
                    None
                } else {
                    Some(apply_dark_mode_filter(&base.background_color, is_dark_mode))
                };
            }
            Ok(options)
        }
        ElementKind::Arrow(_) => Ok(options),
        ElementKind::Selection
        | ElementKind::StickyNote(_)
        | ElementKind::Image(_)
        | ElementKind::Frame(_)
        | ElementKind::MagicFrame(_)
        | ElementKind::Text(_) => Err(UnimplementedType(element.element_type())),
    }
}
