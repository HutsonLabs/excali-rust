//! Fill behaviour the goldens cannot carry (ex-204): hachure-fill's scan on
//! its own, the `dots` filler and the other `Math.random` paths (the golden
//! generator makes `Math.random` throw), and the inputs on which rough.js
//! throws or never returns. `goldens/rough-fills.json` pins every
//! deterministic fill op by op (`tests/goldens.rs`).
//!
//! Expected values without a comment come from running roughjs 4.6.4's
//! `hachure-fill` 0.5.2 under Node (`tools/goldens/node_modules`).

use excali_rough::hachure_fill::hachure_lines;
use excali_rough::path_data::PathError;
use excali_rough::renderer::{pattern_fill_polygons, solid_fill_polygon};
use excali_rough::{Op, OpSetType, Options, Point, Random, RoughGenerator};

fn filled(seed: i32, style: &str) -> Options {
    Options {
        seed,
        fill: Some("#a5d8ff".to_owned()),
        fill_style: style.to_owned(),
        fill_weight: 1.0,
        hachure_gap: 8.0,
        ..Options::default()
    }
}

fn close(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-10 * b.abs().max(1.0)
}

fn assert_lines(actual: &[[Point; 2]], expected: &[[Point; 2]]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (a, e) in actual.iter().zip(expected) {
        for i in 0..2 {
            for j in 0..2 {
                assert!(close(a[i][j], e[i][j]), "{actual:?} != {expected:?}");
            }
        }
    }
}

fn square(size: f64) -> Vec<Vec<Point>> {
    vec![vec![[0.0, 0.0], [size, 0.0], [size, size], [0.0, size]]]
}

fn moves(ops: &[Op]) -> usize {
    ops.iter().filter(|op| matches!(op, Op::Move(_))).count()
}

mod hachure_fill {
    use super::*;

    #[test]
    fn scans_every_gap_from_the_lowest_edge() {
        // hachureLines([[0,0],[10,0],[10,10],[0,10]], 2, 0, 1)
        let lines = hachure_lines(&mut square(10.0), 2.0, 0.0, 1.0);
        assert_eq!(
            lines,
            vec![
                [[0.0, 0.0], [10.0, 0.0]],
                [[0.0, 2.0], [10.0, 2.0]],
                [[0.0, 4.0], [10.0, 4.0]],
                [[0.0, 6.0], [10.0, 6.0]],
                [[0.0, 8.0], [10.0, 8.0]],
            ]
        );
    }

    #[test]
    fn step_offset_one_keeps_iterations_divisible_by_the_gap() {
        // iteration % 2.5 === 0 only every fifth unit step
        let lines = hachure_lines(&mut square(10.0), 2.5, 0.0, 1.0);
        assert_eq!(
            lines,
            vec![[[0.0, 0.0], [10.0, 0.0]], [[0.0, 5.0], [10.0, 5.0]]]
        );
        // any other step draws on every step
        let lines = hachure_lines(&mut square(10.0), 2.5, 0.0, 2.5);
        assert_eq!(
            lines,
            vec![
                [[0.0, 0.0], [10.0, 0.0]],
                [[0.0, 2.5], [10.0, 2.5]],
                [[0.0, 5.0], [10.0, 5.0]],
                [[0.0, 7.5], [10.0, 7.5]],
            ]
        );
    }

    #[test]
    fn gap_is_at_least_a_tenth() {
        let a = hachure_lines(&mut square(1.0), 0.01, 0.0, 1.0);
        let b = hachure_lines(&mut square(1.0), 0.1, 0.0, 1.0);
        assert_eq!(a, b);
    }

    #[test]
    fn x_is_rounded_like_math_round() {
        // Math.round: halves go towards +infinity
        let mut p = vec![vec![[10.5, 0.0], [20.5, 0.0], [20.5, 3.0], [10.5, 3.0]]];
        assert_eq!(
            hachure_lines(&mut p, 1.0, 0.0, 1.0),
            vec![
                [[11.0, 0.0], [21.0, 0.0]],
                [[11.0, 1.0], [21.0, 1.0]],
                [[11.0, 2.0], [21.0, 2.0]],
            ]
        );
        let mut p = vec![vec![[-10.5, 0.0], [-20.5, 0.0], [-20.5, 3.0], [-10.5, 3.0]]];
        assert_eq!(
            hachure_lines(&mut p, 1.0, 0.0, 1.0),
            vec![
                [[-20.0, 0.0], [-10.0, 0.0]],
                [[-20.0, 1.0], [-10.0, 1.0]],
                [[-20.0, 2.0], [-10.0, 2.0]],
            ]
        );
    }

    #[test]
    fn rotated_concave_polygon_pairs_active_edges() {
        let mut p = vec![vec![
            [0.0, 0.0],
            [40.0, 0.0],
            [40.0, 30.0],
            [20.0, 10.0],
            [0.0, 30.0],
        ]];
        let lines = hachure_lines(&mut p, 8.0, 45.0, 1.0);
        assert_lines(
            &lines,
            &[
                [[0.0, 0.0], [0.0, 0.0]],
                [
                    [-8.881784197001252e-16, 11.31370849898476],
                    [11.31370849898476, 8.881784197001252e-16],
                ],
                [
                    [-1.7763568394002505e-15, 22.62741699796952],
                    [22.62741699796952, 1.7763568394002505e-15],
                ],
                [
                    [21.920310216782973, 12.02081528017131],
                    [33.941125496954285, 3.552713678800501e-15],
                ],
                [
                    [27.577164466275352, 17.67766952966369],
                    [40.30508652763321, 4.949747468305837],
                ],
                [
                    [33.23401871576773, 23.334523779156072],
                    [40.30508652763321, 16.263455967290597],
                ],
                [
                    [38.89087296526011, 28.991378028648455],
                    [40.30508652763321, 27.577164466275356],
                ],
            ],
        );
        // the polygon is rotated in place and back, as in hachure-fill:
        // [[0,0],[40,0],[40,30.000000000000004],[20,10.000000000000002],
        //  [1.7763568394002505e-15,30]]
        let expected = [
            [0.0, 0.0],
            [40.0, 0.0],
            [40.0, 30.000000000000004],
            [20.0, 10.000000000000002],
            [1.7763568394002505e-15, 30.0],
        ];
        for (a, e) in p[0].iter().zip(expected) {
            assert!(close(a[0], e[0]) && close(a[1], e[1]), "{:?}", p[0]);
        }
    }

    #[test]
    fn open_polygons_are_closed_and_short_ones_skipped() {
        let closed = vec![vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
            [0.0, 0.0],
        ]];
        assert_eq!(
            hachure_lines(&mut closed.clone(), 2.0, 0.0, 1.0),
            hachure_lines(&mut square(10.0), 2.0, 0.0, 1.0)
        );
        // one vertex: [p, p] after closing, not more than two, so no edges
        assert!(hachure_lines(&mut [vec![[5.0, 5.0]]], 2.0, 0.0, 1.0).is_empty());
    }

    #[test]
    fn empty_polygons_are_skipped() {
        // hachure-fill reads vertices[0][0] and throws a TypeError; the port
        // skips the polygon.
        let mut p = vec![Vec::new(), square(10.0).remove(0)];
        assert_eq!(hachure_lines(&mut p, 2.0, 0.0, 1.0).len(), 5);
    }

    #[test]
    fn non_finite_coordinates_return_instead_of_scanning_forever() {
        // An edge whose ymin or ymax is NaN or infinite is never retired, so
        // hachure-fill's scan never ends; the port returns what it has.
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut p = vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, bad], [0.0, 10.0]]];
            let lines = hachure_lines(&mut p, 2.0, 0.0, 1.0);
            assert!(lines.len() <= 5, "{bad}: {lines:?}");
            let mut p = vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, bad], [0.0, 10.0]]];
            hachure_lines(&mut p, 2.0, 30.0, 1.0);
        }
        // a scan line that no longer moves (|y| >= 2^53 with step 1)
        let mut p = vec![vec![
            [0.0, 1e17],
            [10.0, 1e17],
            [10.0, 1e17 + 64.0],
            [0.0, 1e17 + 64.0],
        ]];
        hachure_lines(&mut p, 2.0, 0.0, 1.0);
    }
}

mod fillers {
    use super::*;

    #[test]
    fn solid_fill_polygon_jitters_each_vertex_once() {
        let o = Options {
            max_randomness_offset: 0.0,
            ..filled(3, "solid")
        };
        let mut rng = Random::new(3);
        let set = solid_fill_polygon(&square(10.0), &o, &mut rng);
        assert_eq!(set.kind, OpSetType::FillPath);
        assert_eq!(
            set.ops,
            vec![
                Op::Move([0.0, 0.0]),
                Op::LineTo([10.0, 0.0]),
                Op::LineTo([10.0, 10.0]),
                Op::LineTo([0.0, 10.0]),
            ]
        );
        // two draws per vertex
        let mut expected = Random::new(3);
        (0..8).for_each(|_| {
            expected.next();
        });
        assert_eq!(rng.next(), expected.next());
        // two points or fewer: nothing
        let set = solid_fill_polygon(&[vec![[0.0, 0.0], [1.0, 1.0]]], &o, &mut rng);
        assert!(set.ops.is_empty());
    }

    #[test]
    fn pattern_fills_rotate_the_polygons_in_place() {
        let o = filled(5, "hachure");
        let mut polygons = vec![vec![[0.0, 0.0], [80.0, 10.0], [90.0, 60.0], [15.0, 50.0]]];
        let set = pattern_fill_polygons(&mut polygons, &o, &mut Random::new(5));
        assert_eq!(set.kind, OpSetType::FillSketch);
        assert!(!set.ops.is_empty());
        // hachureLines(q, 8, 49, 1) leaves
        // [[0,0],[80,10.000000000000007],[90,60],[15.000000000000007,50]]
        let expected = [
            [0.0, 0.0],
            [80.0, 10.000000000000007],
            [90.0, 60.0],
            [15.000000000000007, 50.0],
        ];
        for (a, e) in polygons[0].iter().zip(expected) {
            assert!(close(a[0], e[0]) && close(a[1], e[1]), "{polygons:?}");
        }
    }

    #[test]
    fn dots_are_small_ellipses_along_vertical_scan_lines() {
        // dot-filler.js: hachureAngle 0 (scan lines at 90 degrees), one
        // fillWeight-sized ellipse every gap, jittered by Math.random within
        // gap / 4. At roughness 0 each ellipse is one stroke (one move).
        let o = Options {
            roughness: 0.0,
            ..filled(11, "dots")
        };
        let d = RoughGenerator::new().rectangle(0.0, 0.0, 100.0, 60.0, &o);
        assert_eq!(d.sets.len(), 2);
        let fill = &d.sets[0];
        assert_eq!(fill.kind, OpSetType::FillSketch);
        let lines = hachure_lines(
            &mut [vec![[0.0, 0.0], [100.0, 0.0], [100.0, 60.0], [0.0, 60.0]]],
            8.0,
            90.0,
            1.0,
        );
        let dots: usize = lines
            .iter()
            .map(|[a, b]| {
                let length = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
                ((length / 8.0).ceil() - 1.0).max(0.0) as usize
            })
            .sum();
        assert!(dots > 0);
        assert_eq!(moves(&fill.ops), dots);
        // every dot stays within the jitter (gap / 4) and its own size of
        // the rectangle, shifted left by gap / 4
        for op in &fill.ops {
            let data = op.data();
            for xy in data.chunks(2) {
                assert!(
                    (-4.0 - 2.0 - 1.0..=100.0 + 2.0 + 1.0).contains(&xy[0]),
                    "{op:?}"
                );
                assert!(
                    (-2.0 - 1.0..=60.0 + 8.0 + 2.0 + 1.0).contains(&xy[1]),
                    "{op:?}"
                );
            }
        }
        // Math.random: two drawings differ
        let again = RoughGenerator::new().rectangle(0.0, 0.0, 100.0, 60.0, &o);
        assert_ne!(again.sets[0], d.sets[0]);
    }

    #[test]
    fn dots_at_roughness_one_draw_two_strokes_per_dot() {
        let o = filled(11, "dots");
        let d = RoughGenerator::new().ellipse(60.0, 40.0, 100.0, 60.0, &o);
        let fill = &d.sets[0];
        assert_eq!(fill.kind, OpSetType::FillSketch);
        assert!(moves(&fill.ops) > 0);
        assert_eq!(moves(&fill.ops) % 2, 0);
        assert!(fill
            .ops
            .iter()
            .all(|op| op.data().iter().all(|v| v.is_finite())));
    }

    #[test]
    fn seed_zero_fills_draw_from_math_random() {
        let o = filled(0, "cross-hatch");
        let g = RoughGenerator::new();
        let a = g.rectangle(10.0, 10.0, 100.0, 60.0, &o);
        let b = g.rectangle(10.0, 10.0, 100.0, 60.0, &o);
        assert_eq!(a.sets[0].kind, OpSetType::FillSketch);
        assert!(!a.sets[0].ops.is_empty());
        assert_ne!(a.sets[0], b.sets[0]);
    }

    #[test]
    fn a_zero_draw_falls_back_to_math_random_for_the_skip_offset() {
        // `(o.randomizer?.next() || Math.random()) > 0.7`: seed -2^31 is a
        // fixed point of the generator (every draw is 0), so the skip offset
        // comes from Math.random: sometimes 1, sometimes the gap.
        let o = Options {
            hachure_gap: 2.5,
            hachure_angle: -90.0,
            ..filled(i32::MIN, "hachure")
        };
        let g = RoughGenerator::new();
        // four ops per hachure line (two strokes of move + bcurveTo); step 1
        // gives lines at y = 0 and 5, step 2.5 at 0, 2.5, 5, 7.5
        let counts: std::collections::BTreeSet<usize> = (0..64)
            .map(|_| g.rectangle(0.0, 0.0, 10.0, 10.0, &o).sets[0].ops.len())
            .collect();
        assert_eq!(counts, [8, 16].into_iter().collect());
    }

    #[test]
    fn no_draw_before_the_fill_uses_math_random_and_draws_nothing() {
        // path("M 10 10"): svgPath draws no random numbers, so the options
        // have no randomizer when polygonHachureLines runs; the one-point set
        // has no scan lines.
        let o = filled(1, "hachure");
        let d = RoughGenerator::new().path("M 10 10", &o).unwrap();
        assert_eq!(d.sets.len(), 2);
        assert_eq!(d.sets[0].kind, OpSetType::FillSketch);
        assert!(d.sets[0].ops.is_empty());
        // and a polygon with one point (no outline draws either)
        let d = RoughGenerator::new().polygon(&[[3.0, 4.0]], &o);
        assert_eq!(d.sets[0].kind, OpSetType::FillSketch);
        assert!(d.sets[0].ops.is_empty());
    }

    #[test]
    fn a_curve_fill_that_never_flattens_is_an_error() {
        // pointsOnBezierCurves subdivides a NaN curve until the stack
        // overflows (RangeError in rough.js)
        let o = filled(1, "hachure");
        let pts = [[0.0, 0.0], [f64::NAN, 30.0], [60.0, 5.0], [100.0, 40.0]];
        assert_eq!(
            RoughGenerator::new().curve(&pts, &o),
            Err(PathError::CallStackExceeded)
        );
        // unfilled, the same curve draws
        let o = Options { fill: None, ..o };
        assert!(RoughGenerator::new().curve(&pts, &o).is_ok());
    }

    #[test]
    fn fill_truthiness_follows_each_method() {
        let g = RoughGenerator::new();
        let empty = Options {
            fill: Some(String::new()),
            ..filled(1, "hachure")
        };
        // "" is falsy everywhere
        assert_eq!(g.rectangle(0.0, 0.0, 10.0, 10.0, &empty).sets.len(), 1);
        let none = Options {
            fill: Some("none".to_owned()),
            ..filled(1, "hachure")
        };
        // rectangle, polygon, ellipse and arc only test `o.fill`
        assert_eq!(g.rectangle(0.0, 0.0, 10.0, 10.0, &none).sets.len(), 2);
        assert_eq!(g.ellipse(0.0, 0.0, 10.0, 10.0, &none).sets.len(), 2);
        let tri = [[0.0, 0.0], [30.0, 30.0], [60.0, 0.0]];
        assert_eq!(g.polygon(&tri, &none).sets.len(), 2);
        assert_eq!(
            g.arc(0.0, 0.0, 10.0, 10.0, 0.0, 3.0, true, &none)
                .sets
                .len(),
            2
        );
        // curve and path skip "none"; path also skips "transparent"
        assert_eq!(g.curve(&tri, &none).unwrap().sets.len(), 1);
        assert_eq!(
            g.path("M 0 0 L 10 10 L 0 10 Z", &none).unwrap().sets.len(),
            1
        );
        let transparent = Options {
            fill: Some("transparent".to_owned()),
            ..filled(1, "hachure")
        };
        assert_eq!(
            g.path("M 0 0 L 10 10 L 0 10 Z", &transparent)
                .unwrap()
                .sets
                .len(),
            1
        );
        assert_eq!(
            g.rectangle(0.0, 0.0, 10.0, 10.0, &transparent).sets.len(),
            2
        );
    }

    #[test]
    fn fill_comes_before_the_outline() {
        let d = RoughGenerator::new().rectangle(0.0, 0.0, 10.0, 10.0, &filled(1, "solid"));
        let kinds: Vec<_> = d.sets.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [OpSetType::FillPath, OpSetType::Path]);
    }
}
