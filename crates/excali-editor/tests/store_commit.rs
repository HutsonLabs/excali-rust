//! A commit whose elements match the store's snapshot (ex-710). Upstream's
//! `StoreSnapshot.detectChangedElements` walks the scene's own element map
//! (`packages/element/src/store.ts:904-960`); the port's `Session::commit`
//! used to copy every element into a new map first, so a scroll at 1,000
//! elements copied 1,000 elements. [`StoreSnapshot::has_element_changes`]
//! answers from the elements as they are, and must agree with the
//! detection on every kind of change: none, a version bump, an element
//! added, removed, deleted, restored to a lower version, an image without
//! a file, and a duplicated id.

mod support;

use std::rc::Rc;

use excali_core::app_state::AppState;
use excali_core::element::{Element, ElementKind};
use excali_editor::session::{ActionResult, Session};
use excali_editor::store::CaptureUpdateAction;
use serde_json::json;
use support::{rect, with, TestEnv};

fn session(elements: Vec<Element>) -> Session<TestEnv> {
    let mut s = Session::new(TestEnv::with_layout(), AppState::default());
    s.sync_action_result(ActionResult {
        elements: Some(elements),
        app_state: None,
        capture_update: CaptureUpdateAction::Immediately,
    })
    .unwrap();
    s
}

/// What the snapshot's own detection says of `next`: whether a new
/// elements snapshot is made from it.
fn detected(s: &mut Session<TestEnv>, next: &[Element]) -> bool {
    let map = next
        .iter()
        .map(|e| (e.base.id.clone(), e.clone()))
        .collect();
    let snapshot = Rc::clone(s.store.snapshot());
    let cloned = snapshot.maybe_clone(
        CaptureUpdateAction::Immediately,
        Some(&map),
        None,
        &mut s.env,
    );
    !Rc::ptr_eq(&cloned.elements, &snapshot.elements)
}

fn check(s: &mut Session<TestEnv>, next: &[Element], want: bool, case: &str) {
    assert_eq!(detected(s, next), want, "{case}: detection");
    assert_eq!(
        s.store.snapshot().has_element_changes(next),
        want,
        "{case}: has_element_changes"
    );
}

#[test]
fn the_check_agrees_with_the_detection() {
    let mut s = session(vec![rect("a", 0.0, 0.0), rect("b", 200.0, 0.0)]);
    // as the session holds them: syncing the indices bumped their versions
    let (a, b) = (s.elements()[0].clone(), s.elements()[1].clone());
    check(&mut s, &[a.clone(), b.clone()], false, "unchanged");
    check(&mut s, &[b.clone(), a.clone()], false, "reordered");

    let moved = with(&a, json!({"x": 10}), &mut s.env);
    check(&mut s, &[moved.clone(), b.clone()], true, "version bump");
    check(
        &mut s,
        &[a.clone(), b.clone(), rect("c", 0.0, 0.0)],
        true,
        "added",
    );
    check(&mut s, std::slice::from_ref(&a), true, "removed");
    let deleted = with(&b, json!({"isDeleted": true}), &mut s.env);
    check(&mut s, &[a.clone(), deleted], true, "deleted");
    let mut older = a.clone();
    older.base.version -= 1.0;
    check(&mut s, &[older, b.clone()], false, "lower version");
    // the first of a duplicated id is not what the map keeps
    check(
        &mut s,
        &[moved.clone(), a.clone(), b.clone()],
        false,
        "duplicate, last unchanged",
    );
    check(
        &mut s,
        &[a.clone(), moved, b.clone()],
        true,
        "duplicate, last changed",
    );

    let mut image = rect("img", 0.0, 0.0);
    image.kind = ElementKind::Image(Default::default());
    check(
        &mut s,
        &[a.clone(), b.clone(), image],
        false,
        "image without a file",
    );
}

#[test]
fn a_scroll_commit_leaves_the_elements_snapshot() {
    let mut s = session(vec![rect("a", 0.0, 0.0), rect("b", 200.0, 0.0)]);
    let before = Rc::clone(&s.store.snapshot().elements);
    let mut patch = serde_json::Map::new();
    patch.insert("scrollX".into(), json!(12.5));
    s.set_state(patch);
    s.commit();
    assert!(Rc::ptr_eq(&before, &s.store.snapshot().elements));
}

/// A commit that hands the store the scene's map (`Store::commit_owned`)
/// takes the changed elements from it rather than copying them, and ends
/// in the same snapshot and increments as one that lends it.
#[test]
fn an_owned_commit_is_a_lent_one() {
    let scene = || session(vec![rect("a", 0.0, 0.0), rect("b", 200.0, 0.0)]);
    let (mut lent, mut owned) = (scene(), scene());
    let next = |s: &mut Session<TestEnv>| {
        let (a, b) = (s.elements()[0].clone(), s.elements()[1].clone());
        let moved = with(&a, json!({"x": 10}), &mut s.env);
        let map: excali_core::fractional_index::SceneElementsMap = [moved, b]
            .into_iter()
            .map(|e| (e.base.id.clone(), e))
            .collect();
        map
    };
    let (lent_map, owned_map) = (next(&mut lent), next(&mut owned));
    assert_eq!(lent_map, owned_map);
    let observed = |s: &Session<TestEnv>| {
        excali_editor::store::ObservedAppState::from_app_state(s.app_state())
    };
    let (lo, oo) = (observed(&lent), observed(&owned));
    lent.store.schedule_action(CaptureUpdateAction::Immediately);
    owned
        .store
        .schedule_action(CaptureUpdateAction::Immediately);
    let before = Rc::clone(&owned.store.snapshot().elements);
    let by_ref = lent.store.commit(Some(&lent_map), Some(&lo), &mut lent.env);
    let by_value = owned
        .store
        .commit_owned(Some(owned_map), Some(&oo), &mut owned.env);
    assert!(!by_value.is_empty());
    assert_eq!(format!("{by_ref:?}"), format!("{by_value:?}"));
    assert_eq!(
        format!("{:?}", lent.store.snapshot().elements),
        format!("{:?}", owned.store.snapshot().elements)
    );
    assert!(!Rc::ptr_eq(&before, &owned.store.snapshot().elements));
}
