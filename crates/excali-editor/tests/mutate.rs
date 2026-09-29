//! Version bumps: `mutateElement`, `newElementWith` and `bumpVersion`
//! (`packages/element/src/mutateElement.ts:40-196` at the pinned commit).
//!
//! Every change bumps `version` (by one, unless the update names one),
//! draws a fresh `versionNonce` (unless the update names one) and sets
//! `updated` to `getUpdatedTimestamp()`; an update that changes nothing
//! leaves the element as it was.

mod support;

use excali_core::element::{ArrowFields, ElementKind, LinearFields};
use excali_core::fractional_index::SceneElementsMap;
use excali_editor::mutate::{bump_version, get_size_from_points, mutate_element, new_element_with};
use serde_json::json;
use support::{arrow, obj, prop, rect, TestEnv};

fn empty_map() -> SceneElementsMap {
    SceneElementsMap::new()
}

#[test]
fn mutate_element_bumps_version_nonce_and_updated() {
    let mut env = TestEnv::default();
    let mut element = rect("A", 0.0, 0.0);
    element.base.updated = 42.0;
    let changed = mutate_element(
        &mut element,
        &empty_map(),
        obj(json!({"x": 10, "strokeColor": "#ff0000"})),
        &mut env,
    )
    .unwrap();
    assert!(changed);
    assert_eq!(element.base.x, 10.0);
    assert_eq!(element.base.stroke_color, "#ff0000");
    // mutateElement.ts:142-144
    assert_eq!(element.base.version, 2.0);
    assert_eq!(element.base.version_nonce, 1001.0);
    assert_eq!(element.base.updated, 1.0);
}

#[test]
fn mutate_element_without_changes_keeps_the_element() {
    let mut env = TestEnv::default();
    let mut element = rect("A", 5.0, 5.0);
    let before = element.clone();
    let changed = mutate_element(
        &mut element,
        &empty_map(),
        obj(json!({"x": 5, "y": 5, "frameId": null, "isDeleted": false})),
        &mut env,
    )
    .unwrap();
    assert!(!changed);
    assert_eq!(element, before);
    assert_eq!(env.nonce, 0.0, "no nonce drawn for a no-op");
}

#[test]
fn mutate_element_takes_version_and_nonce_from_the_update() {
    let mut env = TestEnv::default();
    let mut element = rect("A", 0.0, 0.0);
    mutate_element(
        &mut element,
        &empty_map(),
        obj(json!({"x": 1, "version": 7, "versionNonce": 99})),
        &mut env,
    )
    .unwrap();
    assert_eq!(element.base.version, 7.0);
    assert_eq!(element.base.version_nonce, 99.0);
    assert_eq!(env.nonce, 0.0);
}

#[test]
fn mutate_element_always_applies_object_values() {
    // "if object, always update because its attrs could have changed"
    let mut env = TestEnv::default();
    let mut element = rect("A", 0.0, 0.0);
    element.base.bound_elements = Some(vec![]);
    let changed = mutate_element(
        &mut element,
        &empty_map(),
        obj(json!({"boundElements": []})),
        &mut env,
    )
    .unwrap();
    assert!(changed);
    assert_eq!(element.base.version, 2.0);
}

#[test]
fn mutate_element_compares_group_ids_and_scale_by_value() {
    let mut env = TestEnv::default();
    let mut element = rect("A", 0.0, 0.0);
    element.base.group_ids = vec!["g".into()];
    let changed = mutate_element(
        &mut element,
        &empty_map(),
        obj(json!({"groupIds": ["g"]})),
        &mut env,
    )
    .unwrap();
    assert!(!changed);
    assert_eq!(element.base.version, 1.0);
}

#[test]
fn mutate_element_skips_points_equal_but_for_the_first() {
    // `while (--index)` never compares index 0 (mutateElement.ts:102-112)
    let mut env = TestEnv::default();
    let mut element = arrow("A", vec![[0.0, 0.0], [10.0, 10.0]]);
    let changed = mutate_element(
        &mut element,
        &empty_map(),
        obj(json!({"points": [[5, 5], [10, 10]]})),
        &mut env,
    )
    .unwrap();
    // width and height come from getSizeFromPoints, which differ (5 vs 10),
    // so the element changes, but the points are kept as they were
    assert!(changed);
    assert_eq!(prop(&element, "points"), json!([[0, 0], [10, 10]]));
    assert_eq!(element.base.width, 5.0);
    assert_eq!(element.base.height, 5.0);
}

#[test]
fn mutate_element_sizes_from_new_points() {
    let mut env = TestEnv::default();
    let mut element = arrow("A", vec![[0.0, 0.0], [10.0, 10.0]]);
    mutate_element(
        &mut element,
        &empty_map(),
        obj(json!({"points": [[0, 0], [30, -20], [40, 10]]})),
        &mut env,
    )
    .unwrap();
    assert_eq!(prop(&element, "points"), json!([[0, 0], [30, -20], [40, 10]]));
    assert_eq!((element.base.width, element.base.height), (40.0, 30.0));
    assert_eq!(element.base.version, 2.0);
    // an explicit size wins over the computed one
    mutate_element(
        &mut element,
        &empty_map(),
        obj(json!({"points": [[0, 0], [1, 1]], "width": 7})),
        &mut env,
    )
    .unwrap();
    assert_eq!((element.base.width, element.base.height), (7.0, 1.0));
}

#[test]
fn get_size_from_points_is_the_extent() {
    assert_eq!(
        get_size_from_points(&[[1.0, 2.0], [-3.0, 8.0], [4.0, 0.0]]),
        (7.0, 8.0)
    );
}

#[test]
fn mutate_element_routes_elbow_arrows_and_zeroes_their_angle() {
    let mut env = TestEnv::default();
    let mut element = arrow("A", vec![[0.0, 0.0], [100.0, 100.0]]);
    element.kind = ElementKind::Arrow(ArrowFields::new(
        LinearFields::new(vec![[0.0, 0.0], [100.0, 100.0]]),
        true,
    ));
    element.base.angle.0 = 1.0;
    mutate_element(
        &mut element,
        &empty_map(),
        obj(json!({"points": [[0, 0], [100, 50]]})),
        &mut env,
    )
    .unwrap();
    assert_eq!(element.base.angle.0, 0.0);
    let points = prop(&element, "points");
    let points = points.as_array().unwrap();
    // an elbow route between free ends has only axis-aligned segments
    assert!(points.len() >= 3, "{points:?}");
    for pair in points.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        assert!(a[0] == b[0] || a[1] == b[1], "{a} -> {b}");
    }
    assert_eq!(prop(&element, "width"), json!(100));
    assert_eq!(prop(&element, "height"), json!(50));
}

#[test]
fn new_element_with_returns_the_same_element_without_changes() {
    let mut env = TestEnv::default();
    let element = rect("A", 3.0, 3.0);
    let next = new_element_with(&element, obj(json!({"x": 3})), false, &mut env).unwrap();
    assert_eq!(next, element);
    assert_eq!(env.nonce, 0.0);
    // `force` regenerates anyway (mutateElement.ts:149-181)
    let forced = new_element_with(&element, obj(json!({"x": 3})), true, &mut env).unwrap();
    assert_eq!(forced.base.version, 2.0);
    assert_eq!(forced.base.version_nonce, 1001.0);
}

#[test]
fn new_element_with_copies_and_bumps() {
    let mut env = TestEnv::default();
    let mut element = rect("A", 0.0, 0.0);
    element.base.updated = 5.0;
    let next = new_element_with(
        &element,
        obj(json!({"backgroundColor": "#ffc9c9", "groupIds": []})),
        false,
        &mut env,
    )
    .unwrap();
    assert_eq!(element.base.version, 1.0, "the original is untouched");
    assert_eq!(next.base.background_color, "#ffc9c9");
    assert_eq!(next.base.version, 2.0);
    assert_eq!(next.base.version_nonce, 1001.0);
    assert_eq!(next.base.updated, 1.0);
    // an object value always counts as a change
    let again = new_element_with(&next, obj(json!({"groupIds": []})), false, &mut env).unwrap();
    assert_eq!(again.base.version, 3.0);
    // an explicit version and nonce are kept
    let pinned = new_element_with(
        &again,
        obj(json!({"x": 1, "version": 10, "versionNonce": 5})),
        false,
        &mut env,
    )
    .unwrap();
    assert_eq!(
        (pinned.base.version, pinned.base.version_nonce),
        (10.0, 5.0)
    );
}

#[test]
fn bump_version_increments_the_given_or_current_version() {
    let mut env = TestEnv::default();
    let mut element = rect("A", 0.0, 0.0);
    bump_version(&mut element, None, &mut env);
    assert_eq!(element.base.version, 2.0);
    assert_eq!(element.base.version_nonce, 1001.0);
    bump_version(&mut element, Some(10.0), &mut env);
    assert_eq!(element.base.version, 11.0);
    assert_eq!(element.base.version_nonce, 1002.0);
    assert_eq!(element.base.updated, 1.0);
}
