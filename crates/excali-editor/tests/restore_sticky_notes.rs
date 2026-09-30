//! Sticky notes refitted on restore (ex-703).
//!
//! `restoreElements` with `refreshDimensions` lays out every sticky note
//! and its label with `getStickyNoteLayout` (`restore.ts:931-941`,
//! `packages/element/src/stickyNote.ts:669-762`), which measures text, so
//! excali-core asks its environment ([`RestoreEnv::sticky_note_layout`]);
//! excali-editor answers with [`sticky_note_layout`] ([`StickyNoteEnv`]).
//! Every `getStickyNoteLayout` hook excali-core's `restore-elements.json`
//! records (generated from upstream by
//! `tools/goldens/restore-elements-fixtures.mjs`, text 10 px per character)
//! is reproduced from its arguments, and so is the restored scene around
//! it.

use std::collections::VecDeque;

use excali_core::json;
use excali_core::restore::{
    restore_elements, ElbowArrowRequest, LegacyBinding, LegacyBindingRequest,
    RestoreElementsOptions, RestoreEnv, StickyNoteLayout, StickyNoteLayoutRequest, TestEnv,
    TextDimensionsRequest,
};
use excali_editor::restore_env::{sticky_note_layout, RoutingEnv, StickyNoteEnv};
use excali_editor::text_layout::{SceneArrowGeometry, TextLayouter};
use excali_text::restore_env::TextEnv;
use excali_text::text_measurements::CharCountTextMetrics;
use serde_json::{Map, Value};

const RESTORE: &str = include_str!("../../excali-core/tests/fixtures/restore-elements.json");

fn cases() -> Vec<Value> {
    let fixture: Value = serde_json::from_str(RESTORE).expect("restore fixture parses");
    fixture["cases"].as_array().expect("cases").clone()
}

fn is_sticky(hook: &Value) -> bool {
    hook["hook"] == "getStickyNoteLayout"
}

/// JSON equality with numbers compared as the doubles they denote and
/// objects compared by key, whatever their order.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

fn answer(layout: Option<StickyNoteLayout>) -> Value {
    match layout {
        None => Value::Null,
        Some(layout) => {
            let mut out = Map::new();
            out.insert("container".into(), Value::Object(layout.container));
            out.insert(
                "text".into(),
                layout.text.map_or(Value::Null, Value::Object),
            );
            Value::Object(out)
        }
    }
}

/// The port's `getStickyNoteLayout` on restore's arguments, checked
/// against each recorded call in turn: the note as restore passes it, its
/// label's id, and upstream's result.
struct Checked {
    test: TestEnv,
    layouter: TextLayouter<CharCountTextMetrics>,
    hooks: VecDeque<Value>,
    checked: usize,
    failures: Vec<String>,
}

impl RestoreEnv for Checked {
    fn now(&mut self) -> f64 {
        self.test.now()
    }
    fn random_id(&mut self) -> String {
        self.test.random_id()
    }
    fn random_integer(&mut self) -> f64 {
        self.test.random_integer()
    }
    fn sticky_note_layout(
        &mut self,
        request: StickyNoteLayoutRequest<'_>,
    ) -> Option<StickyNoteLayout> {
        let got = self
            .layouter
            .with_layout(|layout, _| sticky_note_layout(layout, &request));
        let id = request.note["id"].clone();
        let Some(hook) = self.hooks.pop_front() else {
            self.failures.push(format!("{id}: not called upstream"));
            return got;
        };
        self.checked += 1;
        let note = Value::Object(request.note.clone());
        if !same(&note, &hook["element"]) {
            self.failures
                .push(format!("{id}: note {note}, upstream {}", hook["element"]));
        }
        let label = request.text.map_or(Value::Null, |t| t["id"].clone());
        if label != hook["other"] {
            self.failures
                .push(format!("{id}: label {label}, upstream {}", hook["other"]));
        }
        let answer = answer(got.clone());
        if !same(&answer, &hook["result"]) {
            self.failures
                .push(format!("{id}: got {answer}, upstream {}", hook["result"]));
        }
        got
    }
}

/// The other hooks the scenes need, answered by the port: text refits
/// ([`TextEnv`] with arrow labels), elbow arrows and legacy bindings
/// ([`RoutingEnv`]); sticky notes by [`Checked`].
struct Scene(RoutingEnv<TextEnv<Checked, CharCountTextMetrics, SceneArrowGeometry>>);

impl RestoreEnv for Scene {
    fn now(&mut self) -> f64 {
        self.0.now()
    }
    fn random_id(&mut self) -> String {
        self.0.random_id()
    }
    fn random_integer(&mut self) -> f64 {
        self.0.random_integer()
    }
    fn migrate_legacy_binding(
        &mut self,
        request: LegacyBindingRequest<'_>,
    ) -> Option<LegacyBinding> {
        self.0.migrate_legacy_binding(request)
    }
    fn refresh_text_dimensions(
        &mut self,
        request: TextDimensionsRequest<'_>,
    ) -> Option<Map<String, Value>> {
        self.0.refresh_text_dimensions(request)
    }
    fn sticky_note_layout(
        &mut self,
        request: StickyNoteLayoutRequest<'_>,
    ) -> Option<StickyNoteLayout> {
        self.0.inner.inner.sticky_note_layout(request)
    }
    fn update_elbow_arrow_points(
        &mut self,
        request: ElbowArrowRequest<'_>,
    ) -> Option<Map<String, Value>> {
        self.0.update_elbow_arrow_points(request)
    }
}

/// Every recorded `getStickyNoteLayout` call is made by the port's restore
/// with the same arguments and answered with upstream's result, and the
/// restored scene is upstream's, element for element.
#[test]
fn restore_hooks_are_reproduced() {
    let mut hooks = 0;
    let mut scenes = 0;
    for case in cases() {
        let Some(recorded) = case["hooks"].as_array() else {
            continue;
        };
        if !recorded.iter().any(is_sticky) {
            continue;
        }
        assert_eq!(case["call"], "restoreElements", "{}", case["id"]);
        let opts = &case["opts"];
        let options = RestoreElementsOptions {
            refresh_dimensions: opts["refreshDimensions"] == true,
            repair_bindings: opts["repairBindings"] == true,
            delete_invisible_elements: opts["deleteInvisibleElements"] == true,
        };
        let checked = Checked {
            test: TestEnv::default(),
            layouter: TextLayouter::new(CharCountTextMetrics),
            hooks: recorded.iter().filter(|h| is_sticky(h)).cloned().collect(),
            checked: 0,
            failures: Vec::new(),
        };
        let mut env = Scene(RoutingEnv::new(TextEnv::with_geometry(
            checked,
            CharCountTextMetrics,
            SceneArrowGeometry,
        )));
        let elements = case["elements"].as_array().expect("elements").clone();
        let restored = restore_elements(&elements, None, options, &mut env).expect("restores");
        let checked = &env.0.inner.inner;
        assert!(
            checked.failures.is_empty(),
            "{}:\n{}",
            case["id"],
            checked.failures.join("\n")
        );
        assert!(checked.hooks.is_empty(), "{}: calls left", case["id"]);
        hooks += checked.checked;
        scenes += 1;
        let got = json::to_string_compact(&Value::Array(
            restored.into_iter().map(Value::Object).collect(),
        ));
        assert_eq!(
            got,
            json::to_string_compact(&case["result"]),
            "{}",
            case["id"]
        );
    }
    assert!(hooks >= 3, "{hooks} sticky note hooks");
    assert!(scenes >= 2, "{scenes} scenes");
}

/// [`StickyNoteEnv`] answers the layout and hands every other request to
/// the environment it wraps.
#[test]
fn sticky_note_env_answers_the_layout() {
    let case = cases()
        .into_iter()
        .find(|c| c["id"] == "upstream-sticky-refit")
        .expect("the refit case");
    let hook = case["hooks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| is_sticky(h))
        .unwrap()
        .clone();
    let note = hook["element"].as_object().unwrap().clone();
    let elements: Vec<Map<String, Value>> = case["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_object().unwrap().clone())
        .collect();
    let label = elements
        .iter()
        .find(|e| e["id"] == hook["other"])
        .expect("the label");
    let mut env = StickyNoteEnv::new(TestEnv::default(), CharCountTextMetrics);
    let got = env.sticky_note_layout(StickyNoteLayoutRequest {
        note: &note,
        text: Some(label),
        elements: &elements,
    });
    assert!(same(&answer(got), &hook["result"]));
    assert_eq!(env.now(), 1.0);
    assert_eq!(env.random_id(), "id0");
    // a note alone sits at its base height
    let alone = env
        .sticky_note_layout(StickyNoteLayoutRequest {
            note: &note,
            text: None,
            elements: &elements,
        })
        .expect("a layout");
    assert_eq!(alone.text, None);
    assert_eq!(alone.container["height"].as_f64(), Some(250.0));
}
