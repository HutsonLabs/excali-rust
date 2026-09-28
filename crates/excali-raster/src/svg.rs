//! SVG images as display items, so the backend draws them with the same
//! scan conversion, stroker and dasher as everything else.
//!
//! Chrome draws an SVG image into a canvas as vector content
//! (`SVGImage::DrawForContainer`: the document's paint record under the
//! source-to-destination transform), which Skia rasterizes like any other
//! path. When the parsed document holds only what the display list can
//! say (paths with solid fills and strokes, groups that only transform),
//! [`display_items`] turns it into fills and strokes, each under its
//! absolute transform, in the document's user units after the `viewBox`
//! mapping. Anything else (gradients, patterns, group opacity, clip paths,
//! masks, filters, blend modes, embedded images, `shape-rendering:
//! crispEdges`, `stroke-linejoin: miter-clip`) answers `None`, and the
//! caller renders the document with resvg instead.

use excali_scene::display::{
    Color, Dash, DisplayItem, FillRule, Group, LineCap, LineJoin, Path, PathCommand, Rect, Rgba,
    Stroke, Transform,
};
use resvg::usvg;

/// The document's drawing as display items, or `None` when it uses
/// something they cannot express.
pub(crate) fn display_items(tree: &usvg::Tree) -> Option<Vec<DisplayItem>> {
    let mut out = Vec::new();
    group_items(tree.root(), &mut out)?;
    Some(out)
}

fn group_items(group: &usvg::Group, out: &mut Vec<DisplayItem>) -> Option<()> {
    let plain = group.opacity().get() == 1.0
        && group.clip_path().is_none()
        && group.mask().is_none()
        && group.filters().is_empty()
        && group.blend_mode() == usvg::BlendMode::Normal;
    if !plain {
        return None;
    }
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => group_items(g, out)?,
            usvg::Node::Path(p) => path_items(p, out)?,
            usvg::Node::Text(t) => group_items(t.flattened(), out)?,
            usvg::Node::Image(_) => return None,
        }
    }
    Some(())
}

fn path_items(p: &usvg::Path, out: &mut Vec<DisplayItem>) -> Option<()> {
    if !p.is_visible() {
        return Some(());
    }
    if p.rendering_mode() == usvg::ShapeRendering::CrispEdges {
        return None;
    }
    let path = display_path(p.data());
    let fill = match p.fill() {
        Some(fill) => {
            let color = color(fill.paint(), fill.opacity())?;
            Some(match plain_rect(p.data()) {
                // Blink fills an SVG <rect> with drawRect (its rect fast path).
                Some(rect) => DisplayItem::FillRect { rect, color },
                None => DisplayItem::Fill {
                    path: path.clone(),
                    color,
                    rule: match fill.rule() {
                        usvg::FillRule::NonZero => FillRule::NonZero,
                        usvg::FillRule::EvenOdd => FillRule::EvenOdd,
                    },
                },
            })
        }
        None => None,
    };
    let stroke = match p.stroke() {
        Some(s) => {
            let mut stroke =
                Stroke::new(color(s.paint(), s.opacity())?, f64::from(s.width().get()));
            stroke.cap = match s.linecap() {
                usvg::LineCap::Butt => LineCap::Butt,
                usvg::LineCap::Round => LineCap::Round,
                usvg::LineCap::Square => LineCap::Square,
            };
            stroke.join = match s.linejoin() {
                usvg::LineJoin::Miter => LineJoin::Miter,
                usvg::LineJoin::Round => LineJoin::Round,
                usvg::LineJoin::Bevel => LineJoin::Bevel,
                usvg::LineJoin::MiterClip => return None,
            };
            stroke.miter_limit = f64::from(s.miterlimit().get());
            if let Some(dashes) = s.dasharray() {
                let dashes: Vec<f64> = dashes.iter().map(|&d| f64::from(d)).collect();
                stroke.dash = Dash::new(&dashes, f64::from(s.dashoffset()));
            }
            Some(DisplayItem::Stroke { path, stroke })
        }
        None => None,
    };
    let items: Vec<DisplayItem> = match p.paint_order() {
        usvg::PaintOrder::FillAndStroke => [fill, stroke],
        usvg::PaintOrder::StrokeAndFill => [stroke, fill],
    }
    .into_iter()
    .flatten()
    .collect();
    if items.is_empty() {
        return Some(());
    }
    let t = p.abs_transform();
    out.push(DisplayItem::Group(Group {
        transform: Transform::new(
            f64::from(t.sx),
            f64::from(t.ky),
            f64::from(t.kx),
            f64::from(t.sy),
            f64::from(t.tx),
            f64::from(t.ty),
        ),
        opacity: 1.0,
        clip: None,
        items,
    }));
    Some(())
}

/// A solid paint with its opacity as a CSS colour; `None` for gradients
/// and patterns.
fn color(paint: &usvg::Paint, opacity: usvg::Opacity) -> Option<Color> {
    match paint {
        usvg::Paint::Color(c) => Some(Color::new(
            Rgba {
                r: c.red,
                g: c.green,
                b: c.blue,
                a: f64::from(opacity.get()),
            }
            .css(),
        )),
        _ => None,
    }
}

fn display_path(data: &tiny_skia::Path) -> Path {
    let mut path = Path::new();
    for segment in data.segments() {
        let p = |pt: tiny_skia::Point| (f64::from(pt.x), f64::from(pt.y));
        match segment {
            tiny_skia::PathSegment::MoveTo(a) => {
                let (x, y) = p(a);
                path.move_to(x, y);
            }
            tiny_skia::PathSegment::LineTo(a) => {
                let (x, y) = p(a);
                path.line_to(x, y);
            }
            tiny_skia::PathSegment::QuadTo(c, a) => {
                let ((cx, cy), (x, y)) = (p(c), p(a));
                path.quad_to(cx, cy, x, y);
            }
            tiny_skia::PathSegment::CubicTo(c1, c2, a) => {
                let ((ax, ay), (bx, by), (x, y)) = (p(c1), p(c2), p(a));
                let start = path_end(&path);
                match start.and_then(|s| quarter_circle(s, (ax, ay), (bx, by), (x, y))) {
                    Some(arc) => {
                        path.arc(
                            arc.cx,
                            arc.cy,
                            arc.radius,
                            arc.start,
                            arc.end,
                            arc.anticlockwise,
                        );
                    }
                    None => {
                        path.cubic_to(ax, ay, bx, by, x, y);
                    }
                }
            }
            tiny_skia::PathSegment::Close => {
                path.close();
            }
        }
    }
    path
}

/// The rectangle a path is, when it is exactly what usvg builds for a
/// `<rect>` without rounded corners (`PathBuilder::from_rect`: the top-left
/// corner, clockwise, closed). Blink draws such an element with its rect
/// fast path (`drawRect`); a `<path>` tracing the same rectangle, which
/// Blink fills as a path, comes out the same and is drawn as a rectangle
/// too.
fn plain_rect(data: &tiny_skia::Path) -> Option<Rect> {
    let segments: Vec<tiny_skia::PathSegment> = data.segments().collect();
    let [tiny_skia::PathSegment::MoveTo(a), tiny_skia::PathSegment::LineTo(b), tiny_skia::PathSegment::LineTo(c), tiny_skia::PathSegment::LineTo(d), tiny_skia::PathSegment::Close] =
        segments.as_slice()
    else {
        return None;
    };
    let square = a.y == b.y && b.x == c.x && c.y == d.y && d.x == a.x;
    (square && a.x < b.x && b.y < c.y).then(|| {
        Rect::new(
            f64::from(a.x),
            f64::from(a.y),
            f64::from(b.x) - f64::from(a.x),
            f64::from(c.y) - f64::from(b.y),
        )
    })
}

/// The point the path's last command ends at, when it is a point command.
fn path_end(path: &Path) -> Option<(f64, f64)> {
    match *path.commands.last()? {
        PathCommand::MoveTo(x, y) | PathCommand::LineTo(x, y) => Some((x, y)),
        PathCommand::QuadTo(_, _, x, y) | PathCommand::CubicTo(_, _, _, _, x, y) => Some((x, y)),
        PathCommand::Arc {
            cx,
            cy,
            radius,
            end,
            ..
        } => Some((cx + radius * end.cos(), cy + radius * end.sin())),
        PathCommand::Close => None,
    }
}

/// A circular arc as `arc()` takes it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CircularArc {
    cx: f64,
    cy: f64,
    radius: f64,
    start: f64,
    end: f64,
    anticlockwise: bool,
}

/// `4/3 tan(π/8)`: the handle length, in radii, of the cubic that stands
/// for a quarter circle.
const KAPPA: f64 = 0.552_284_749_830_793_4;

/// The quarter circle the cubic from `p0` stands for, if it is exactly the
/// cubic usvg builds for one.
///
/// usvg turns circles and the corners of rounded rectangles into
/// axis-aligned quarter-circle cubics (kurbo's `Arc::to_cubic_beziers`),
/// where Chrome keeps them as Skia's conics (`addOval`, `addRRect`); drawn
/// with `arc()` they are those conics again, so their edge pixels agree
/// with Chrome's. Only a cubic that matches that construction to the last
/// bits of its `f32` points is taken: cubics written in the SVG (Font
/// Awesome's rounded corners in the placeholders, `c-26.51 0-48-21.49-48-48`)
/// round the handle differently and stay cubics, as Chrome draws them.
fn quarter_circle(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
) -> Option<CircularArc> {
    let magnitude = [p0, p1, p2, p3]
        .iter()
        .flat_map(|p| [p.0.abs(), p.1.abs()])
        .fold(1.0, f64::max);
    // One f32 ulp at the points' magnitude: kurbo's points are f64 values
    // rounded once, so its handles land within it (Font Awesome's are
    // 2.6 ulps off; the rare real arc a rounding pushes past stays a
    // cubic).
    let tolerance = f64::from(f32::EPSILON) * 2f64.powi(magnitude.log2().floor() as i32);
    let near = |a: f64, b: f64| (a - b).abs() <= tolerance;
    // Leaving p0 vertically (p1 straight above or below it) and arriving
    // at p3 horizontally, or the other way round.
    let (centre, radius) = if p1.0 == p0.0 && p2.1 == p3.1 {
        let c = (p3.0, p0.1);
        let r = (p0.0 - c.0).abs();
        let ok = near(r, (p3.1 - c.1).abs())
            && near((p1.1 - p0.1).abs(), KAPPA * r)
            && near((p2.0 - p3.0).abs(), KAPPA * r)
            && (p1.1 - p0.1).signum() == (p3.1 - p0.1).signum()
            && (p2.0 - p3.0).signum() == (p0.0 - p3.0).signum();
        (c, ok.then_some(r)?)
    } else if p1.1 == p0.1 && p2.0 == p3.0 {
        let c = (p0.0, p3.1);
        let r = (p0.1 - c.1).abs();
        let ok = near(r, (p3.0 - c.0).abs())
            && near((p1.0 - p0.0).abs(), KAPPA * r)
            && near((p2.1 - p3.1).abs(), KAPPA * r)
            && (p1.0 - p0.0).signum() == (p3.0 - p0.0).signum()
            && (p2.1 - p3.1).signum() == (p0.1 - p3.1).signum();
        (c, ok.then_some(r)?)
    } else {
        return None;
    };
    if radius == 0.0 {
        return None;
    }
    let start = (p0.1 - centre.1).atan2(p0.0 - centre.0);
    let end = (p3.1 - centre.1).atan2(p3.0 - centre.0);
    let mut sweep = end - start;
    if sweep > std::f64::consts::PI {
        sweep -= std::f64::consts::TAU;
    } else if sweep < -std::f64::consts::PI {
        sweep += std::f64::consts::TAU;
    }
    Some(CircularArc {
        cx: centre.0,
        cy: centre.1,
        radius,
        start,
        end: start + sweep,
        anticlockwise: sweep < 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_circles_are_arcs() {
        let k = KAPPA * 10.0;
        // From (15, 5) clockwise on screen to (5, 15), centre (5, 5).
        let arc =
            quarter_circle((15.0, 5.0), (15.0, 5.0 + k), (5.0 + k, 15.0), (5.0, 15.0)).unwrap();
        assert_eq!((arc.cx, arc.cy, arc.radius), (5.0, 5.0, 10.0));
        assert_eq!(arc.start, 0.0);
        assert!((arc.end - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!(!arc.anticlockwise);
        // The other way round.
        let back =
            quarter_circle((5.0, 15.0), (5.0 + k, 15.0), (15.0, 5.0 + k), (15.0, 5.0)).unwrap();
        assert!(back.anticlockwise);
        assert_eq!((back.cx, back.cy), (5.0, 5.0));
        // Font Awesome's corner (handle 26.51 for a radius of 48), a
        // different handle, an ellipse, a straight cubic: cubics.
        assert!(quarter_circle(
            (464.0, 448.0),
            (464.0 - 0.0, 448.0),
            (512.0, 448.0 - 21.49),
            (512.0, 400.0)
        )
        .is_none());
        assert!(
            quarter_circle((48.0, 448.0), (21.49, 448.0), (0.0, 426.51), (0.0, 400.0)).is_none()
        );
        assert!(quarter_circle(
            (15.0, 5.0),
            (15.0, 5.0 + 2.0 * k),
            (5.0 + k, 15.0),
            (5.0, 15.0)
        )
        .is_none());
        assert!(quarter_circle(
            (25.0, 5.0),
            (25.0, 5.0 + k),
            (5.0 + 2.0 * k, 15.0),
            (5.0, 15.0)
        )
        .is_none());
        assert!(quarter_circle((0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0)).is_none());
    }

    fn tree(svg: &str) -> usvg::Tree {
        usvg::Tree::from_str(svg, &crate::decode::svg_options()).unwrap()
    }

    fn kinds(items: &[DisplayItem]) -> Vec<String> {
        items
            .iter()
            .flat_map(|i| match i {
                DisplayItem::Group(g) => kinds(&g.items),
                DisplayItem::Fill { color, .. } => vec![format!("fill {}", color.as_str())],
                DisplayItem::FillRect { color, .. } => {
                    vec![format!("fillRect {}", color.as_str())]
                }
                DisplayItem::Stroke { stroke, .. } => {
                    vec![format!("stroke {} {}", stroke.color.as_str(), stroke.width)]
                }
                other => vec![format!("{other:?}")],
            })
            .collect()
    }

    #[test]
    fn document_order_and_paints() {
        let t = tree(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><rect width="4" height="4" fill="#1971c2"/><circle cx="20" cy="10" r="5" fill="#e03131" fill-opacity="0.8"/><path d="M0 0 L10 10" fill="none" stroke="#2f9e44" stroke-width="2.5"/></svg>"##,
        );
        let items = display_items(&t).unwrap();
        assert_eq!(
            kinds(&items),
            [
                "fillRect rgba(25, 113, 194, 1)",
                "fill rgba(224, 49, 49, 0.800000011920929)",
                "stroke rgba(47, 158, 68, 1) 2.5",
            ]
        );
    }

    #[test]
    fn the_fixture_svg_and_the_placeholders_convert() {
        let shape = include_str!("../tests/fixtures/images/shape.svg");
        assert_eq!(
            display_items(&tree(shape)).map(|i| kinds(&i).len()),
            Some(3)
        );
        for builtin in excali_scene::display::BuiltinImage::ALL {
            let items = display_items(&tree(builtin.svg())).unwrap();
            assert!(kinds(&items)
                .iter()
                .all(|k| k == "fill rgba(136, 136, 136, 1)"));
        }
    }

    #[test]
    fn what_the_list_cannot_say_falls_back() {
        for svg in [
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><defs><linearGradient id="g"><stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs><rect width="4" height="4" fill="url(#g)"/></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><g opacity="0.5"><rect width="4" height="4"/><rect width="2" height="2"/></g></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" shape-rendering="crispEdges"/></svg>"##,
        ] {
            assert!(display_items(&tree(svg)).is_none(), "{svg}");
        }
    }
}
