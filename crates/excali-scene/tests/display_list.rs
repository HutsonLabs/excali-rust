//! The display list (ex-216): the renderer-independent drawing vocabulary
//! the backends (`excali-raster`, `excali-canvas2d`, `excali-svg`) consume.
//!
//! Upstream draws straight onto a `CanvasRenderingContext2D`
//! (`packages/element/src/renderElement.ts`,
//! `packages/excalidraw/renderer/staticScene.ts`), so the display list keeps
//! the canvas's semantics: matrices compose as `ctx.transform`, opacity is
//! `globalAlpha` applied to every draw, clips intersect as `ctx.clip`, and
//! the value rules of `setLineDash`, `lineWidth`, `arc` and `roundRect` are
//! the HTML canvas specification's. These tests pin those rules; the
//! backends are then checked against the same list.

use std::f64::consts::{FRAC_PI_2, PI};

use excali_scene::display::{
    Clip, Color, Dash, Direction, DisplayItem, DisplayList, FillRule, Font, Group, ImageFilter,
    ImageItem, LineCap, LineJoin, PaintState, Painter, Path, PathCommand, Rect, Rgba, Stroke,
    TextAlign, TextRun, Transform,
};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn assert_point(actual: (f64, f64), expected: (f64, f64)) {
    assert!(
        close(actual.0, expected.0) && close(actual.1, expected.1),
        "{actual:?} != {expected:?}"
    );
}

mod transform {
    use super::*;

    #[test]
    fn identity_is_the_canvas_default() {
        // A fresh context's matrix is [1 0 0 1 0 0] (HTML canvas, "the
        // current transformation matrix ... initially the identity").
        assert_eq!(
            Transform::IDENTITY,
            Transform::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
        );
        assert!(Transform::IDENTITY.is_identity());
        assert!(!Transform::translate(1.0, 0.0).is_identity());
        assert_eq!(Transform::default(), Transform::IDENTITY);
    }

    #[test]
    fn named_constructors_are_the_canvas_methods() {
        // translate(x, y) = [1 0 0 1 x y]; scale(x, y) = [x 0 0 y 0 0];
        // rotate(a) = [cos a, sin a, -sin a, cos a, 0, 0].
        assert_eq!(
            Transform::translate(3.0, 4.0),
            Transform::new(1.0, 0.0, 0.0, 1.0, 3.0, 4.0)
        );
        assert_eq!(
            Transform::scale(2.0, -1.0),
            Transform::new(2.0, 0.0, 0.0, -1.0, 0.0, 0.0)
        );
        let r = Transform::rotate(FRAC_PI_2);
        assert!(close(r.a, 0.0) && close(r.b, 1.0) && close(r.c, -1.0) && close(r.d, 0.0));
        assert_eq!((r.e, r.f), (0.0, 0.0));
    }

    #[test]
    fn apply_maps_a_point() {
        let t = Transform::new(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
        // x' = a x + c y + e, y' = b x + d y + f
        assert_eq!(t.apply(1.0, 1.0), (9.0, 12.0));
        assert_point(Transform::rotate(FRAC_PI_2).apply(1.0, 0.0), (0.0, 1.0));
    }

    #[test]
    fn concat_is_ctx_transform() {
        // renderElement.ts:1126-1132 (export path): translate(cx, cy),
        // rotate(angle), translate(-w/2, -h/2). The matrix after the three
        // calls maps the element's local origin to the rotated top-left.
        let (cx, cy, w, h) = (60.0, 45.0, 100.0, 50.0);
        let m = Transform::translate(cx, cy)
            .concat(&Transform::rotate(FRAC_PI_2))
            .concat(&Transform::translate(-w / 2.0, -h / 2.0));
        // Local (0, 0) is (-50, -25) around the centre; rotated a quarter
        // turn clockwise (canvas y points down) that is (25, -50).
        assert_point(m.apply(0.0, 0.0), (cx + 25.0, cy - 50.0));
        assert_point(m.apply(w / 2.0, h / 2.0), (cx, cy));
        // Order matters: scale then translate differs from translate then scale.
        let st = Transform::scale(2.0, 2.0).concat(&Transform::translate(1.0, 1.0));
        let ts = Transform::translate(1.0, 1.0).concat(&Transform::scale(2.0, 2.0));
        assert_eq!(st.apply(0.0, 0.0), (2.0, 2.0));
        assert_eq!(ts.apply(0.0, 0.0), (1.0, 1.0));
    }
}

mod path {
    use super::*;
    use PathCommand::*;

    #[test]
    fn builder_records_commands_in_order() {
        let mut p = Path::new();
        p.move_to(0.0, 0.0)
            .line_to(10.0, 0.0)
            .quad_to(15.0, 5.0, 10.0, 10.0)
            .cubic_to(8.0, 12.0, 2.0, 12.0, 0.0, 10.0)
            .arc(5.0, 5.0, 5.0, 0.0, PI, false)
            .close();
        assert_eq!(
            p.commands,
            vec![
                MoveTo(0.0, 0.0),
                LineTo(10.0, 0.0),
                QuadTo(15.0, 5.0, 10.0, 10.0),
                CubicTo(8.0, 12.0, 2.0, 12.0, 0.0, 10.0),
                Arc {
                    cx: 5.0,
                    cy: 5.0,
                    radius: 5.0,
                    start: 0.0,
                    end: PI,
                    anticlockwise: false,
                },
                Close,
            ]
        );
        assert!(!p.is_empty());
        assert!(Path::new().is_empty());
    }

    #[test]
    fn rect_is_the_canvas_rect() {
        // HTML canvas rect(x, y, w, h): a closed subpath of the four corners,
        // then a new subpath at (x, y).
        let p = Path::rect(1.0, 2.0, 3.0, 4.0);
        assert_eq!(
            p.commands,
            vec![
                MoveTo(1.0, 2.0),
                LineTo(4.0, 2.0),
                LineTo(4.0, 6.0),
                LineTo(1.0, 6.0),
                Close,
                MoveTo(1.0, 2.0),
            ]
        );
    }

    fn arc(cx: f64, cy: f64, radius: f64, start: f64, end: f64) -> PathCommand {
        Arc {
            cx,
            cy,
            radius,
            start,
            end,
            anticlockwise: false,
        }
    }

    #[test]
    fn round_rect_is_the_canvas_round_rect() {
        // renderElement.ts:537-546 clips rounded images with
        // roundRect(0, 0, w, h, r); staticScene.ts:165-189 clips frames the
        // same way. HTML canvas roundRect: start after the top-left corner,
        // clockwise, a quarter ellipse per corner, close, new subpath at (x, y).
        let p = Path::round_rect(0.0, 0.0, 100.0, 50.0, 10.0);
        assert_eq!(
            p.commands,
            vec![
                MoveTo(10.0, 0.0),
                LineTo(90.0, 0.0),
                arc(90.0, 10.0, 10.0, -FRAC_PI_2, 0.0),
                LineTo(100.0, 40.0),
                arc(90.0, 40.0, 10.0, 0.0, FRAC_PI_2),
                LineTo(10.0, 50.0),
                arc(10.0, 40.0, 10.0, FRAC_PI_2, PI),
                LineTo(0.0, 10.0),
                arc(10.0, 10.0, 10.0, PI, PI + FRAC_PI_2),
                Close,
                MoveTo(0.0, 0.0),
            ]
        );
    }

    #[test]
    fn round_rect_scales_radii_that_do_not_fit() {
        // "If scale is less than 1, then set each radius to radius × scale",
        // scale = min(w / (2r), h / (2r)).
        let p = Path::round_rect(0.0, 0.0, 100.0, 10.0, 20.0);
        assert_eq!(p.commands[0], MoveTo(5.0, 0.0));
        assert_eq!(p.commands[2], arc(95.0, 5.0, 5.0, -FRAC_PI_2, 0.0));
    }

    #[test]
    fn round_rect_normalises_negative_sizes() {
        // "If w is negative, set x to x + w and w to -w" (and likewise h).
        let flipped = Path::round_rect(100.0, 50.0, -100.0, -50.0, 10.0);
        assert_eq!(flipped, Path::round_rect(0.0, 0.0, 100.0, 50.0, 10.0));
    }

    #[test]
    fn round_rect_ignores_bad_arguments() {
        // Non-finite arguments return without drawing; a negative radius
        // throws a RangeError in the browser, which adds nothing either.
        assert!(Path::round_rect(f64::NAN, 0.0, 1.0, 1.0, 0.0).is_empty());
        assert!(Path::round_rect(0.0, 0.0, f64::INFINITY, 1.0, 0.0).is_empty());
        assert!(Path::round_rect(0.0, 0.0, 1.0, 1.0, -1.0).is_empty());
        assert!(Path::round_rect(0.0, 0.0, 1.0, 1.0, f64::NAN).is_empty());
    }

    #[test]
    fn canonical_starts_every_subpath() {
        // lineTo with no subpath "ensure[s] there is a subpath for (x, y)",
        // i.e. it is a moveTo; quadratic and bezier curves ensure a subpath
        // for their first control point.
        let mut p = Path::new();
        p.line_to(1.0, 2.0).line_to(3.0, 4.0);
        assert_eq!(
            p.canonical().commands,
            vec![MoveTo(1.0, 2.0), LineTo(3.0, 4.0)]
        );
        let mut p = Path::new();
        p.quad_to(1.0, 1.0, 2.0, 0.0);
        assert_eq!(
            p.canonical().commands,
            vec![MoveTo(1.0, 1.0), QuadTo(1.0, 1.0, 2.0, 0.0)]
        );
        let mut p = Path::new();
        p.cubic_to(1.0, 1.0, 2.0, 2.0, 3.0, 0.0);
        assert_eq!(
            p.canonical().commands,
            vec![MoveTo(1.0, 1.0), CubicTo(1.0, 1.0, 2.0, 2.0, 3.0, 0.0)]
        );
    }

    #[test]
    fn canonical_skips_non_finite_commands() {
        // Every path method returns early when an argument is infinite or NaN.
        let mut p = Path::new();
        p.move_to(0.0, 0.0)
            .line_to(f64::NAN, 1.0)
            .line_to(1.0, 1.0)
            .quad_to(f64::INFINITY, 0.0, 1.0, 1.0)
            .cubic_to(0.0, 0.0, 0.0, f64::NEG_INFINITY, 1.0, 1.0)
            .arc(0.0, 0.0, f64::NAN, 0.0, 1.0, false)
            .move_to(f64::NAN, 0.0);
        assert_eq!(
            p.canonical().commands,
            vec![MoveTo(0.0, 0.0), LineTo(1.0, 1.0)]
        );
    }

    #[test]
    fn canonical_drops_negative_radius_arcs() {
        // arc() with a negative radius throws IndexSizeError: nothing is added.
        let mut p = Path::new();
        p.move_to(0.0, 0.0).arc(0.0, 0.0, -1.0, 0.0, 1.0, false);
        assert_eq!(p.canonical().commands, vec![MoveTo(0.0, 0.0)]);
    }

    fn cubic_ends(commands: &[PathCommand]) -> Vec<(f64, f64)> {
        commands
            .iter()
            .filter_map(|c| match *c {
                CubicTo(_, _, _, _, x, y) => Some((x, y)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn canonical_turns_arcs_into_cubics() {
        // No subpath: the arc starts one. A quarter circle is one cubic
        // with the standard 4/3 tan(θ/4) handle length.
        let mut p = Path::new();
        p.arc(0.0, 0.0, 10.0, 0.0, FRAC_PI_2, false);
        let c = p.canonical().commands;
        assert_eq!(c.len(), 2);
        assert_eq!(c[0], MoveTo(10.0, 0.0));
        let CubicTo(x1, y1, x2, y2, x, y) = c[1] else {
            panic!("expected a cubic, got {:?}", c[1]);
        };
        let k = 10.0 * 4.0 / 3.0 * (FRAC_PI_2 / 4.0).tan();
        assert_point((x1, y1), (10.0, k));
        assert_point((x2, y2), (k, 10.0));
        assert_point((x, y), (0.0, 10.0));
    }

    #[test]
    fn canonical_arc_after_a_point_draws_a_line_to_its_start() {
        // "If the path has any subpaths, add a straight line from the last
        // point in the subpath to the start point of the arc."
        let mut p = Path::new();
        p.move_to(0.0, 0.0).arc(20.0, 0.0, 5.0, 0.0, PI, false);
        let c = p.canonical().commands;
        assert_eq!(c[0], MoveTo(0.0, 0.0));
        assert_eq!(c[1], LineTo(25.0, 0.0));
        let ends = cubic_ends(&c);
        assert_eq!(ends.len(), 2, "half circle = two quarter cubics");
        assert_point(ends[0], (20.0, 5.0));
        assert_point(ends[1], (15.0, 0.0));
    }

    #[test]
    fn canonical_arc_direction_and_full_turns() {
        // Clockwise from 0 to π/2 passes through the bottom (y down);
        // anticlockwise from 0 to π/2 goes the long way, through the top.
        let mut cw = Path::new();
        cw.arc(0.0, 0.0, 1.0, 0.0, FRAC_PI_2, false);
        assert_eq!(cubic_ends(&cw.canonical().commands).len(), 1);
        let mut ccw = Path::new();
        ccw.arc(0.0, 0.0, 1.0, 0.0, FRAC_PI_2, true);
        let ends = cubic_ends(&ccw.canonical().commands);
        assert_eq!(ends.len(), 3, "three quarters anticlockwise");
        assert_point(ends[0], (0.0, -1.0));
        assert_point(*ends.last().unwrap(), (0.0, 1.0));
        // end - start >= 2π clockwise is the whole circle, not the remainder.
        let mut full = Path::new();
        full.arc(0.0, 0.0, 1.0, 0.0, 5.0 * PI, false);
        let ends = cubic_ends(&full.canonical().commands);
        assert_eq!(ends.len(), 4);
        assert_point(*ends.last().unwrap(), (1.0, 0.0));
        // A clockwise sweep with end < start wraps to the positive remainder.
        let mut wrap = Path::new();
        wrap.arc(0.0, 0.0, 1.0, FRAC_PI_2, 0.0, false);
        assert_eq!(cubic_ends(&wrap.canonical().commands).len(), 3);
    }

    #[test]
    fn canonical_zero_radius_arc_is_a_point() {
        let mut p = Path::new();
        p.move_to(0.0, 0.0).arc(3.0, 4.0, 0.0, 0.0, PI, false);
        assert_eq!(
            p.canonical().commands,
            vec![MoveTo(0.0, 0.0), LineTo(3.0, 4.0)]
        );
    }

    #[test]
    fn canonical_after_close_continues_from_the_subpath_start() {
        // closePath starts a new subpath at the previous subpath's first point.
        let mut p = Path::new();
        p.move_to(1.0, 1.0)
            .line_to(5.0, 1.0)
            .close()
            .line_to(9.0, 9.0);
        assert_eq!(
            p.canonical().commands,
            vec![
                MoveTo(1.0, 1.0),
                LineTo(5.0, 1.0),
                Close,
                MoveTo(1.0, 1.0),
                LineTo(9.0, 9.0),
            ]
        );
    }
}

mod paint {
    use super::*;

    #[test]
    fn css_names() {
        assert_eq!(FillRule::NonZero.as_css(), "nonzero");
        assert_eq!(FillRule::EvenOdd.as_css(), "evenodd");
        assert_eq!(FillRule::default(), FillRule::NonZero);
        assert_eq!(LineCap::Butt.as_css(), "butt");
        assert_eq!(LineCap::Round.as_css(), "round");
        assert_eq!(LineCap::Square.as_css(), "square");
        assert_eq!(LineJoin::Miter.as_css(), "miter");
        assert_eq!(LineJoin::Round.as_css(), "round");
        assert_eq!(LineJoin::Bevel.as_css(), "bevel");
        assert_eq!(TextAlign::Left.as_css(), "left");
        assert_eq!(TextAlign::Center.as_css(), "center");
        assert_eq!(TextAlign::Right.as_css(), "right");
        assert_eq!(Direction::Ltr.as_css(), "ltr");
        assert_eq!(Direction::Rtl.as_css(), "rtl");
    }

    #[test]
    fn colors_keep_their_source_text() {
        // The SVG writer writes the colour exactly as the element stores it
        // (export.test.ts.snap: stroke="#1e1e1e").
        let c = Color::new("#1e1e1e");
        assert_eq!(c.as_str(), "#1e1e1e");
        assert_eq!(Color::from("red").as_str(), "red");
    }

    #[test]
    fn colors_resolve_through_tinycolor() {
        assert_eq!(
            Color::new("#1e1e1e").rgba(),
            Some(Rgba {
                r: 30,
                g: 30,
                b: 30,
                a: 1.0
            })
        );
        assert_eq!(
            Color::new("rgba(0, 0, 0, 0.16)").rgba(),
            Some(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0.16
            })
        );
        assert_eq!(
            Color::new("#ff000080").rgba().map(|c| (c.r, c.g, c.b)),
            Some((255, 0, 0))
        );
        assert_eq!(
            Color::new("transparent").rgba(),
            Some(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0.0
            })
        );
        assert_eq!(
            Color::new("hsl(120, 100%, 25%)").rgba().map(|c| c.g),
            Some(128)
        );
        // Not colours: nothing is painted.
        assert_eq!(Color::new("").rgba(), None);
        assert_eq!(Color::new("none").rgba(), None);
        assert_eq!(Color::new("not a colour").rgba(), None);
    }

    #[test]
    fn rgba_css_is_what_canvas2d_receives() {
        assert_eq!(
            Rgba {
                r: 30,
                g: 30,
                b: 30,
                a: 1.0
            }
            .css(),
            "rgba(30, 30, 30, 1)"
        );
        assert_eq!(
            Rgba {
                r: 0,
                g: 0,
                b: 200,
                a: 0.04
            }
            .css(),
            "rgba(0, 0, 200, 0.04)"
        );
    }

    #[test]
    fn dash_follows_set_line_dash() {
        // setLineDash: an odd list is repeated to make it even.
        let d = Dash::new(&[8.0, 10.0], 0.0).unwrap();
        assert_eq!(d.segments(), &[8.0, 10.0]);
        assert_eq!(d.offset(), 0.0);
        let d = Dash::new(&[5.0], 2.0).unwrap();
        assert_eq!(d.segments(), &[5.0, 5.0]);
        assert_eq!(d.offset(), 2.0);
        let d = Dash::new(&[1.0, 2.0, 3.0], 0.0).unwrap();
        assert_eq!(d.segments(), &[1.0, 2.0, 3.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn dash_that_canvas_ignores_is_solid() {
        // Any negative or non-finite entry: setLineDash returns and the
        // line stays solid. An empty list is solid, and so is a list whose
        // entries sum to 0 (the dash pattern has no length).
        assert_eq!(Dash::new(&[], 0.0), None);
        assert_eq!(Dash::new(&[4.0, -1.0], 0.0), None);
        assert_eq!(Dash::new(&[4.0, f64::NAN], 0.0), None);
        assert_eq!(Dash::new(&[f64::INFINITY], 0.0), None);
        assert_eq!(Dash::new(&[0.0, 0.0], 0.0), None);
        // A non-finite lineDashOffset is ignored: the offset stays 0.
        assert_eq!(Dash::new(&[1.0, 1.0], f64::NAN).unwrap().offset(), 0.0);
        assert_eq!(Dash::new(&[1.0, 1.0], f64::INFINITY).unwrap().offset(), 0.0);
    }

    #[test]
    fn stroke_defaults_are_the_canvas_defaults() {
        let s = Stroke::new(Color::new("#000"), 2.0);
        assert_eq!(s.width, 2.0);
        assert_eq!(s.cap, LineCap::Butt);
        assert_eq!(s.join, LineJoin::Miter);
        assert_eq!(s.miter_limit, 10.0);
        assert_eq!(s.dash, None);
        let s = s
            .with_cap(LineCap::Round)
            .with_join(LineJoin::Round)
            .with_dash(Dash::new(&[8.0, 10.0], 0.0));
        assert_eq!(s.cap, LineCap::Round);
        assert_eq!(s.join, LineJoin::Round);
        assert_eq!(
            s.dash.as_ref().map(|d| d.segments().to_vec()),
            Some(vec![8.0, 10.0])
        );
    }

    #[test]
    fn stroke_width_canvas_ignores_is_one() {
        // lineWidth and miterLimit assignments of zero, negative, infinite
        // or NaN values are ignored, leaving the defaults 1 and 10.
        for bad in [0.0, -2.0, f64::NAN, f64::INFINITY] {
            let mut s = Stroke::new(Color::new("#000"), bad);
            s.miter_limit = bad;
            assert_eq!(s.effective_width(), 1.0, "width {bad}");
            assert_eq!(s.effective_miter_limit(), 10.0, "miter {bad}");
        }
        let mut s = Stroke::new(Color::new("#000"), 0.5);
        s.miter_limit = 4.0;
        assert_eq!(s.effective_width(), 0.5);
        assert_eq!(s.effective_miter_limit(), 4.0);
    }
}

mod text_and_images {
    use super::*;

    #[test]
    fn font_css_is_get_font_string() {
        // common/src/utils.ts:139-147: `${fontSize}px ${family string}`,
        // the size printed as JavaScript prints numbers.
        let f = Font::new(20.0, "Excalifont, Xiaolai, sans-serif, Segoe UI Emoji");
        assert_eq!(
            f.css(),
            "20px Excalifont, Xiaolai, sans-serif, Segoe UI Emoji"
        );
        assert_eq!(Font::new(16.5, "Nunito").css(), "16.5px Nunito");
        assert_eq!(Font::new(1e21, "Helvetica").css(), "1e+21px Helvetica");
        assert_eq!(
            Font::new(0.1 + 0.2, "Helvetica").css(),
            "0.30000000000000004px Helvetica"
        );
    }

    #[test]
    fn text_run_defaults() {
        let run = TextRun::new(
            "hi",
            1.0,
            2.0,
            Font::new(20.0, "Virgil"),
            Color::new("#1e1e1e"),
        );
        assert_eq!(run.text, "hi");
        assert_eq!((run.x, run.y), (1.0, 2.0));
        assert_eq!(run.align, TextAlign::Left);
        assert_eq!(run.direction, Direction::Ltr);
    }

    #[test]
    fn dark_theme_filter_is_upstreams_numeric_filter() {
        // DARK_THEME_FILTER (common/src/constants.ts:204) and the numeric
        // version applyDarkModeFilter uses (colors.ts:19-116).
        assert_eq!(
            ImageFilter::DarkTheme.css(),
            "invert(93%) hue-rotate(180deg)"
        );
        let (r, g, b) = ImageFilter::DarkTheme.apply_rgb(255, 255, 255);
        let expected = excali_core::color::apply_dark_mode_filter("#ffffff", true);
        assert_eq!(format!("#{r:02x}{g:02x}{b:02x}"), expected);
        let (r, g, b) = ImageFilter::DarkTheme.apply_rgb(0x1e, 0x1e, 0x1e);
        let expected = excali_core::color::apply_dark_mode_filter("#1e1e1e", true);
        assert_eq!(format!("#{r:02x}{g:02x}{b:02x}"), expected);
        let (r, g, b) = ImageFilter::DarkTheme.apply_rgb(0x19, 0x71, 0xc2);
        let expected = excali_core::color::apply_dark_mode_filter("#1971c2", true);
        assert_eq!(format!("#{r:02x}{g:02x}{b:02x}"), expected);
    }

    #[test]
    fn image_item_defaults() {
        let img = ImageItem::new("file-1", Rect::new(0.0, 0.0, 100.0, 50.0));
        assert_eq!(img.id, "file-1");
        assert_eq!(img.source, None);
        assert!(img.smoothing, "imageSmoothingEnabled defaults to true");
        assert_eq!(img.filter, None);
    }

    #[test]
    fn rect_normalises() {
        let r = Rect::new(10.0, 10.0, -4.0, -6.0).normalized();
        assert_eq!(r, Rect::new(6.0, 4.0, 4.0, 6.0));
        assert!(Rect::new(0.0, 0.0, 0.0, 1.0).is_empty());
        assert!(!Rect::new(0.0, 0.0, 1.0, 1.0).is_empty());
    }
}

// ---------------------------------------------------------------------------
// Replay: the one walk every backend shares.

#[derive(Debug, PartialEq)]
enum Call {
    Fill(Vec<PathCommand>, Rgba, FillRule, PaintState),
    Stroke(Vec<PathCommand>, f64, Rgba, PaintState),
    Image(String, PaintState),
    Text(String, Rgba, PaintState),
    PushClip(Vec<PathCommand>, FillRule, Transform),
    PopClip,
}

#[derive(Default)]
struct Recorder(Vec<Call>);

impl Painter for Recorder {
    fn fill(&mut self, path: &Path, color: Rgba, rule: FillRule, state: &PaintState) {
        self.0
            .push(Call::Fill(path.commands.clone(), color, rule, *state));
    }
    fn stroke(&mut self, path: &Path, stroke: &Stroke, color: Rgba, state: &PaintState) {
        self.0.push(Call::Stroke(
            path.commands.clone(),
            stroke.width,
            color,
            *state,
        ));
    }
    fn image(&mut self, image: &ImageItem, state: &PaintState) {
        self.0.push(Call::Image(image.id.clone(), *state));
    }
    fn text(&mut self, run: &TextRun, color: Rgba, state: &PaintState) {
        self.0.push(Call::Text(run.text.clone(), color, *state));
    }
    fn push_clip(&mut self, clip: &Clip, transform: &Transform) {
        self.0.push(Call::PushClip(
            clip.path.commands.clone(),
            clip.rule,
            *transform,
        ));
    }
    fn pop_clip(&mut self) {
        self.0.push(Call::PopClip);
    }
}

fn replay(list: &DisplayList) -> Vec<Call> {
    let mut r = Recorder::default();
    list.replay(&mut r);
    r.0
}

const BLACK: Rgba = Rgba {
    r: 0,
    g: 0,
    b: 0,
    a: 1.0,
};

fn state(transform: Transform, alpha: f64) -> PaintState {
    PaintState { transform, alpha }
}

mod replay {
    use super::*;

    #[test]
    fn items_replay_in_order_at_the_root_state() {
        let mut list = DisplayList::new();
        assert!(list.is_empty());
        list.push(DisplayItem::Fill {
            path: Path::rect(0.0, 0.0, 10.0, 10.0),
            color: Color::new("#000"),
            rule: FillRule::NonZero,
        });
        list.push(DisplayItem::Stroke {
            path: Path::rect(0.0, 0.0, 10.0, 10.0),
            stroke: Stroke::new(Color::new("#000000"), 2.0),
        });
        list.push(DisplayItem::Image(ImageItem::new(
            "img",
            Rect::new(0.0, 0.0, 1.0, 1.0),
        )));
        list.push(DisplayItem::Text(TextRun::new(
            "a",
            0.0,
            0.0,
            Font::new(20.0, "Virgil"),
            Color::new("black"),
        )));
        assert_eq!(list.len(), 4);
        let root = state(Transform::IDENTITY, 1.0);
        assert_eq!(PaintState::ROOT, root);
        let rect = Path::rect(0.0, 0.0, 10.0, 10.0).commands;
        assert_eq!(
            replay(&list),
            vec![
                Call::Fill(rect.clone(), BLACK, FillRule::NonZero, root),
                Call::Stroke(rect, 2.0, BLACK, root),
                Call::Image("img".into(), root),
                Call::Text("a".into(), BLACK, root),
            ]
        );
    }

    #[test]
    fn groups_compose_transform_and_multiply_opacity() {
        // renderElement.ts: every element is drawn in its own save/restore
        // with a translate/rotate and globalAlpha = opacity; the sticky-note
        // footer multiplies (globalAlpha *= STICKY_NOTE_FOOTER.opacity).
        let inner = Group {
            transform: Transform::rotate(FRAC_PI_2),
            opacity: 0.5,
            clip: None,
            items: vec![DisplayItem::Text(TextRun::new(
                "footer",
                0.0,
                0.0,
                Font::new(12.0, "Helvetica"),
                Color::new("#000"),
            ))],
        };
        let outer = Group {
            transform: Transform::translate(10.0, 20.0),
            opacity: 0.8,
            clip: None,
            items: vec![
                DisplayItem::Fill {
                    path: Path::rect(0.0, 0.0, 1.0, 1.0),
                    color: Color::new("#000"),
                    rule: FillRule::NonZero,
                },
                DisplayItem::Group(inner),
            ],
        };
        let mut list = DisplayList::new();
        list.push(DisplayItem::Group(outer));
        list.push(DisplayItem::Fill {
            path: Path::rect(0.0, 0.0, 1.0, 1.0),
            color: Color::new("#000"),
            rule: FillRule::EvenOdd,
        });
        let calls = replay(&list);
        assert_eq!(calls.len(), 3);
        let Call::Fill(_, _, _, s) = &calls[0] else {
            panic!()
        };
        assert_eq!(*s, state(Transform::translate(10.0, 20.0), 0.8));
        let Call::Text(_, _, s) = &calls[1] else {
            panic!()
        };
        assert_eq!(
            s.transform,
            Transform::translate(10.0, 20.0).concat(&Transform::rotate(FRAC_PI_2))
        );
        assert!(close(s.alpha, 0.4));
        // Leaving a group restores the parent state.
        let Call::Fill(_, _, rule, s) = &calls[2] else {
            panic!()
        };
        assert_eq!(*rule, FillRule::EvenOdd);
        assert_eq!(*s, PaintState::ROOT);
    }

    #[test]
    fn opacity_canvas_ignores_keeps_the_parent_alpha() {
        // globalAlpha = x is ignored unless 0 <= x <= 1.
        for bad in [f64::NAN, -0.5, 3.0, f64::INFINITY] {
            let mut list = DisplayList::new();
            list.push(DisplayItem::Group(Group {
                transform: Transform::IDENTITY,
                opacity: 0.5,
                clip: None,
                items: vec![DisplayItem::Group(Group {
                    transform: Transform::IDENTITY,
                    opacity: bad,
                    clip: None,
                    items: vec![DisplayItem::Fill {
                        path: Path::rect(0.0, 0.0, 1.0, 1.0),
                        color: Color::new("#000"),
                        rule: FillRule::NonZero,
                    }],
                })],
            }));
            let Call::Fill(_, _, _, s) = &replay(&list)[0] else {
                panic!()
            };
            assert_eq!(s.alpha, 0.5, "opacity {bad}");
        }
        // 1.5 under a 0.5 parent is 0.75, which the canvas accepts.
        let mut list = DisplayList::new();
        list.push(DisplayItem::Group(Group {
            transform: Transform::IDENTITY,
            opacity: 0.5,
            clip: None,
            items: vec![DisplayItem::Group(Group {
                transform: Transform::IDENTITY,
                opacity: 1.5,
                clip: None,
                items: vec![DisplayItem::Fill {
                    path: Path::rect(0.0, 0.0, 1.0, 1.0),
                    color: Color::new("#000"),
                    rule: FillRule::NonZero,
                }],
            })],
        }));
        let Call::Fill(_, _, _, s) = &replay(&list)[0] else {
            panic!()
        };
        assert_eq!(s.alpha, 0.75);
    }

    #[test]
    fn clips_push_in_the_group_space_and_pop_on_exit() {
        // staticScene.ts:165-189: save, clip to the frame's roundRect, draw
        // the frame's children, restore.
        let clip_path = Path::round_rect(0.0, 0.0, 50.0, 50.0, 8.0);
        let mut list = DisplayList::new();
        list.push(DisplayItem::Group(Group {
            transform: Transform::translate(5.0, 5.0),
            opacity: 1.0,
            clip: Some(Clip {
                path: clip_path.clone(),
                rule: FillRule::NonZero,
            }),
            items: vec![DisplayItem::Group(Group {
                transform: Transform::scale(2.0, 2.0),
                opacity: 1.0,
                clip: Some(Clip {
                    path: Path::rect(0.0, 0.0, 1.0, 1.0),
                    rule: FillRule::EvenOdd,
                }),
                items: vec![DisplayItem::Fill {
                    path: Path::rect(0.0, 0.0, 1.0, 1.0),
                    color: Color::new("#000"),
                    rule: FillRule::NonZero,
                }],
            })],
        }));
        list.push(DisplayItem::Fill {
            path: Path::rect(0.0, 0.0, 1.0, 1.0),
            color: Color::new("#000"),
            rule: FillRule::NonZero,
        });
        let calls = replay(&list);
        let inner = Transform::translate(5.0, 5.0).concat(&Transform::scale(2.0, 2.0));
        let unit = Path::rect(0.0, 0.0, 1.0, 1.0).commands;
        assert_eq!(
            calls,
            vec![
                Call::PushClip(
                    clip_path.commands,
                    FillRule::NonZero,
                    Transform::translate(5.0, 5.0)
                ),
                Call::PushClip(unit.clone(), FillRule::EvenOdd, inner),
                Call::Fill(unit.clone(), BLACK, FillRule::NonZero, state(inner, 1.0)),
                Call::PopClip,
                Call::PopClip,
                Call::Fill(unit, BLACK, FillRule::NonZero, PaintState::ROOT),
            ]
        );
    }

    #[test]
    fn items_with_no_colour_are_not_painted() {
        // A value that is not a colour paints nothing; "transparent" is a
        // colour with alpha 0 and is passed on.
        let mut list = DisplayList::new();
        list.push(DisplayItem::Fill {
            path: Path::rect(0.0, 0.0, 1.0, 1.0),
            color: Color::new(""),
            rule: FillRule::NonZero,
        });
        list.push(DisplayItem::Stroke {
            path: Path::rect(0.0, 0.0, 1.0, 1.0),
            stroke: Stroke::new(Color::new("bogus"), 1.0),
        });
        list.push(DisplayItem::Text(TextRun::new(
            "x",
            0.0,
            0.0,
            Font::new(1.0, "a"),
            Color::new("none"),
        )));
        list.push(DisplayItem::Stroke {
            path: Path::rect(0.0, 0.0, 1.0, 1.0),
            stroke: Stroke::new(Color::new("transparent"), 1.0),
        });
        let calls = replay(&list);
        assert_eq!(calls.len(), 1);
        assert!(matches!(&calls[0], Call::Stroke(_, _, c, _) if c.a == 0.0));
    }

    #[test]
    fn replay_from_a_base_state() {
        // bootstrapCanvas (renderer/helpers.ts:73-127) scales by the device
        // pixel ratio before anything is drawn; a backend passes that in.
        let mut list = DisplayList::new();
        list.push(DisplayItem::Fill {
            path: Path::rect(0.0, 0.0, 1.0, 1.0),
            color: Color::new("#000"),
            rule: FillRule::NonZero,
        });
        let mut r = Recorder::default();
        list.replay_from(&mut r, state(Transform::scale(2.0, 2.0), 0.5));
        let Call::Fill(_, _, _, s) = &r.0[0] else {
            panic!()
        };
        assert_eq!(*s, state(Transform::scale(2.0, 2.0), 0.5));
    }

    #[test]
    fn display_list_collects_and_iterates() {
        let items = vec![
            DisplayItem::Fill {
                path: Path::rect(0.0, 0.0, 1.0, 1.0),
                color: Color::new("#000"),
                rule: FillRule::NonZero,
            },
            DisplayItem::Text(TextRun::new(
                "t",
                0.0,
                0.0,
                Font::new(1.0, "a"),
                Color::new("#000"),
            )),
        ];
        let list: DisplayList = items.clone().into_iter().collect();
        assert_eq!(list.items, items);
        assert_eq!(list.iter().count(), 2);
        let mut extended = DisplayList::new();
        extended.extend(items);
        assert_eq!(extended, list);
    }
}

mod boundaries {
    /// ADR-008: the display list carries no element knowledge. The module
    /// that defines it must not name `excali_core`'s element model or the
    /// sketch generators; only the colour parser comes from `excali-core`.
    #[test]
    fn display_module_does_not_know_elements() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/display");
        let mut sources = 0;
        for entry in std::fs::read_dir(dir).expect("src/display exists") {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            sources += 1;
            for forbidden in [
                "excali_core::element",
                "excali_core::document",
                "excali_rough",
                "excali_freehand",
                "Element",
            ] {
                assert!(
                    !text.contains(forbidden),
                    "{} mentions {forbidden}",
                    path.display()
                );
            }
        }
        assert!(sources > 1);
    }
}
