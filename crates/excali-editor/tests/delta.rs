//! `ElementsDelta` and `AppStateDelta`, ported from
//! `packages/element/tests/delta.test.tsx` at the pinned commit.
//!
//! Upstream's cases that need text or arrow layout to run during
//! `redrawElements` ("detects a container resized by restoring an empty
//! label", "detects a bound label/arrow repositioned by a container's
//! redraw") exercise `redrawTextBoundingBox` and `updateBoundElements`,
//! whose calls `redrawElements` makes through `HistoryEnv`; here they are
//! the deterministic stand-ins of `tests/support`.

mod support;

use std::collections::HashSet;

use excali_core::element::{Element, ElementKind, FractionalIndex, StickyNoteFields};
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use excali_editor::delta::{
    redraw_elements, AppStateDelta, ApplyToOptions, Delta, ElementsDelta, Partial,
};
use excali_editor::mutate::mutate_element;
use excali_editor::store::{ObservedAppState, SnapshotElements};
use indexmap::IndexMap;
use serde_json::{json, Value};
use support::{arrow, obj, rect, text, with, TestEnv};

fn partial(value: Value) -> Partial {
    obj(value).into_iter().map(|(k, v)| (k, Some(v))).collect()
}

fn delta(deleted: Value, inserted: Value) -> Delta {
    Delta::new(partial(deleted), partial(inserted))
}

fn map(elements: &[Element]) -> SceneElementsMap {
    elements
        .iter()
        .map(|e| (e.base.id.clone(), e.clone()))
        .collect()
}

fn snapshot(elements: &SceneElementsMap) -> SnapshotElements {
    elements
        .iter()
        .map(|(id, e)| (id.clone(), std::rc::Rc::new(e.clone())))
        .collect()
}

fn history_options() -> ApplyToOptions {
    ApplyToOptions {
        excluded_properties: ["version", "versionNonce"]
            .into_iter()
            .map(String::from)
            .collect(),
    }
}

fn indexed(mut element: Element, index: &str) -> Element {
    element.base.index = Some(FractionalIndex(index.into()));
    element
}

/// `ElementsDelta.calculate(before, after).applyTo(before, before, {excludedProperties: version, versionNonce})`.
fn apply(
    before: &SceneElementsMap,
    after: &SceneElementsMap,
    env: &mut TestEnv,
) -> (SceneElementsMap, bool) {
    let mut delta = ElementsDelta::calculate(before, after, env);
    delta
        .apply_to(before, &snapshot(before), &history_options(), env)
        .expect("applyTo")
}

/// `boundText(type, text)`: a container `container` with an empty (or
/// given) label `label` bound to it, and the same pair with the label
/// deleted and unbound.
fn bound_text(
    kind: &str,
    content: &str,
    env: &mut TestEnv,
) -> (Element, Element, SceneElementsMap, SceneElementsMap) {
    let mut container = match kind {
        "arrow" => arrow("container", vec![[0.0, 0.0], [250.0, 0.0]]),
        _ => rect("container", 0.0, 0.0),
    };
    container.base.index = Some(FractionalIndex("a0".into()));
    container.base.width = 250.0;
    container.base.height = if kind == "arrow" { 0.0 } else { 250.0 };
    if kind == "stickynote" {
        container.kind = ElementKind::StickyNote(StickyNoteFields { base_height: 250.0 });
    }
    container.base.bound_elements = Some(vec![excali_core::element::BoundElement {
        id: "label".into(),
        kind: excali_core::element::BoundElementType::Text,
    }]);
    let mut label = indexed(text("label", content, 0.0, 0.0), "a1");
    if let ElementKind::Text(t) = &mut label.kind {
        t.container_id = Some("container".into());
        t.font_size = 20.0;
        t.text_align = excali_core::element::TextAlign::Center;
        t.vertical_align = excali_core::element::VerticalAlign::Middle;
    }
    let before = map(&[container.clone(), label.clone()]);
    let after = map(&[
        with(&container, json!({"boundElements": []}), env),
        with(&label, json!({"isDeleted": true}), env),
    ]);
    (container, label, before, after)
}

// ---------------------------------------------------------------------------
// visible changes

#[test]
fn ignores_empty_label_deletion_and_restoration() {
    for kind in ["rectangle", "stickynote"] {
        let mut env = TestEnv::default();
        let (container, label, before, after) = bound_text(kind, "", &mut env);
        let (deleted, visible) = apply(&before, &after, &mut env);
        assert!(!visible, "{kind}: deletion is not visible");
        assert!(deleted[&label.base.id].base.is_deleted);
        assert_eq!(
            deleted[&container.base.id].base.bound_elements,
            Some(vec![])
        );

        let (restored, visible) = apply(&after, &before, &mut env);
        assert!(!visible, "{kind}: restoration is not visible");
        assert!(!restored[&label.base.id].base.is_deleted);
        assert_eq!(
            support::prop(&restored[&container.base.id], "boundElements"),
            json!([{"id": "label", "type": "text"}])
        );
    }
}

#[test]
fn keeps_deletion_and_restoration_of_nonempty_labels_visible() {
    let mut env = TestEnv::default();
    let (_, _, before, after) = bound_text("stickynote", "hello", &mut env);
    assert!(apply(&before, &after, &mut env).1);
    assert!(apply(&after, &before, &mut env).1);
}

#[test]
fn ignores_excluded_and_already_applied_properties() {
    for mode in ["excluded", "already applied"] {
        let mut env = TestEnv::default();
        let element = indexed(rect("r", 0.0, 0.0), "a0");
        let before = map(std::slice::from_ref(&element));
        let after = map(&[with(&element, json!({"strokeColor": "red"}), &mut env)]);
        let mut delta = ElementsDelta::calculate(&before, &after, &mut env);
        let current = if mode == "excluded" { &before } else { &after };
        let mut options = history_options();
        if mode == "excluded" {
            options.excluded_properties.insert("strokeColor".into());
        }
        let (elements, visible) = delta
            .apply_to(current, &snapshot(current), &options, &mut env)
            .unwrap();
        assert_eq!(
            elements["r"].base.stroke_color, current["r"].base.stroke_color,
            "{mode}"
        );
        assert!(!visible, "{mode}");
    }
}

#[test]
fn keeps_snapshot_restoration_visible_for_a_metadata_only_update() {
    let mut env = TestEnv::default();
    let element = indexed(rect("r", 0.0, 0.0), "a0");
    let before = map(std::slice::from_ref(&element));
    let after = map(&[with(&element, json!({"version": 2}), &mut env)]);
    let mut delta = ElementsDelta::calculate(&before, &after, &mut env);
    let (elements, visible) = delta
        .apply_to(
            &SceneElementsMap::new(),
            &snapshot(&before),
            &ApplyToOptions::default(),
            &mut env,
        )
        .unwrap();
    assert!(!elements["r"].base.is_deleted);
    assert!(visible);
}

#[test]
fn keeps_an_arrows_empty_label_gap_visible() {
    let mut env = TestEnv::default();
    let (_, _, before, after) = bound_text("arrow", "", &mut env);
    assert!(apply(&before, &after, &mut env).1);
    assert!(apply(&after, &before, &mut env).1);
}

#[test]
fn detects_a_container_resized_by_restoring_an_empty_label() {
    let mut env = TestEnv::with_layout();
    let (container, label, _, _) = bound_text("rectangle", "", &mut env);
    let small = with(
        &container,
        json!({"width": 20, "height": 10, "boundElements": []}),
        &mut env,
    );
    let before = map(&[
        small.clone(),
        with(&label, json!({"isDeleted": true}), &mut env),
    ]);
    let after = map(&[
        with(
            &small,
            json!({"boundElements": [{"id": "label", "type": "text"}]}),
            &mut env,
        ),
        with(&label, json!({"isDeleted": false}), &mut env),
    ]);
    let (elements, visible) = apply(&before, &after, &mut env);
    assert!(elements["container"].base.height > 10.0);
    assert!(visible);
}

#[test]
fn detects_a_bound_label_repositioned_by_a_containers_redraw() {
    let mut env = TestEnv::with_layout();
    let (container, label, _, _) = bound_text("rectangle", "hello", &mut env);
    let misplaced = with(&label, json!({"x": -100, "y": -100}), &mut env);
    let before = map(&[container.clone(), misplaced.clone()]);
    let after = map(&[
        with(
            &container,
            json!({"version": container.base.version + 1.0}),
            &mut env,
        ),
        misplaced,
    ]);
    let (elements, visible) = apply(&before, &after, &mut env);
    // the changed container brings its label into redrawTextBoundingBoxes
    assert_eq!(
        env.text_redraws,
        [("label".to_string(), "container".to_string())]
    );
    assert_eq!(elements["container"].base.height, container.base.height);
    assert_ne!(elements["label"].base.x, -100.0);
    assert!(visible);
}

#[test]
fn detects_a_bound_arrow_repositioned_by_a_containers_redraw() {
    let mut env = TestEnv::with_layout();
    let mut container = indexed(rect("container", 0.0, 0.0), "a0");
    container.base.bound_elements = Some(vec![excali_core::element::BoundElement {
        id: "arrow".into(),
        kind: excali_core::element::BoundElementType::Arrow,
    }]);
    let mut bound = indexed(arrow("arrow", vec![[0.0, 0.0], [100.0, 0.0]]), "a1");
    bound.base.x = 200.0;
    bound.base.y = 50.0;
    if let Some(linear) = bound.kind.linear_mut() {
        linear.start_binding = Some(excali_core::element::FixedPointBinding {
            element_id: "container".into(),
            fixed_point: [1.0, 0.5],
            mode: excali_core::element::BindMode::Orbit,
        });
    }
    let before = map(&[container.clone(), bound.clone()]);
    let after = map(&[
        with(
            &container,
            json!({"version": container.base.version + 1.0}),
            &mut env,
        ),
        bound,
    ]);
    let (elements, visible) = apply(&before, &after, &mut env);
    assert_eq!(env.bound_updates.len(), 1);
    assert_eq!(env.bound_updates[0].0, "container");
    assert_eq!(elements["container"].base.width, container.base.width);
    assert_ne!(elements["arrow"].base.x, 200.0);
    assert!(visible);
}

// ---------------------------------------------------------------------------
// redrawElements: which elements the layout runs on (delta.ts:2034-2124)

fn contained(id: &str, container: &str) -> Element {
    let mut label = text(id, "label", 0.0, 0.0);
    if let ElementKind::Text(t) = &mut label.kind {
        t.container_id = Some(container.into());
    }
    label
}

fn container_of(id: &str, label: &str) -> Element {
    let mut container = rect(id, 0.0, 0.0);
    container.base.bound_elements = Some(vec![excali_core::element::BoundElement {
        id: label.into(),
        kind: excali_core::element::BoundElementType::Text,
    }]);
    container
}

fn deleted(mut element: Element) -> Element {
    element.base.is_deleted = true;
    element
}

fn pairs(env: &TestEnv) -> Vec<(&str, &str)> {
    env.text_redraws
        .iter()
        .map(|(t, c)| (t.as_str(), c.as_str()))
        .collect()
}

#[test]
fn redraws_text_boxes_from_both_directions_once_per_container() {
    let mut env = TestEnv::default();
    let mut elements = map(&[
        container_of("c1", "t1"),
        contained("t1", "c1"),
        container_of("c2", "t2"),
        contained("t2", "c2"),
        container_of("c3", "t3"),
        contained("t3", "c3"),
    ]);
    // a changed label brings its container, a changed container its label;
    // both changed is one pair
    let mut changed = map(&[
        elements["t1"].clone(),
        elements["c2"].clone(),
        elements["c3"].clone(),
        elements["t3"].clone(),
    ]);
    redraw_elements(&mut elements, &mut changed, &mut env).unwrap();
    assert_eq!(pairs(&env), [("t1", "c1"), ("t2", "c2"), ("t3", "c3")]);
}

#[test]
fn skips_text_boxes_with_a_deleted_side() {
    let mut env = TestEnv::default();
    let mut elements = map(&[
        // a deleted changed container with a live label
        deleted(container_of("c1", "t1")),
        contained("t1", "c1"),
        // a deleted changed label in a live container
        container_of("c2", "t2"),
        deleted(contained("t2", "c2")),
        // a live changed label whose container is deleted (not found among
        // the non-deleted elements)
        deleted(container_of("c3", "t3")),
        contained("t3", "c3"),
        // a live changed container whose label is deleted
        container_of("c4", "t4"),
        deleted(contained("t4", "c4")),
        // an empty containerId finds no container
        contained("t5", ""),
    ]);
    let mut changed = map(&[
        elements["c1"].clone(),
        elements["t2"].clone(),
        elements["t3"].clone(),
        elements["c4"].clone(),
        elements["t5"].clone(),
    ]);
    redraw_elements(&mut elements, &mut changed, &mut env).unwrap();
    assert!(env.text_redraws.is_empty(), "{:?}", env.text_redraws);
}

#[test]
fn redraws_arrows_of_non_deleted_bindable_elements_with_the_changed_elements() {
    let mut env = TestEnv::default();
    let mut elements = map(&[
        rect("rect", 0.0, 0.0),
        deleted(rect("gone", 0.0, 0.0)),
        arrow("arrow", vec![[0.0, 0.0], [10.0, 0.0]]),
        contained("label", "rect"),
        text("free", "free", 0.0, 0.0),
    ]);
    let mut changed = elements.clone();
    redraw_elements(&mut elements, &mut changed, &mut env).unwrap();
    let all: Vec<String> = ["rect", "gone", "arrow", "label", "free"]
        .map(String::from)
        .to_vec();
    assert_eq!(
        env.bound_updates,
        [("rect".to_string(), all.clone()), ("free".to_string(), all)]
    );
}

/// An arrow layout that fails unless `changed` shows the container as the
/// text layout left it.
fn expect_grown_container(
    _: &mut dyn ChangeStamp,
    elements: &mut SceneElementsMap,
    id: &str,
    changed: &SceneElementsMap,
) -> Result<(), String> {
    if changed[id] != elements[id] {
        return Err(format!("stale {id} in changedElements"));
    }
    Ok(())
}

#[test]
fn passes_the_laid_out_elements_as_changed_to_the_arrow_layout() {
    let mut env = TestEnv {
        text_layout: Some(support::centre_label),
        arrow_layout: Some(expect_grown_container),
        ..TestEnv::default()
    };
    let mut container = container_of("c", "t");
    container.base.height = 10.0;
    let mut elements = map(&[container, contained("t", "c")]);
    let mut changed = map(&[elements["c"].clone()]);
    redraw_elements(&mut elements, &mut changed, &mut env).unwrap();
    assert!(elements["c"].base.height > 10.0);
    assert_eq!(changed["c"], elements["c"]);
    assert_eq!(env.bound_updates.len(), 1);
}

/// An arrow layout that mutates an element no delta names and no binding
/// reaches.
fn move_unrelated(
    stamp: &mut dyn ChangeStamp,
    elements: &mut SceneElementsMap,
    _: &str,
    _: &SceneElementsMap,
) -> Result<(), String> {
    let mut unrelated = elements["unrelated"].clone();
    let x = unrelated.base.x + 1.0;
    mutate_element(
        &mut unrelated,
        elements,
        support::obj(json!({"x": x})),
        stamp,
    )
    .map_err(|e| e.to_string())?;
    elements.insert("unrelated".into(), unrelated);
    Ok(())
}

#[test]
fn guards_against_untracked_layout_mutations() {
    let mut env = TestEnv {
        arrow_layout: Some(move_unrelated),
        ..TestEnv::default()
    };
    let element = indexed(rect("changed", 0.0, 0.0), "a0");
    let unrelated = indexed(rect("unrelated", 0.0, 0.0), "a1");
    let before = map(&[element.clone(), unrelated.clone()]);
    let after = map(&[
        with(&element, json!({"strokeColor": "red"}), &mut env),
        unrelated,
    ]);
    let mut delta = ElementsDelta::calculate(&before, &after, &mut env);
    let error = delta
        .apply_to(&before, &snapshot(&before), &history_options(), &mut env)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Redrawn element \"unrelated\" is missing from idsToCheck"
    );
}

#[test]
fn keeps_an_empty_text_elements_arrow_bindings_visible() {
    let mut env = TestEnv::default();
    let mut label = indexed(text("label", "", 0.0, 0.0), "a0");
    label.base.bound_elements = Some(vec![excali_core::element::BoundElement {
        id: "arrow".into(),
        kind: excali_core::element::BoundElementType::Arrow,
    }]);
    let mut bound = indexed(arrow("arrow", vec![[0.0, 0.0], [100.0, 100.0]]), "a1");
    if let Some(linear) = bound.kind.linear_mut() {
        linear.start_binding = Some(excali_core::element::FixedPointBinding {
            element_id: "label".into(),
            fixed_point: [1.0, 0.5],
            mode: excali_core::element::BindMode::Orbit,
        });
    }
    let before = map(&[label.clone(), bound.clone()]);
    let after = map(&[
        with(
            &label,
            json!({"isDeleted": true, "boundElements": []}),
            &mut env,
        ),
        with(&bound, json!({"startBinding": null}), &mut env),
    ]);
    assert!(apply(&before, &after, &mut env).1);
}

// ---------------------------------------------------------------------------
// elements delta calculation

#[test]
fn does_not_fail_when_a_removed_element_was_already_deleted() {
    let mut env = TestEnv::default();
    let mut element = rect("r", 100.0, 100.0);
    element.base.is_deleted = true;
    let prev = map(&[element]);
    let delta = ElementsDelta::calculate(&prev, &SceneElementsMap::new(), &mut env);
    assert!(delta.removed.is_empty());
    assert_eq!(delta.updated.len(), 1);
}

#[test]
fn does_not_fail_when_adding_an_element_as_already_deleted() {
    let mut env = TestEnv::default();
    let mut element = rect("r", 100.0, 100.0);
    element.base.is_deleted = true;
    let next = map(&[element]);
    let delta = ElementsDelta::calculate(&SceneElementsMap::new(), &next, &mut env);
    assert!(delta.added.is_empty());
    assert_eq!(delta.updated.len(), 1);
}

#[test]
fn creates_an_updated_delta_for_a_version_only_change() {
    let mut env = TestEnv::default();
    let base = rect("r", 100.0, 100.0);
    let mut modified = base.clone();
    modified.base.version += 1.0;
    modified.base.version_nonce += 1.0;
    let delta = ElementsDelta::calculate(&map(&[base]), &map(&[modified]), &mut env);
    let mut updated = IndexMap::new();
    updated.insert(
        "r".to_string(),
        delta_of(
            json!({"version": 1, "versionNonce": 0}),
            json!({"version": 2, "versionNonce": 1}),
        ),
    );
    assert_eq!(
        delta,
        ElementsDelta::create(IndexMap::new(), IndexMap::new(), updated, false)
    );
}

fn delta_of(deleted: Value, inserted: Value) -> Delta {
    delta(deleted, inserted)
}

#[test]
fn calculates_added_removed_and_updated_deltas() {
    let mut env = TestEnv::default();
    let a = rect("a", 0.0, 0.0);
    let b = rect("b", 0.0, 0.0);
    let c = rect("c", 0.0, 0.0);
    let prev = map(&[a.clone(), b.clone()]);
    let next = map(&[
        with(&a, json!({"x": 5}), &mut env),
        with(&b, json!({"isDeleted": true}), &mut env),
        c,
    ]);
    let delta = ElementsDelta::calculate(&prev, &next, &mut env);
    assert_eq!(delta.updated.keys().collect::<Vec<_>>(), ["a"]);
    assert_eq!(delta.removed.keys().collect::<Vec<_>>(), ["b"]);
    assert_eq!(delta.added.keys().collect::<Vec<_>>(), ["c"]);
    // updated: only the differing keys, `id` and `updated` stripped
    let updated = &delta.updated["a"];
    assert_eq!(
        updated.deleted.keys().collect::<Vec<_>>(),
        ["version", "versionNonce", "x"]
    );
    // added: the whole element as inserted, a deleted stub before it
    let added = &delta.added["c"];
    assert_eq!(added.deleted["isDeleted"], Some(json!(true)));
    assert_eq!(added.deleted["version"], Some(json!(0)));
    assert!(!added.inserted.contains_key("id"));
    assert!(!added.inserted.contains_key("updated"));
    assert_eq!(added.inserted["type"], Some(json!("rectangle")));
    // the inverse swaps added and removed
    let inverse = delta.inverse();
    assert_eq!(inverse.added.keys().collect::<Vec<_>>(), ["b"]);
    assert_eq!(inverse.removed.keys().collect::<Vec<_>>(), ["c"]);
    assert_eq!(
        inverse.updated["a"].inserted["x"]
            .as_ref()
            .and_then(Value::as_f64),
        Some(0.0)
    );
}

// ---------------------------------------------------------------------------
// squash

fn created(
    added: Vec<(&str, Delta)>,
    removed: Vec<(&str, Delta)>,
    updated: Vec<(&str, Delta)>,
) -> ElementsDelta {
    let record = |v: Vec<(&str, Delta)>| -> IndexMap<String, Delta> {
        v.into_iter().map(|(k, d)| (k.to_string(), d)).collect()
    };
    ElementsDelta::create(record(added), record(removed), record(updated), false)
}

#[test]
fn does_not_squash_an_empty_delta() {
    let updated = delta(
        json!({"x": 100, "version": 1, "versionNonce": 1}),
        json!({"x": 200, "version": 2, "versionNonce": 2}),
    );
    let mut delta1 = created(vec![], vec![], vec![("id1", updated.clone())]);
    delta1.squash(&ElementsDelta::empty());
    assert!(!delta1.is_empty());
    assert_eq!(delta1.updated["id1"], updated);
}

#[test]
fn squashes_mutually_exclusive_delta_types() {
    let added = delta(
        json!({"x": 100, "version": 1, "versionNonce": 1, "isDeleted": true}),
        json!({"x": 200, "version": 2, "versionNonce": 2, "isDeleted": false}),
    );
    let removed = delta(
        json!({"x": 100, "version": 1, "versionNonce": 1, "isDeleted": false}),
        json!({"x": 200, "version": 2, "versionNonce": 2, "isDeleted": true}),
    );
    let updated = delta(
        json!({"x": 100, "version": 1, "versionNonce": 1}),
        json!({"x": 200, "version": 2, "versionNonce": 2}),
    );
    let mut delta1 = created(
        vec![("id1", added.clone())],
        vec![("id2", removed.clone())],
        vec![],
    );
    delta1.squash(&created(vec![], vec![], vec![("id3", updated.clone())]));
    assert_eq!(delta1.added["id1"], added);
    assert_eq!(delta1.removed["id2"], removed);
    assert_eq!(delta1.updated["id3"], updated);
}

#[test]
fn squashes_the_same_delta_types() {
    let mut delta1 = created(
        vec![(
            "id1",
            delta(
                json!({"x": 100, "version": 1, "versionNonce": 1, "isDeleted": true}),
                json!({"x": 200, "version": 2, "versionNonce": 2, "isDeleted": false}),
            ),
        )],
        vec![(
            "id2",
            delta(
                json!({"x": 100, "version": 1, "versionNonce": 1, "isDeleted": false}),
                json!({"x": 200, "version": 2, "versionNonce": 2, "isDeleted": true}),
            ),
        )],
        vec![(
            "id3",
            delta(
                json!({"x": 100, "version": 1, "versionNonce": 1}),
                json!({"x": 200, "version": 2, "versionNonce": 2}),
            ),
        )],
    );
    delta1.squash(&created(
        vec![(
            "id1",
            delta(
                json!({"y": 100, "version": 2, "versionNonce": 2, "isDeleted": true}),
                json!({"y": 200, "version": 3, "versionNonce": 3, "isDeleted": false}),
            ),
        )],
        vec![(
            "id2",
            delta(
                json!({"y": 100, "version": 2, "versionNonce": 2, "isDeleted": false}),
                json!({"y": 200, "version": 3, "versionNonce": 3, "isDeleted": true}),
            ),
        )],
        vec![(
            "id3",
            delta(
                json!({"y": 100, "version": 2, "versionNonce": 2}),
                json!({"y": 200, "version": 3, "versionNonce": 3}),
            ),
        )],
    ));
    assert_eq!(
        delta1.added["id1"],
        delta(
            json!({"x": 100, "y": 100, "version": 2, "versionNonce": 2, "isDeleted": true}),
            json!({"x": 200, "y": 200, "version": 3, "versionNonce": 3, "isDeleted": false}),
        )
    );
    assert_eq!(
        delta1.removed["id2"],
        delta(
            json!({"x": 100, "y": 100, "version": 2, "versionNonce": 2, "isDeleted": false}),
            json!({"x": 200, "y": 200, "version": 3, "versionNonce": 3, "isDeleted": true}),
        )
    );
    assert_eq!(
        delta1.updated["id3"],
        delta(
            json!({"x": 100, "y": 100, "version": 2, "versionNonce": 2}),
            json!({"x": 200, "y": 200, "version": 3, "versionNonce": 3}),
        )
    );
}

#[test]
fn squashes_different_delta_types() {
    // id1: added -> updated => added; id2: removed -> added => added;
    // id3: updated -> removed => removed
    let mut delta1 = created(
        vec![(
            "id1",
            delta(
                json!({"x": 100, "version": 1, "versionNonce": 1, "isDeleted": true}),
                json!({"x": 101, "version": 2, "versionNonce": 2, "isDeleted": false}),
            ),
        )],
        vec![(
            "id2",
            delta(
                json!({"x": 200, "version": 1, "versionNonce": 1, "isDeleted": false}),
                json!({"x": 201, "version": 2, "versionNonce": 2, "isDeleted": true}),
            ),
        )],
        vec![(
            "id3",
            delta(
                json!({"x": 300, "version": 1, "versionNonce": 1}),
                json!({"x": 301, "version": 2, "versionNonce": 2}),
            ),
        )],
    );
    delta1.squash(&created(
        vec![(
            "id2",
            delta(
                json!({"y": 200, "version": 2, "versionNonce": 2, "isDeleted": true}),
                json!({"y": 201, "version": 3, "versionNonce": 3, "isDeleted": false}),
            ),
        )],
        vec![(
            "id3",
            delta(
                json!({"y": 300, "version": 2, "versionNonce": 2, "isDeleted": false}),
                json!({"y": 301, "version": 3, "versionNonce": 3, "isDeleted": true}),
            ),
        )],
        vec![(
            "id1",
            delta(
                json!({"y": 100, "version": 2, "versionNonce": 2}),
                json!({"y": 101, "version": 3, "versionNonce": 3}),
            ),
        )],
    ));
    let expected_added: IndexMap<String, Delta> = [
        (
            "id1".to_string(),
            delta(
                json!({"x": 100, "y": 100, "version": 2, "versionNonce": 2, "isDeleted": true}),
                json!({"x": 101, "y": 101, "version": 3, "versionNonce": 3, "isDeleted": false}),
            ),
        ),
        (
            "id2".to_string(),
            delta(
                json!({"x": 200, "y": 200, "version": 2, "versionNonce": 2, "isDeleted": true}),
                json!({"x": 201, "y": 201, "version": 3, "versionNonce": 3, "isDeleted": false}),
            ),
        ),
    ]
    .into_iter()
    .collect();
    assert_eq!(delta1.added, expected_added);
    assert_eq!(
        delta1.removed["id3"],
        delta(
            json!({"x": 300, "y": 300, "version": 2, "versionNonce": 2, "isDeleted": false}),
            json!({"x": 301, "y": 301, "version": 3, "versionNonce": 3, "isDeleted": true}),
        )
    );
    assert_eq!(delta1.removed.len(), 1);
    assert!(delta1.updated.is_empty());
}

#[test]
fn squashes_bound_elements() {
    let mut delta1 = created(
        vec![],
        vec![],
        vec![(
            "id1",
            delta(
                json!({"version": 1, "versionNonce": 1, "boundElements": [{"id": "t1", "type": "text"}]}),
                json!({"version": 2, "versionNonce": 2, "boundElements": [{"id": "t2", "type": "text"}]}),
            ),
        )],
    );
    delta1.squash(&created(
        vec![],
        vec![],
        vec![(
            "id1",
            delta(
                json!({"version": 2, "versionNonce": 2, "boundElements": [{"id": "a1", "type": "arrow"}]}),
                json!({"version": 3, "versionNonce": 3, "boundElements": [{"id": "a2", "type": "arrow"}]}),
            ),
        )],
    ));
    assert_eq!(
        delta1.updated["id1"].deleted["boundElements"],
        Some(json!([{"id": "t1", "type": "text"}, {"id": "a1", "type": "arrow"}]))
    );
    assert_eq!(
        delta1.updated["id1"].inserted["boundElements"],
        Some(json!([{"id": "t2", "type": "text"}, {"id": "a2", "type": "arrow"}]))
    );
}

// ---------------------------------------------------------------------------
// AppStateDelta

fn observed(value: Value) -> ObservedAppState {
    ObservedAppState::from_map(&obj(value))
}

fn dto(delta: &AppStateDelta) -> String {
    serde_json::to_string(&delta.to_dto()).unwrap()
}

#[test]
fn app_state_delta_keeps_a_stable_order_for_root_properties() {
    let common = json!({
        "viewBackgroundColor": "#ffffff",
        "selectedElementIds": {},
        "selectedGroupIds": {},
        "editingGroupId": null,
        "croppingElementId": null,
        "lockedMultiSelections": {},
        "activeLockedId": null,
    });
    let with_keys = |first: Value, last: bool| {
        let mut m = obj(common.clone());
        let extra = obj(first);
        if last {
            m.extend(extra);
            m
        } else {
            let mut n = extra;
            n.extend(m);
            n
        }
    };
    let linear = json!({"elementId": "id1", "isEditing": false});
    let prev1 = with_keys(json!({"name": "", "selectedLinearElement": null}), true);
    let next1 = with_keys(
        json!({"name": "untitled scene", "selectedLinearElement": linear}),
        true,
    );
    let prev2 = with_keys(json!({"selectedLinearElement": null, "name": ""}), false);
    let next2 = with_keys(
        json!({"selectedLinearElement": linear, "name": "untitled scene"}),
        false,
    );
    let d1 = AppStateDelta::calculate(
        &ObservedAppState::from_map(&prev1),
        &ObservedAppState::from_map(&next1),
    );
    let d2 = AppStateDelta::calculate(
        &ObservedAppState::from_map(&prev2),
        &ObservedAppState::from_map(&next2),
    );
    assert_eq!(dto(&d1), dto(&d2));
}

#[test]
fn app_state_delta_keeps_a_stable_order_for_selected_element_ids() {
    let make = |ids: Value| {
        let mut m = obj(json!({
            "name": "",
            "viewBackgroundColor": "#ffffff",
            "selectedGroupIds": {},
            "editingGroupId": null,
            "croppingElementId": null,
            "selectedLinearElement": null,
            "activeLockedId": null,
            "lockedMultiSelections": {},
        }));
        m.insert("selectedElementIds".into(), ids);
        ObservedAppState::from_map(&m)
    };
    let d1 = AppStateDelta::calculate(
        &make(json!({"id5": true, "id2": true, "id4": true})),
        &make(json!({"id1": true, "id2": true, "id3": true})),
    );
    let d2 = AppStateDelta::calculate(
        &make(json!({"id4": true, "id2": true, "id5": true})),
        &make(json!({"id3": true, "id2": true, "id1": true})),
    );
    assert_eq!(dto(&d1), dto(&d2));
    // only the differing ids are kept
    assert_eq!(
        d1.delta.deleted["selectedElementIds"],
        Some(json!({"id4": true, "id5": true}))
    );
    assert_eq!(
        d1.delta.inserted["selectedElementIds"],
        Some(json!({"id1": true, "id3": true}))
    );
}

#[test]
fn app_state_delta_keeps_a_stable_order_for_selected_group_ids() {
    let make = |ids: Value| {
        let mut m = obj(json!({
            "name": "",
            "viewBackgroundColor": "#ffffff",
            "selectedElementIds": {},
            "editingGroupId": null,
            "croppingElementId": null,
            "selectedLinearElement": null,
            "activeLockedId": null,
            "lockedMultiSelections": {},
        }));
        m.insert("selectedGroupIds".into(), ids);
        ObservedAppState::from_map(&m)
    };
    let d1 = AppStateDelta::calculate(
        &make(json!({"id5": false, "id2": true, "id4": true, "id0": true})),
        &make(json!({"id0": true, "id1": true, "id2": false, "id3": true})),
    );
    let d2 = AppStateDelta::calculate(
        &make(json!({"id0": true, "id4": true, "id2": true, "id5": false})),
        &make(json!({"id3": true, "id2": false, "id1": true, "id0": true})),
    );
    assert_eq!(dto(&d1), dto(&d2));
}

#[test]
fn app_state_delta_does_not_squash_an_empty_delta() {
    let inner = delta(
        json!({"name": "untitled scene"}),
        json!({"name": "titled scene"}),
    );
    let mut d1 = AppStateDelta::create(inner.clone());
    d1.squash(&AppStateDelta::empty());
    assert!(!d1.is_empty());
    assert_eq!(d1.delta, inner);
}

#[test]
fn app_state_delta_squashes_exclusive_properties() {
    let mut d1 = AppStateDelta::create(delta(
        json!({"name": "untitled scene"}),
        json!({"name": "titled scene"}),
    ));
    d1.squash(&AppStateDelta::create(delta(
        json!({"viewBackgroundColor": "#ffffff"}),
        json!({"viewBackgroundColor": "#000000"}),
    )));
    assert_eq!(
        d1.delta,
        delta(
            json!({"name": "untitled scene", "viewBackgroundColor": "#ffffff"}),
            json!({"name": "titled scene", "viewBackgroundColor": "#000000"}),
        )
    );
}

#[test]
fn app_state_delta_squashes_selections() {
    let mut d1 = AppStateDelta::create(delta(
        json!({
            "name": "untitled scene",
            "selectedElementIds": {"id1": true},
            "selectedGroupIds": {},
            "lockedMultiSelections": {"g1": true},
        }),
        json!({
            "name": "titled scene",
            "selectedElementIds": {"id2": true},
            "selectedGroupIds": {"g1": true},
            "lockedMultiSelections": {},
        }),
    ));
    d1.squash(&AppStateDelta::create(delta(
        json!({
            "selectedElementIds": {"id3": true},
            "selectedGroupIds": {"g1": true},
            "lockedMultiSelections": {},
        }),
        json!({
            "selectedElementIds": {"id2": true},
            "selectedGroupIds": {"g2": true, "g3": true},
            "lockedMultiSelections": {"g3": true},
        }),
    )));
    assert_eq!(
        d1.delta,
        delta(
            json!({
                "name": "untitled scene",
                "selectedElementIds": {"id1": true, "id3": true},
                "selectedGroupIds": {"g1": true},
                "lockedMultiSelections": {"g1": true},
            }),
            json!({
                "name": "titled scene",
                "selectedElementIds": {"id2": true},
                "selectedGroupIds": {"g1": true, "g2": true, "g3": true},
                "lockedMultiSelections": {"g3": true},
            }),
        )
    );
}

#[test]
fn app_state_delta_applies_selection_as_a_diff() {
    let mut app_state = excali_core::app_state::AppState::default();
    app_state.insert("selectedElementIds", json!({"a": true, "b": true}));
    let d = AppStateDelta::create(delta(
        json!({"selectedElementIds": {"a": true}}),
        json!({"selectedElementIds": {"c": true}}),
    ));
    let elements = map(&[
        rect("a", 0.0, 0.0),
        rect("b", 0.0, 0.0),
        rect("c", 0.0, 0.0),
    ]);
    let (next, visible) = d.apply_to(&app_state, &elements);
    assert_eq!(
        next.get("selectedElementIds"),
        Some(&json!({"b": true, "c": true}))
    );
    assert!(visible);
    // selections of deleted elements are dropped and are not visible
    let mut deleted = elements.clone();
    deleted.get_mut("c").unwrap().base.is_deleted = true;
    let d = AppStateDelta::create(delta(
        json!({"selectedElementIds": {}}),
        json!({"selectedElementIds": {"c": true}}),
    ));
    let mut none_selected = excali_core::app_state::AppState::default();
    none_selected.insert("selectedElementIds", json!({}));
    let (next, visible) = d.apply_to(&none_selected, &deleted);
    assert_eq!(next.get("selectedElementIds"), Some(&json!({})));
    assert!(!visible);
    let _ = observed(json!({}));
    let _: HashSet<String> = HashSet::new();
}
