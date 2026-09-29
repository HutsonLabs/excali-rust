//! Parity with the vendored `@excalidraw/laser-pointer` 1.3.1
//! (`packages/laser-pointer/src` at the pinned commit) and with upstream's
//! `getConstantWidthFreedrawOutline` (`packages/element/src/shape.ts:1247-1268`).
//!
//! `goldens/laser-pointer.json` is written by `tools/goldens/generate.mjs`
//! from the checkout's own sources: each case builds a `LaserPointer` with
//! the options (a named `sizeMapping`, see `SIZE_MAPPINGS` in
//! `tools/goldens/fixtures.mjs`), adds the points, optionally closes it and
//! resets `keepHead`, and records `originalPoints` and
//! `getStrokeOutline(sizeOverride)`, or the error a call threw.
//!
//! `originalPoints` involve no floating-point operation and are compared
//! with `==`. So are outlines, which go through `Math.sin`, `Math.cos` and
//! `Math.atan2`: `excali_math::js` computes them as V8 does, on every
//! platform (ex-009).

use std::path::Path;
use std::sync::Arc;

use excali_freehand::{
    constant_width_options, constant_width_outline, douglas_peucker, LaserPoint, LaserPointer,
    LaserPointerError, LaserPointerOptions, SimplifyPhase, SizeMapping, SizeMappingDetails,
    CONSTANT_WIDTH_COLLISION_SIMPLIFY_TOLERANCE, CONSTANT_WIDTH_SIZE_FACTOR,
    CORNER_DETECTION_MAX_ANGLE, MAX_TAIL_LENGTH,
};
use serde_json::Value;

fn load(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../goldens")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (run node tools/goldens/generate.mjs)",
            path.display()
        )
    });
    let doc: Value = serde_json::from_str(&text).expect("golden parses");
    doc["cases"].as_array().expect("cases").clone()
}

fn f(v: &Value) -> f64 {
    v.as_f64()
        .unwrap_or_else(|| panic!("number expected, got {v}"))
}

fn point3(v: &Value) -> LaserPoint {
    let a = v.as_array().expect("point");
    assert_eq!(a.len(), 3, "laser points are [x, y, r]");
    [f(&a[0]), f(&a[1]), f(&a[2])]
}

fn points3(v: &Value) -> Vec<LaserPoint> {
    v.as_array().expect("points").iter().map(point3).collect()
}

/// `easeOut` (`packages/common/src/utils.ts:232-234`).
fn ease_out(k: f64) -> f64 {
    1.0 - excali_math::js::pow(1.0 - k, 4.0)
}

/// `SIZE_MAPPINGS` in `tools/goldens/fixtures.mjs`.
fn size_mapping(name: &str) -> SizeMapping {
    match name {
        "one" => Arc::new(|_: &SizeMappingDetails| 1.0),
        "constantWidth" => Arc::new(|d: &SizeMappingDetails| excali_math::js::max(0.1, d.pressure)),
        "pressure" => Arc::new(|d: &SizeMappingDetails| d.pressure),
        "tiny" => Arc::new(|_: &SizeMappingDetails| 0.1),
        "zero" => Arc::new(|_: &SizeMappingDetails| 0.0),
        "zeroBefore3" => {
            Arc::new(|d: &SizeMappingDetails| if d.current_index < 3 { 0.0 } else { 1.0 })
        }
        "firstIndexTiny" => {
            Arc::new(|d: &SizeMappingDetails| if d.current_index == 0 { 0.01 } else { 1.0 })
        }
        // laserTrails.ts:28-40 at a clock of 1000 ms
        "trail" => Arc::new(|d: &SizeMappingDetails| {
            let t = excali_math::js::max(0.0, 1.0 - (1000.0 - d.pressure) / 1000.0);
            let remaining = d.total_length as f64 - d.current_index as f64;
            let l = (50.0 - excali_math::js::min(50.0, remaining)) / 50.0;
            excali_math::js::min(ease_out(l), ease_out(t))
        }),
        other => panic!("unknown sizeMapping {other}"),
    }
}

/// `Object.assign({}, LaserPointer.defaults, options)`.
fn options(v: &Value) -> LaserPointerOptions {
    let mut o = LaserPointerOptions::default();
    for (k, v) in v.as_object().expect("options") {
        match k.as_str() {
            "size" => o.size = f(v),
            "streamline" => o.streamline = f(v),
            "simplify" => o.simplify = f(v),
            "simplifyPhase" => {
                o.simplify_phase = match v.as_str().expect("phase") {
                    "tail" => SimplifyPhase::Tail,
                    "output" => SimplifyPhase::Output,
                    "input" => SimplifyPhase::Input,
                    other => panic!("unknown simplifyPhase {other}"),
                }
            }
            "keepHead" => o.keep_head = v.as_bool().expect("keepHead"),
            "sizeMapping" => o.size_mapping = size_mapping(v.as_str().expect("name")),
            other => panic!("unknown option {other}"),
        }
    }
    o
}

/// Outline coordinates are compared exactly (`-0 == 0`).
fn close(a: f64, b: f64) -> bool {
    a == b
}

fn check_outline<const N: usize>(id: &str, got: &[[f64; N]], want: &[[f64; N]]) {
    assert_eq!(got.len(), want.len(), "{id}: outline length");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(
            g.iter().zip(w).all(|(&g, &w)| close(g, w)),
            "{id}: outline[{i}] = {g:?}, upstream {w:?}"
        );
    }
}

#[test]
fn laser_pointer_goldens() {
    let cases = load("laser-pointer.json");
    assert!(
        cases.len() >= 50,
        "laser-pointer.json has {} cases",
        cases.len()
    );
    let (mut outlines, mut errors) = (0, 0);
    for c in &cases {
        let id = c["id"].as_str().expect("id");
        let mut laser = LaserPointer::new(options(&c["options"]));
        let points = points3(&c["points"]);
        let mut error = None;
        for (i, &p) in points.iter().enumerate() {
            if let Err(e) = laser.add_point(p) {
                error = Some((Value::from(i), e));
                break;
            }
        }
        if error.is_none() && c.get("close").and_then(Value::as_bool) == Some(true) {
            if let Err(e) = laser.close() {
                error = Some((Value::from("close"), e));
            }
        }
        assert_eq!(
            laser.original_points(),
            points3(&c["originalPoints"]).as_slice(),
            "{id}: originalPoints"
        );
        match (error, c.get("error")) {
            (Some((at, e)), Some(want)) => {
                assert_eq!(at, want["at"], "{id}: error at");
                assert_eq!(
                    e.to_string(),
                    want["message"].as_str().expect("message"),
                    "{id}"
                );
                assert_eq!(e, LaserPointerError::NotImplemented, "{id}");
                errors += 1;
            }
            (None, None) => {
                if let Some(keep_head) = c.get("keepHeadAfter") {
                    laser.options.keep_head = keep_head.as_bool().expect("keepHeadAfter");
                }
                let size_override = c.get("sizeOverride").map(f);
                let outline = laser.get_stroke_outline(size_override);
                check_outline(id, &outline, &points3(&c["outline"]));
                outlines += 1;
            }
            (got, want) => panic!(
                "{id}: port error {:?}, upstream error {want:?}",
                got.map(|(at, e)| (at, e.to_string()))
            ),
        }
    }
    assert!(
        outlines >= 50 && errors == 2,
        "{outlines} outlines, {errors} errors"
    );
}

/// `LaserPointer.defaults` and `LaserPointer.constants` (`state.ts:29-43`).
#[test]
fn defaults_and_constants_are_upstreams() {
    let o = LaserPointerOptions::default();
    assert_eq!(o.size, 2.0);
    assert_eq!(o.streamline, 0.45);
    assert_eq!(o.simplify, 0.1);
    assert_eq!(o.simplify_phase, SimplifyPhase::Output);
    assert!(!o.keep_head);
    let d = SizeMappingDetails {
        pressure: 0.3,
        running_length: 12.0,
        current_index: 2,
        total_length: 9,
    };
    assert_eq!((o.size_mapping)(&d), 1.0);
    assert_eq!(CORNER_DETECTION_MAX_ANGLE, 75.0);
    assert_eq!(MAX_TAIL_LENGTH, 50.0);
    assert_eq!(LaserPointer::corner_detection_variance(35.0), 1.0);
    assert_eq!(LaserPointer::corner_detection_variance(35.5), 0.5);
}

/// `CONSTANT_WIDTH_FREEDRAW` and `createLaserPointer` (`shape.ts:1207-1253`):
/// size `strokeWidth * 1.4`, streamline from `strokeOptions` (default 0.5),
/// simplify 0, `sizeMapping` `max(0.1, pressure)`.
#[test]
fn constant_width_options_are_upstreams() {
    assert_eq!(CONSTANT_WIDTH_SIZE_FACTOR, 1.4);
    assert_eq!(CONSTANT_WIDTH_COLLISION_SIMPLIFY_TOLERANCE, 0.2);
    let o = constant_width_options(4.0, None);
    assert_eq!(o.size, 4.0 * 1.4);
    assert_eq!(o.streamline, 0.5);
    assert_eq!(o.simplify, 0.0);
    assert_eq!(o.simplify_phase, SimplifyPhase::Output);
    assert!(!o.keep_head);
    assert_eq!(constant_width_options(2.0, Some(0.2)).streamline, 0.2);
    let at = |pressure| {
        (o.size_mapping)(&SizeMappingDetails {
            pressure,
            running_length: 0.0,
            current_index: 0,
            total_length: 3,
        })
    };
    assert_eq!(at(1.0), 1.0);
    assert_eq!(at(0.5), 0.5);
    assert_eq!(at(0.01), 0.1);
    assert_eq!(at(0.0), 0.1);
}

/// The `excalidraw/` cases in laser-pointer.json use exactly the options
/// `constant_width_options` builds, and `constant_width_outline` is their
/// outline without the third coordinate.
#[test]
fn excalidraw_cases_are_constant_width_outlines() {
    let mut seen = 0;
    for c in load("laser-pointer.json") {
        let id = c["id"].as_str().expect("id");
        if !id.starts_with("excalidraw/") {
            continue;
        }
        let o = &c["options"];
        assert_eq!(o["sizeMapping"], "constantWidth", "{id}");
        assert_eq!(f(&o["simplify"]), 0.0, "{id}");
        let stroke_width = f(&o["size"]) / 1.4;
        let streamline = f(&o["streamline"]);
        assert_eq!(
            constant_width_options(stroke_width, Some(streamline)).size,
            f(&o["size"]),
            "{id}"
        );
        let points: Vec<[f64; 2]> = points3(&c["points"])
            .iter()
            .map(|&[x, y, r]| {
                assert_eq!(r, 1.0, "{id}: pressure 1");
                [x, y]
            })
            .collect();
        let want: Vec<[f64; 2]> = points3(&c["outline"])
            .iter()
            .map(|&[x, y, _]| [x, y])
            .collect();
        check_outline(
            id,
            &constant_width_outline(&points, stroke_width, Some(streamline)),
            &want,
        );
        seen += 1;
    }
    assert!(seen >= 15, "{seen} excalidraw cases");
}

/// `douglasPeucker` (`simplify.ts`): epsilon 0 and two points are returned
/// as is; otherwise the farthest point splits the polyline while it is at
/// least epsilon away.
#[test]
fn douglas_peucker_is_upstreams() {
    let line: Vec<LaserPoint> = vec![[0.0, 0.0, 1.0], [5.0, 0.5, 2.0], [10.0, 0.0, 3.0]];
    assert_eq!(douglas_peucker(&line, 0.0), line);
    assert_eq!(douglas_peucker(&line[..2], 5.0), line[..2].to_vec());
    assert_eq!(douglas_peucker(&line, 0.6), vec![line[0], line[2]]);
    assert_eq!(
        douglas_peucker(&line, 0.5),
        line,
        "a distance equal to epsilon keeps the point"
    );
    let bump: Vec<LaserPoint> = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.05, 0.0],
        [2.0, 3.0, 0.0],
        [3.0, 0.05, 0.0],
        [4.0, 0.0, 0.0],
    ];
    assert_eq!(douglas_peucker(&bump, 1.0), vec![bump[0], bump[2], bump[4]]);
}

/// `addPoint` drops a point with the same x and y as the last one kept, and
/// a fresh pointer has no outline.
#[test]
fn add_point_skips_repeated_positions() {
    let mut laser = LaserPointer::new(LaserPointerOptions::default());
    assert!(laser.get_stroke_outline(None).is_empty());
    laser.add_point([1.0, 2.0, 0.5]).unwrap();
    laser.add_point([1.0, 2.0, 0.9]).unwrap();
    laser.add_point([3.0, 2.0, 0.9]).unwrap();
    assert_eq!(laser.original_points(), &[[1.0, 2.0, 0.5], [3.0, 2.0, 0.9]]);
}
