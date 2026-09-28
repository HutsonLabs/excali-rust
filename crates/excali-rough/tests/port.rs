//! Behaviour the goldens cannot carry: the seed-0 fallback to `Math.random`
//! (the golden generator makes `Math.random` throw), the inputs on which
//! rough.js throws or never returns, and the parser stages on their own.

use excali_rough::path_data::{absolutize, normalize, parse_path, PathError, Segment};
use excali_rough::points_on_curve::{
    curve_to_bezier, points_on_bezier_curves, simplify, simplify_points,
};
use excali_rough::points_on_path::points_on_path;
use excali_rough::{random_seed, Op, OpSetType, Options, Random, RoughGenerator, Shape};

fn seg(key: char, data: &[f64]) -> Segment {
    Segment {
        key,
        data: data.to_vec(),
    }
}

fn seeded(seed: i32) -> Options {
    Options {
        seed,
        ..Options::default()
    }
}

mod random {
    use super::*;

    #[test]
    fn park_miller_first_draws() {
        // bin/math.js: ((2 ** 31 - 1) & (seed = Math.imul(48271, seed))) / 2 ** 31
        let mut r = Random::new(1);
        assert_eq!(r.next(), 48271.0 / 2_147_483_648.0);
        assert_eq!(
            r.next(),
            f64::from(48271i32.wrapping_mul(48271) & 0x7fff_ffff) / 2_147_483_648.0
        );
    }

    #[test]
    fn imul_wraps_like_javascript() {
        // Math.imul(48271, 2147483647) = -48271; & (2^31 - 1) = 2147435377
        let mut r = Random::new(i32::MAX);
        assert_eq!(r.next(), 2_147_435_377.0 / 2_147_483_648.0);
    }

    #[test]
    fn seed_zero_falls_back_to_an_unseeded_source() {
        let mut r = Random::new(0);
        let draws: Vec<f64> = (0..64).map(|_| r.next()).collect();
        assert!(draws.iter().all(|x| (0.0..1.0).contains(x)));
        // Math.random, not the LCG: seed 0 would give 0 forever.
        assert!(draws.iter().any(|&x| x != draws[0]));
        assert!(draws.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn new_seed_is_a_31_bit_non_negative_integer() {
        // generator.js newSeed -> math.js randomSeed: floor(Math.random() * 2 ** 31)
        let seeds: Vec<i32> = (0..100).map(|_| random_seed()).collect();
        assert!(seeds.iter().all(|&s| s >= 0));
        assert!(seeds.iter().any(|&s| s != seeds[0]));
        assert!(RoughGenerator::new_seed() >= 0);
    }
}

mod options {
    use super::*;

    #[test]
    fn defaults_are_the_generator_js_block() {
        let o = Options::default();
        assert_eq!(o.max_randomness_offset, 2.0);
        assert_eq!(o.roughness, 1.0);
        assert_eq!(o.bowing, 1.0);
        assert_eq!(o.stroke, "#000");
        assert_eq!(o.stroke_width, 1.0);
        assert_eq!(o.curve_tightness, 0.0);
        assert_eq!(o.curve_fitting, 0.95);
        assert_eq!(o.curve_step_count, 9.0);
        assert_eq!(o.fill_style, "hachure");
        assert_eq!(o.fill_weight, -1.0);
        assert_eq!(o.hachure_angle, -41.0);
        assert_eq!(o.hachure_gap, -1.0);
        assert_eq!(o.dash_offset, -1.0);
        assert_eq!(o.dash_gap, -1.0);
        assert_eq!(o.zigzag_offset, -1.0);
        assert_eq!(o.seed, 0);
        assert!(!o.disable_multi_stroke);
        assert!(!o.disable_multi_stroke_fill);
        assert!(!o.preserve_vertices);
        assert_eq!(o.fill_shape_roughness_gain, 0.8);
        assert_eq!(o.simplification, None);
        assert_eq!(o.stroke_line_dash, None);
        assert_eq!(o.stroke_line_dash_offset, None);
        assert_eq!(o.fill_line_dash, None);
        assert_eq!(o.fill_line_dash_offset, None);
        assert_eq!(o.fixed_decimal_place_digits, None);
    }

    #[test]
    fn config_options_become_the_generator_defaults() {
        // new RoughGenerator({ options }) -> defaultOptions = _o(config.options)
        let g = RoughGenerator::with_options(Options {
            roughness: 2.5,
            ..Options::default()
        });
        assert_eq!(g.default_options().roughness, 2.5);
        assert_eq!(RoughGenerator::new().default_options(), &Options::default());
    }

    #[test]
    fn drawable_carries_the_resolved_options() {
        let o = Options {
            seed: 9,
            stroke_line_dash: Some(vec![8.0, 10.0]),
            ..Options::default()
        };
        let d = RoughGenerator::new().line(0.0, 0.0, 10.0, 10.0, &o);
        assert_eq!(d.shape, Shape::Line);
        assert_eq!(d.shape.as_str(), "line");
        assert_eq!(d.options, o);
    }
}

mod generator {
    use super::*;

    #[test]
    fn same_seed_same_drawing() {
        let g = RoughGenerator::new();
        let a = g.ellipse(10.0, 10.0, 80.0, 40.0, &seeded(42));
        let b = g.ellipse(10.0, 10.0, 80.0, 40.0, &seeded(42));
        assert_eq!(a, b);
        let c = g.ellipse(10.0, 10.0, 80.0, 40.0, &seeded(43));
        assert_ne!(a, c);
    }

    #[test]
    fn shapes_are_named_like_rough_js() {
        let g = RoughGenerator::new();
        let o = seeded(1);
        let pts = [[0.0, 0.0], [10.0, 10.0], [20.0, 0.0]];
        assert_eq!(
            g.rectangle(0.0, 0.0, 1.0, 1.0, &o).shape.as_str(),
            "rectangle"
        );
        assert_eq!(g.ellipse(0.0, 0.0, 1.0, 1.0, &o).shape.as_str(), "ellipse");
        assert_eq!(g.circle(0.0, 0.0, 1.0, &o).shape.as_str(), "circle");
        assert_eq!(g.linear_path(&pts, &o).shape.as_str(), "linearPath");
        assert_eq!(g.polygon(&pts, &o).shape.as_str(), "polygon");
        assert_eq!(g.curve(&pts, &o).shape.as_str(), "curve");
        assert_eq!(
            g.arc(0.0, 0.0, 1.0, 1.0, 0.0, 1.0, false, &o)
                .shape
                .as_str(),
            "arc"
        );
        assert_eq!(g.path("M0 0L1 1", &o).unwrap().shape.as_str(), "path");
        let set = &g.line(0.0, 0.0, 1.0, 1.0, &o).sets[0];
        assert_eq!(set.kind, OpSetType::Path);
        assert_eq!(set.kind.as_str(), "path");
        assert_eq!(OpSetType::FillPath.as_str(), "fillPath");
        assert_eq!(OpSetType::FillSketch.as_str(), "fillSketch");
    }

    #[test]
    fn ops_name_and_data() {
        let m = Op::Move([1.0, 2.0]);
        let l = Op::LineTo([3.0, 4.0]);
        let c = Op::BCurveTo([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!((m.name(), m.data()), ("move", &[1.0, 2.0][..]));
        assert_eq!((l.name(), l.data()), ("lineTo", &[3.0, 4.0][..]));
        assert_eq!(c.name(), "bcurveTo");
        assert_eq!(c.data().len(), 6);
    }

    #[test]
    fn empty_curve_is_an_empty_path() {
        // rough.js reads points[0][0] and throws a TypeError; the port draws
        // nothing instead of panicking.
        let d = RoughGenerator::new().curve(&[], &seeded(1));
        assert_eq!(d.sets.len(), 1);
        assert!(d.sets[0].ops.is_empty());
    }

    #[test]
    fn path_errors_are_the_parser_errors() {
        let g = RoughGenerator::new();
        let o = seeded(1);
        // "10 20 L 60 60" is retried as "M0,010 20 L 60 60": 20 L is an
        // implicit lineto whose second parameter is the command L.
        assert_eq!(
            g.path("10 20 L 60 60", &o),
            Err(PathError::ParamNotANumber {
                mode: 'L',
                text: "L".into()
            })
        );
        assert_eq!(g.path("M 0 0 L 10", &o), Err(PathError::EndedShort));
        assert_eq!(g.path("M 0 0 x 5", &o), Err(PathError::InvalidData));
        // rough.js loops forever here (Z takes no parameters, so the number
        // is never consumed); the port reports it.
        assert_eq!(
            g.path("M 0 0 L 5 5 Z 5", &o),
            Err(PathError::ParamAfterClose)
        );
        assert!(!PathError::EndedShort.to_string().is_empty());
    }

    // rough.js 4.6.4 throws `RangeError: Maximum call stack size exceeded`
    // on all of these (checked against the pinned package bundled with
    // esbuild): the Bezier flattening never reaches its tolerance once a
    // coordinate is, or overflows to, a non-finite value, and pointsOnPath
    // runs whether or not the path is simplified.
    const NEVER_FLAT: [&str; 2] = ["M0 0 C 1e400 0 0 0 1 1", "M0 0 C 1e308 0 -1e308 0 1 1"];

    #[test]
    fn path_whose_curve_never_flattens_is_a_call_stack_error() {
        let g = RoughGenerator::new();
        for d in NEVER_FLAT {
            for simplification in [None, Some(0.5)] {
                let o = Options {
                    seed: 1,
                    simplification,
                    ..Options::default()
                };
                assert_eq!(
                    g.path(d, &o),
                    Err(PathError::CallStackExceeded),
                    "{d} simplification {simplification:?}"
                );
            }
        }
        assert_eq!(
            PathError::CallStackExceeded.to_string(),
            "Maximum call stack size exceeded"
        );
    }

    #[test]
    fn path_with_roughness_below_minus_one_is_a_call_stack_error() {
        // Unsimplified, rough.js flattens the path to (1 + roughness) / 2;
        // below -1 that is a negative epsilon and simplify never returns.
        let g = RoughGenerator::new();
        for d in ["M0 0 L 10 10", "M0 0"] {
            let o = Options {
                seed: 1,
                roughness: -3.0,
                ..Options::default()
            };
            assert_eq!(g.path(d, &o), Err(PathError::CallStackExceeded), "{d}");
        }
        let o = Options {
            seed: 1,
            roughness: -1.0,
            ..Options::default()
        };
        assert!(g.path("M0 0 L 10 10", &o).is_ok());
    }

    #[test]
    fn empty_path_draws_nothing() {
        let d = RoughGenerator::new().path("", &seeded(1)).unwrap();
        assert!(d.sets.is_empty());
    }
}

mod path_data {
    use super::*;

    #[test]
    fn parse_repeats_commands_and_turns_moves_into_lines() {
        assert_eq!(
            parse_path("M 0 0 1 1 l 2 2 3 3 m 1 1 2 2").unwrap(),
            vec![
                seg('M', &[0.0, 0.0]),
                seg('L', &[1.0, 1.0]),
                seg('l', &[2.0, 2.0]),
                seg('l', &[3.0, 3.0]),
                seg('m', &[1.0, 1.0]),
                seg('l', &[2.0, 2.0]),
            ]
        );
    }

    #[test]
    fn parse_prepends_a_move_when_the_path_does_not_start_with_one() {
        assert_eq!(
            parse_path("L 5 5").unwrap(),
            vec![seg('M', &[0.0, 0.0]), seg('L', &[5.0, 5.0])]
        );
    }

    #[test]
    fn parse_reads_compact_numbers() {
        assert_eq!(
            parse_path("M10-20l.5.5-3e1,4E+1L+6 1.").unwrap(),
            vec![
                seg('M', &[10.0, -20.0]),
                seg('l', &[0.5, 0.5]),
                seg('l', &[-30.0, 40.0]),
                seg('L', &[6.0, 1.0]),
            ]
        );
    }

    #[test]
    fn parse_loses_the_sign_of_zero_like_the_js_round_trip() {
        // tokens hold `${parseFloat(text)}`, and String(-0) is "0"
        let s = parse_path("M -0 -0.0").unwrap();
        assert!(s[0].data.iter().all(|v| *v == 0.0 && v.is_sign_positive()));
    }

    #[test]
    fn absolutize_resolves_relative_commands() {
        let abs = absolutize(
            &parse_path("M 10 10 l 5 5 h 5 v -5 c 1 1 2 2 3 3 z m 1 1 a 1 1 0 0 1 2 2 t 1 1")
                .unwrap(),
        );
        assert_eq!(
            abs,
            vec![
                seg('M', &[10.0, 10.0]),
                seg('L', &[15.0, 15.0]),
                seg('H', &[20.0]),
                seg('V', &[10.0]),
                seg('C', &[21.0, 11.0, 22.0, 12.0, 23.0, 13.0]),
                seg('Z', &[]),
                seg('M', &[11.0, 11.0]),
                seg('A', &[1.0, 1.0, 0.0, 0.0, 1.0, 13.0, 13.0]),
                seg('T', &[14.0, 14.0]),
            ]
        );
    }

    #[test]
    fn normalize_keeps_only_m_l_c_z() {
        let n = normalize(&absolutize(
            &parse_path("M 0 0 H 3 V 3 Q 6 6 9 3 T 12 3 S 15 0 18 3 A 0 0 0 0 1 20 20 Z").unwrap(),
        ));
        assert!(n.iter().all(|s| matches!(s.key, 'M' | 'L' | 'C' | 'Z')));
        assert_eq!(n[1], seg('L', &[3.0, 0.0]));
        assert_eq!(n[2], seg('L', &[3.0, 3.0]));
        // Q -> C: control points at 2/3 towards the quadratic control point
        assert_eq!(
            n[3],
            seg(
                'C',
                &[
                    3.0 + 2.0 * 3.0 / 3.0,
                    3.0 + 2.0 * 3.0 / 3.0,
                    9.0 + 2.0 * (6.0 - 9.0) / 3.0,
                    3.0 + 2.0 * 3.0 / 3.0,
                    9.0,
                    3.0
                ]
            )
        );
        // zero radius arc -> a straight cubic
        assert_eq!(n[6], seg('C', &[18.0, 3.0, 20.0, 20.0, 20.0, 20.0]));
        assert_eq!(n[7], seg('Z', &[]));
    }

    #[test]
    fn arc_to_the_current_point_is_dropped() {
        let n = normalize(&parse_path("M 5 5 A 3 3 0 0 1 5 5").unwrap());
        assert_eq!(n, vec![seg('M', &[5.0, 5.0])]);
    }
}

mod points {
    use super::*;

    #[test]
    fn curve_to_bezier_matches_the_catmull_rom_conversion() {
        let b = curve_to_bezier(&[[0.0, 0.0], [10.0, 10.0], [20.0, 0.0]], 0.0).unwrap();
        assert_eq!(b, vec![[0.0, 0.0], [10.0, 10.0], [20.0, 0.0], [20.0, 0.0]]);
        let b = curve_to_bezier(&[[0.0, 0.0], [6.0, 6.0], [12.0, 0.0], [18.0, 6.0]], 0.0).unwrap();
        assert_eq!(b.len(), 10);
        assert_eq!(b[0], [0.0, 0.0]);
        assert_eq!(b[1], [1.0, 1.0]);
        assert!(curve_to_bezier(&[[0.0, 0.0], [1.0, 1.0]], 0.0).is_err());
    }

    #[test]
    fn flat_bezier_is_its_end_points() {
        let pts = points_on_bezier_curves(
            &[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]],
            0.15,
            None,
        )
        .unwrap();
        assert_eq!(pts, vec![[0.0, 0.0], [3.0, 0.0]]);
    }

    #[test]
    fn bezier_that_never_flattens_is_a_call_stack_error() {
        // rough.js throws RangeError on each: a non-finite point, a
        // tolerance of 0 on a straight segment (0 < 0 never holds), a
        // negative or NaN tolerance.
        let curvy = [[0.0, 0.0], [1.0, 1.0], [2.0, 0.0], [3.0, 0.0]];
        let flat = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
        let cases: [(&[[f64; 2]], f64); 6] = [
            (
                &[[0.0, 0.0], [f64::INFINITY, 0.0], [0.0, 0.0], [1.0, 1.0]],
                0.15,
            ),
            (&[[0.0, 0.0], [f64::NAN, 0.0], [0.0, 0.0], [1.0, 1.0]], 0.15),
            (&[[0.0, 0.0], [1e308, 0.0], [-1e308, 0.0], [1.0, 1.0]], 0.15),
            (&flat, 0.0),
            (&curvy, -1.0),
            (&curvy, f64::NAN),
        ];
        for (points, tolerance) in cases {
            for distance in [None, Some(0.5)] {
                assert_eq!(
                    points_on_bezier_curves(points, tolerance, distance),
                    Err(PathError::CallStackExceeded),
                    "{points:?} tolerance {tolerance} distance {distance:?}"
                );
            }
        }
        // a small tolerance still converges
        assert_eq!(
            points_on_bezier_curves(&curvy, 1e-12, None).unwrap().len(),
            2190
        );
    }

    #[test]
    fn points_on_path_on_a_curve_that_never_flattens_is_a_call_stack_error() {
        for distance in [0.0, 0.5] {
            assert_eq!(
                points_on_path("M0 0 C 1e400 0 0 0 1 1", 1.0, distance),
                Err(PathError::CallStackExceeded)
            );
        }
        assert_eq!(
            points_on_path("M0 0 L 1 1", 1.0, -1.0),
            Err(PathError::CallStackExceeded)
        );
    }

    #[test]
    fn simplify_with_a_negative_epsilon_is_a_call_stack_error() {
        // simplifyPoints recurses on every range when sqrt(0) > epsilon.
        for pts in [
            &[][..],
            &[[0.0, 0.0]][..],
            &[[0.0, 0.0], [1.0, 1.0], [2.0, 0.0]][..],
        ] {
            assert_eq!(simplify(pts, -1.0), Err(PathError::CallStackExceeded));
            let mut out = Vec::new();
            assert_eq!(
                simplify_points(pts, 0, pts.len(), -1.0, &mut out),
                Err(PathError::CallStackExceeded)
            );
        }
        assert_eq!(
            simplify(&[[0.0, 0.0], [1.0, 1.0]], f64::NAN),
            Ok(vec![[0.0, 0.0], [1.0, 1.0]])
        );
    }

    #[test]
    fn simplify_does_not_overflow_the_stack_on_deep_splits() {
        // Each split peels one point off, so the recursion is as deep as the
        // polyline is long. rough.js throws RangeError past the engine's
        // stack (Node 26: at 5,000 of these points); the port has no depth
        // limit and returns the simplified polyline (a documented
        // divergence, see points_on_curve::simplify_points).
        let n = 20_000;
        let pts: Vec<[f64; 2]> = (0..n)
            .map(|i| {
                let sign = if i % 2 == 1 { -1.0 } else { 1.0 };
                [i as f64, sign * (n - i) as f64]
            })
            .collect();
        let out = simplify(&pts, 0.5).unwrap();
        assert_eq!(out, pts);
    }

    #[test]
    fn simplify_is_ramer_douglas_peucker() {
        let pts = [
            [0.0, 0.0],
            [1.0, 0.1],
            [2.0, -0.1],
            [3.0, 5.0],
            [4.0, 6.0],
            [5.0, 7.0],
        ];
        assert_eq!(
            simplify(&pts, 0.5).unwrap(),
            vec![[0.0, 0.0], [2.0, -0.1], [3.0, 5.0], [5.0, 7.0]]
        );
    }

    #[test]
    fn points_on_path_splits_subpaths_and_closes() {
        let sets = points_on_path("M 0 0 L 10 0 L 10 10 Z M 20 20 L 30 30", 1.0, 0.0).unwrap();
        assert_eq!(
            sets,
            vec![
                vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 0.0]],
                vec![[20.0, 20.0], [30.0, 30.0]],
            ]
        );
    }
}
