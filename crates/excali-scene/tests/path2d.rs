//! `new Path2D(d)` (ex-402): upstream fills a freedraw's outline with
//! `context.fill(new Path2D(svgPath))` (`renderElement.ts:496-516`), so the
//! static scene reads SVG path data into a display-list path as Chrome's
//! Path2D constructor does (the SVG path grammar, "render up to the
//! error").

use excali_math::js;
use excali_scene::display::{Path, PathCommand};
use excali_scene::freedraw::get_svg_path_from_stroke;
use PathCommand::*;

fn cmds(d: &str) -> Vec<PathCommand> {
    Path::from_svg_path_data(d).commands
}

fn close_to(actual: &[PathCommand], expected: &[PathCommand]) -> bool {
    fn values(c: &PathCommand) -> Vec<f64> {
        match *c {
            MoveTo(x, y) | LineTo(x, y) => vec![x, y],
            QuadTo(a, b, c, d) => vec![a, b, c, d],
            CubicTo(a, b, c, d, e, f) => vec![a, b, c, d, e, f],
            _ => vec![],
        }
    }
    actual.len() == expected.len()
        && actual.iter().zip(expected).all(|(a, e)| {
            std::mem::discriminant(a) == std::mem::discriminant(e)
                && values(a)
                    .iter()
                    .zip(values(e))
                    .all(|(x, y)| (x - y).abs() < 1e-9)
        })
}

#[test]
fn excalidraws_freedraw_paths() {
    // getSvgPathFromStroke: M p0 Q p_i mid … L p0 Z
    let d = get_svg_path_from_stroke(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
    assert_eq!(d, "M 0,0 Q 0,0 5,0 10,0 10,5 10,10 5,5 L 0,0 Z");
    assert_eq!(
        cmds(&d),
        [
            MoveTo(0.0, 0.0),
            QuadTo(0.0, 0.0, 5.0, 0.0),
            QuadTo(10.0, 0.0, 10.0, 5.0),
            QuadTo(10.0, 10.0, 5.0, 5.0),
            LineTo(0.0, 0.0),
            Close,
        ]
    );
    assert_eq!(cmds(""), []);
}

#[test]
fn numbers() {
    assert_eq!(cmds("M1.5.5"), [MoveTo(1.5, 0.5)]);
    assert_eq!(cmds("M-1-2"), [MoveTo(-1.0, -2.0)]);
    // a two-decimal trim of 1.5e+21 leaves 1.5+21: two numbers
    assert_eq!(cmds("M 1.5+21"), [MoveTo(1.5, 21.0)]);
    assert_eq!(cmds("M1e1 2E-1"), [MoveTo(10.0, 0.2)]);
    assert_eq!(cmds("M .5 , .25"), [MoveTo(0.5, 0.25)]);
    // an exponent without digits is an error
    assert_eq!(cmds("M0 0 L1e 2"), [MoveTo(0.0, 0.0)]);
}

#[test]
fn commands_and_repeats() {
    assert_eq!(
        cmds("M0 0 1 1 2 2"),
        [MoveTo(0.0, 0.0), LineTo(1.0, 1.0), LineTo(2.0, 2.0)]
    );
    assert_eq!(cmds("m1 1 1 1"), [MoveTo(1.0, 1.0), LineTo(2.0, 2.0)]);
    assert_eq!(
        cmds("m1 1 l2 0 h3 v4 z l1 1"),
        [
            MoveTo(1.0, 1.0),
            LineTo(3.0, 1.0),
            LineTo(6.0, 1.0),
            LineTo(6.0, 5.0),
            Close,
            LineTo(2.0, 2.0),
        ]
    );
    assert_eq!(
        cmds("M0 0 H5 V5 L0 5z"),
        [
            MoveTo(0.0, 0.0),
            LineTo(5.0, 0.0),
            LineTo(5.0, 5.0),
            LineTo(0.0, 5.0),
            Close,
        ]
    );
}

#[test]
fn smooth_curves_reflect_the_previous_control_point() {
    assert_eq!(
        cmds("M0 0 C 1 1 2 1 3 0 S 5 -1 6 0"),
        [
            MoveTo(0.0, 0.0),
            CubicTo(1.0, 1.0, 2.0, 1.0, 3.0, 0.0),
            CubicTo(4.0, -1.0, 5.0, -1.0, 6.0, 0.0),
        ]
    );
    // S after a line: the first control point is the current point
    assert_eq!(
        cmds("M0 0 L 3 0 S 5 -1 6 0"),
        [
            MoveTo(0.0, 0.0),
            LineTo(3.0, 0.0),
            CubicTo(3.0, 0.0, 5.0, -1.0, 6.0, 0.0),
        ]
    );
    assert_eq!(
        cmds("M0 0 Q 1 2 2 0 T 4 0 t 2 0"),
        [
            MoveTo(0.0, 0.0),
            QuadTo(1.0, 2.0, 2.0, 0.0),
            QuadTo(3.0, -2.0, 4.0, 0.0),
            QuadTo(5.0, 2.0, 6.0, 0.0),
        ]
    );
}

#[test]
fn errors_keep_what_came_before() {
    assert_eq!(cmds("L1 1"), []);
    assert_eq!(cmds("M0 0 L1"), [MoveTo(0.0, 0.0)]);
    assert_eq!(
        cmds("M0 0 L1 1 X 2 2"),
        [MoveTo(0.0, 0.0), LineTo(1.0, 1.0)]
    );
    // parameters after Z
    assert_eq!(
        cmds("M0 0 L1 1 Z 2 2"),
        [MoveTo(0.0, 0.0), LineTo(1.0, 1.0), Close]
    );
    // two commas
    assert_eq!(cmds("M0,,0"), []);
    // a bad arc flag
    assert_eq!(cmds("M0 0 A1 1 0 2 0 3 3"), [MoveTo(0.0, 0.0)]);
}

#[test]
fn arcs() {
    // a half circle of radius 10 from (0, 0) to (20, 0), clockwise on
    // screen through (10, -10): two quarter-turn cubics
    let k = 4.0 / 3.0 * js::tan(std::f64::consts::FRAC_PI_8) * 10.0;
    assert!(close_to(
        &cmds("M0 0 A 10 10 0 0 1 20 0"),
        &[
            MoveTo(0.0, 0.0),
            CubicTo(0.0, -k, 10.0 - k, -10.0, 10.0, -10.0),
            CubicTo(10.0 + k, -10.0, 20.0, -k, 20.0, 0.0),
        ]
    ));
    // the other sweep goes through (10, 10)
    let other = cmds("M0 0 a 10 10 0 0 0 20 0");
    assert!(
        matches!(other[1], CubicTo(_, _, _, _, x, y) if (x - 10.0).abs() < 1e-9 && (y - 10.0).abs() < 1e-9)
    );
    // radii too small are scaled up; a zero radius is a line; an arc to
    // the current point adds nothing
    assert_eq!(cmds("M0 0 A 1 1 0 0 1 20 0").len(), 3);
    assert_eq!(
        cmds("M0 0 A 0 5 0 0 1 20 0"),
        [MoveTo(0.0, 0.0), LineTo(20.0, 0.0)]
    );
    assert_eq!(cmds("M0 0 A 5 5 0 0 1 0 0"), [MoveTo(0.0, 0.0)]);
    // flags may run into the next number
    assert_eq!(cmds("M0 0 A5 5 0 0120 0").len(), 3);
}
