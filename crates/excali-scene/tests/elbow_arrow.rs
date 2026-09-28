//! Elbow arrow bodies from their fixed points (ex-210): the rounded path of
//! `generateElbowArrowShape(points, 16)` (`packages/element/src/shape.ts:
//! 1018-1081`) that `_generateElementShape` draws with `generator.path` as a
//! continuous path, skipping arrows with a coordinate beyond 1e6
//! (`shape.ts:900-920`); the headings it turns on (`headingForPoint*`,
//! `packages/element/src/heading.ts:37-67`); and `validateElbowPoints`
//! (`packages/element/src/elbowArrow.ts:2293-2304`, `DEDUP_TRESHOLD` at
//! `:110`), which rejects a segment that is neither horizontal nor
//! vertical. `site/content/research/rendering.md` sections 2 and 5.
//!
//! The expected path strings are worked by hand from upstream's loop; the
//! drawables of `goldens/elements-elbow-arrow.json` are compared op by op in
//! `tests/goldens.rs`.

use excali_core::element::{
    ArrowFields, Element, ElementBase, ElementKind, ElementType, LineFields, LinearFields,
    LocalPoint,
};
use excali_rough::RoughGenerator;
use excali_scene::elbow_arrow::{
    elbow_arrow_path, validate_elbow_points, validate_elbow_points_with_tolerance, DEDUP_TRESHOLD,
    ELBOW_ARROW_CORNER_RADIUS, ELBOW_ARROW_MAX_COORDINATE,
};
use excali_scene::heading::{
    heading_for_point, heading_for_point_is_horizontal, heading_is_horizontal, heading_is_vertical,
    vector_to_heading, Heading,
};
use excali_scene::rough_options::generate_rough_options;
use excali_scene::shape::{generate_elbow_arrow_shape, RenderConfig, ShapeError, Theme};

// ---------------------------------------------------------------------------
// Constants

#[test]
fn constants_match_upstream() {
    // shape.ts:917 `generateElbowArrowShape(points, 16)`
    assert_eq!(ELBOW_ARROW_CORNER_RADIUS, 16.0);
    // shape.ts:904 `Math.abs(point[0]) <= 1e6 && Math.abs(point[1]) <= 1e6`
    assert_eq!(ELBOW_ARROW_MAX_COORDINATE, 1e6);
    // elbowArrow.ts:110
    assert_eq!(DEDUP_TRESHOLD, 1.0);
}

// ---------------------------------------------------------------------------
// Headings (heading.ts)

#[test]
fn vector_to_heading_follows_upstream_comparisons() {
    // x > |y| → right; x <= -|y| → left; y > |x| → down; else up
    assert_eq!(vector_to_heading([5.0, 0.0]), Heading::Right);
    assert_eq!(vector_to_heading([5.0, 4.9]), Heading::Right);
    assert_eq!(vector_to_heading([-5.0, 0.0]), Heading::Left);
    assert_eq!(vector_to_heading([0.0, 5.0]), Heading::Down);
    assert_eq!(vector_to_heading([0.0, -5.0]), Heading::Up);
    // the ties: the zero vector and x = -|y| are left, the other diagonals
    // fall through to up
    assert_eq!(vector_to_heading([0.0, 0.0]), Heading::Left);
    assert_eq!(vector_to_heading([-3.0, 3.0]), Heading::Left);
    assert_eq!(vector_to_heading([-3.0, -3.0]), Heading::Left);
    assert_eq!(vector_to_heading([3.0, 3.0]), Heading::Up);
    assert_eq!(vector_to_heading([3.0, -3.0]), Heading::Up);
    // NaN fails every comparison
    assert_eq!(vector_to_heading([f64::NAN, 1.0]), Heading::Up);
}

#[test]
fn heading_vectors_are_upstreams() {
    // HEADING_RIGHT = [1, 0], DOWN = [0, 1], LEFT = [-1, 0], UP = [0, -1]
    assert_eq!(Heading::Right.vector(), [1.0, 0.0]);
    assert_eq!(Heading::Down.vector(), [0.0, 1.0]);
    assert_eq!(Heading::Left.vector(), [-1.0, 0.0]);
    assert_eq!(Heading::Up.vector(), [0.0, -1.0]);
}

#[test]
fn heading_for_point_is_the_heading_from_o_to_p() {
    // vectorFromPoint(p, o) = p - o
    assert_eq!(heading_for_point([10.0, 0.0], [0.0, 0.0]), Heading::Right);
    assert_eq!(heading_for_point([0.0, 0.0], [10.0, 0.0]), Heading::Left);
    assert_eq!(heading_for_point([3.0, 10.0], [3.0, 0.0]), Heading::Down);
    assert_eq!(heading_for_point([3.0, 0.0], [3.0, 10.0]), Heading::Up);
    assert!(heading_for_point_is_horizontal([10.0, 0.0], [0.0, 0.0]));
    assert!(heading_for_point_is_horizontal([0.0, 0.0], [10.0, 0.0]));
    assert!(!heading_for_point_is_horizontal([0.0, 10.0], [0.0, 0.0]));
    // coincident points head left, so horizontally
    assert!(heading_for_point_is_horizontal([4.0, 4.0], [4.0, 4.0]));
    for h in [Heading::Right, Heading::Left] {
        assert!(heading_is_horizontal(h) && !heading_is_vertical(h));
    }
    for h in [Heading::Up, Heading::Down] {
        assert!(!heading_is_horizontal(h) && heading_is_vertical(h));
    }
}

// ---------------------------------------------------------------------------
// validateElbowPoints

#[test]
fn axis_aligned_points_are_valid() {
    assert!(validate_elbow_points(&[
        [0.0, 0.0],
        [50.0, 0.0],
        [50.0, 50.0],
        [100.0, 50.0]
    ]));
    assert!(validate_elbow_points(&[
        [0.0, 0.0],
        [0.0, -60.0],
        [180.0, -60.0]
    ]));
    // a zero-length segment is aligned both ways
    assert!(validate_elbow_points(&[[5.0, 5.0], [5.0, 5.0]]));
    // no segments at all: `[].every` is true
    assert!(validate_elbow_points(&[]));
    assert!(validate_elbow_points(&[[3.0, 4.0]]));
}

#[test]
fn non_axis_aligned_segments_are_rejected() {
    // the restore test's invalid arrow (restore.ts:1078)
    assert!(!validate_elbow_points(&[
        [0.0, 0.0],
        [60.0, 40.0],
        [100.0, 50.0]
    ]));
    // one diagonal segment anywhere is enough
    assert!(!validate_elbow_points(&[
        [0.0, 0.0],
        [50.0, 0.0],
        [50.0, 50.0],
        [52.0, 60.0]
    ]));
    assert!(!validate_elbow_points(&[[0.0, 0.0], [1.0, 1.0]]));
}

#[test]
fn tolerance_is_strict() {
    // |dx| < 1 || |dy| < 1 with the default DEDUP_TRESHOLD
    assert!(validate_elbow_points(&[[0.0, 0.0], [0.999, 40.0]]));
    assert!(validate_elbow_points(&[[0.0, 0.0], [40.0, -0.999]]));
    assert!(!validate_elbow_points(&[[0.0, 0.0], [1.0, 40.0]]));
    assert!(!validate_elbow_points(&[[0.0, 0.0], [40.0, 1.0]]));
    // a caller's tolerance
    assert!(validate_elbow_points_with_tolerance(
        &[[0.0, 0.0], [4.0, 40.0]],
        5.0
    ));
    assert!(!validate_elbow_points_with_tolerance(
        &[[0.0, 0.0], [0.5, 40.0]],
        0.5
    ));
    assert!(!validate_elbow_points_with_tolerance(
        &[[0.0, 0.0], [0.0, 0.0]],
        0.0
    ));
}

#[test]
fn nan_coordinates_are_rejected() {
    // NaN < tolerance is false
    assert!(!validate_elbow_points(&[[0.0, 0.0], [f64::NAN, f64::NAN]]));
    assert!(validate_elbow_points(&[[0.0, 0.0], [f64::NAN, 0.0]]));
}

// ---------------------------------------------------------------------------
// generateElbowArrowShape

fn path(points: &[LocalPoint]) -> String {
    elbow_arrow_path(points, ELBOW_ARROW_CORNER_RADIUS)
}

#[test]
fn straight_arrow_is_a_single_line() {
    assert_eq!(path(&[[0.0, 0.0], [200.0, 0.0]]), "M 0 0 L 200 0");
    // one point: the move and a line back to it
    assert_eq!(path(&[[0.0, 0.0]]), "M 0 0 L 0 0");
}

#[test]
fn each_corner_is_a_quadratic_of_radius_16() {
    // fixtures.mjs ELBOW (the rough-primitives path/elbow case)
    assert_eq!(
        path(&[[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [200.0, 100.0]]),
        "M 0 0 L 84 0 Q 100 0, 100 16 L 100 84 Q 100 100, 116 100 L 200 100"
    );
    // the golden cases
    assert_eq!(
        path(&[[0.0, 0.0], [160.0, 0.0], [160.0, 120.0]]),
        "M 0 0 L 144 0 Q 160 0, 160 16 L 160 120"
    );
    assert_eq!(
        path(&[[0.0, 0.0], [100.0, 0.0], [100.0, 80.0], [220.0, 80.0]]),
        "M 0 0 L 84 0 Q 100 0, 100 16 L 100 64 Q 100 80, 116 80 L 220 80"
    );
    assert_eq!(
        path(&[[0.0, 0.0], [0.0, -60.0], [180.0, -60.0], [180.0, 20.0]]),
        "M 0 0 L 0 -44 Q 0 -60, 16 -60 L 164 -60 Q 180 -60, 180 -44 L 180 20"
    );
}

#[test]
fn every_direction_offsets_the_right_way() {
    // leftwards then up, into the corner from the right and from below
    assert_eq!(
        path(&[[200.0, 100.0], [100.0, 100.0], [100.0, 0.0], [0.0, 0.0]]),
        "M 200 100 L 116 100 Q 100 100, 100 84 L 100 16 Q 100 0, 84 0 L 0 0"
    );
    // down then left
    assert_eq!(
        path(&[[0.0, 0.0], [0.0, 100.0], [-100.0, 100.0]]),
        "M 0 0 L 0 84 Q 0 100, -16 100 L -100 100"
    );
}

#[test]
fn short_segments_shrink_the_corner_to_half_their_length() {
    // corner = min(16, |point - next| / 2, |point - prev| / 2)
    assert_eq!(
        path(&[
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 12.0],
            [60.0, 12.0],
            [60.0, 40.0]
        ]),
        "M 0 0 L 14 0 Q 20 0, 20 6 L 20 6 Q 20 12, 26 12 L 46 12 Q 60 12, 60 26 L 60 40"
    );
    // fractions are written as JavaScript writes them
    assert_eq!(
        path(&[[0.0, 0.0], [10.5, 0.0], [10.5, 7.0]]),
        "M 0 0 L 7 0 Q 10.5 0, 10.5 3.5 L 10.5 7"
    );
    assert_eq!(
        path(&[[0.0, 0.0], [0.1, 0.0], [0.1, 0.2]]),
        "M 0 0 L 0.05 0 Q 0.1 0, 0.1 0.05 L 0.1 0.2"
    );
}

#[test]
fn radius_is_a_parameter() {
    assert_eq!(
        elbow_arrow_path(&[[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]], 4.0),
        "M 0 0 L 96 0 Q 100 0, 100 4 L 100 100"
    );
    assert_eq!(
        elbow_arrow_path(&[[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]], 0.0),
        "M 0 0 L 100 0 Q 100 0, 100 0 L 100 100"
    );
}

#[test]
fn coincident_points_make_a_zero_corner() {
    // a repeated point heads "left" (horizontal) with a zero corner
    assert_eq!(
        path(&[[0.0, 0.0], [50.0, 0.0], [50.0, 0.0], [50.0, 50.0]]),
        "M 0 0 L 50 0 Q 50 0, 50 0 L 50 0 Q 50 0, 50 0 L 50 50"
    );
}

#[test]
fn negative_zero_is_written_as_zero() {
    // `${-0}` is "0"
    assert_eq!(path(&[[-0.0, -0.0], [100.0, -0.0]]), "M 0 0 L 100 0");
}

// ---------------------------------------------------------------------------
// The drawable (shape.ts:900-920)

fn element(kind: ElementKind) -> Element {
    let mut base = ElementBase::new("el", 0.0, 0.0, 1041657908.0, 1.0);
    base.width = 160.0;
    base.height = 120.0;
    Element::new(base, kind)
}

fn elbow(points: &[LocalPoint]) -> Element {
    element(ElementKind::Arrow(ArrowFields::new(
        LinearFields::new(points.to_vec()),
        true,
    )))
}

const L: [LocalPoint; 3] = [[0.0, 0.0], [160.0, 0.0], [160.0, 120.0]];

fn build(
    el: &Element,
    config: &RenderConfig<'_>,
) -> Result<Option<excali_rough::Drawable>, ShapeError> {
    generate_elbow_arrow_shape(el, &RoughGenerator::new(), config)
}

#[test]
fn body_is_the_rounded_path_with_continuous_options() {
    let generator = RoughGenerator::new();
    for roughness in [0.0, 1.0, 2.0] {
        let mut el = elbow(&L);
        el.base.roughness = roughness;
        let got = build(&el, &RenderConfig::default()).unwrap().unwrap();
        // generateRoughOptions(element, true, isDarkMode): preserveVertices
        // even at roughness 2
        let o = generate_rough_options(&el, true, false)
            .unwrap()
            .to_rough(generator.default_options());
        assert!(o.preserve_vertices);
        let want = generator
            .path("M 0 0 L 144 0 Q 160 0, 160 16 L 160 120", &o)
            .unwrap();
        assert_eq!(got, want, "roughness {roughness}");
        assert_eq!(got.shape.as_str(), "path");
    }
}

#[test]
fn dark_mode_inverts_the_stroke() {
    let el = elbow(&L);
    let dark = RenderConfig {
        theme: Theme::Dark,
        ..RenderConfig::default()
    };
    let light = build(&el, &RenderConfig::default()).unwrap().unwrap();
    let dark = build(&el, &dark).unwrap().unwrap();
    assert_eq!(light.options.stroke, "#1e1e1e");
    assert_ne!(dark.options.stroke, light.options.stroke);
    assert_eq!(dark.sets, light.sets);
}

#[test]
fn empty_points_draw_the_origin() {
    let generator = RoughGenerator::new();
    let el = elbow(&[]);
    let got = build(&el, &RenderConfig::default()).unwrap().unwrap();
    let o = generate_rough_options(&el, true, false)
        .unwrap()
        .to_rough(generator.default_options());
    assert_eq!(got, generator.path("M 0 0 L 0 0", &o).unwrap());
}

#[test]
fn extreme_coordinates_are_not_drawn() {
    // |coordinate| <= 1e6 is drawn, anything beyond is not (shape = [])
    let edge = elbow(&[[0.0, 0.0], [1e6, 0.0], [1e6, -1e6]]);
    assert!(build(&edge, &RenderConfig::default()).unwrap().is_some());
    for points in [
        vec![[0.0, 0.0], [2_000_000.0, 0.0]],
        vec![[0.0, 0.0], [0.0, -1_000_000.5]],
        vec![[-1e7, 0.0], [0.0, 0.0]],
        vec![[0.0, 0.0], [f64::NAN, 0.0]],
        vec![[0.0, 0.0], [f64::INFINITY, 0.0]],
    ] {
        assert_eq!(
            build(&elbow(&points), &RenderConfig::default()),
            Ok(None),
            "{points:?}"
        );
    }
}

#[test]
fn only_elbow_arrows_are_built_here() {
    let plain = element(ElementKind::Arrow(ArrowFields::new(
        LinearFields::new(L.to_vec()),
        false,
    )));
    assert_eq!(
        build(&plain, &RenderConfig::default()),
        Err(ShapeError::NotAnElbowArrow(ElementType::Arrow))
    );
    let line = element(ElementKind::Line(LineFields {
        linear: LinearFields::new(L.to_vec()),
        polygon: false,
    }));
    assert_eq!(
        build(&line, &RenderConfig::default()),
        Err(ShapeError::NotAnElbowArrow(ElementType::Line))
    );
    assert_eq!(
        build(&element(ElementKind::Rectangle), &RenderConfig::default()),
        Err(ShapeError::NotAnElbowArrow(ElementType::Rectangle))
    );
}
