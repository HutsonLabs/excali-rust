//! `refreshTextDimensions` (`packages/element/src/newElement.ts:533-580` at
//! the pinned commit, with `getAdjustedDimensions` 393-483 and
//! `adjustXYWithRotation` 485-531) and the [`RestoreEnv`] it answers.
//!
//! `restoreElements` with `refreshDimensions` asks its environment for the
//! refreshed dimensions of every text but a sticky label
//! (`restore.ts:1032-1045`); [`TextEnv`] answers with this port. Every
//! `refreshTextDimensions` hook recorded in excali-core's
//! `restore-elements.json` (generated from upstream by
//! `tools/goldens/restore-elements-fixtures.mjs`, measuring 10 px per
//! character as upstream's test environment does) is reproduced from its
//! arguments, and so is the restored scene around it.

mod elements;

use excali_core::element::Element;
use excali_core::json;
use excali_core::restore::{
    restore_elements, RestoreElementsOptions, RestoreEnv, StickyNoteLayout,
    StickyNoteLayoutRequest, TestEnv, TextDimensionsRequest,
};
use excali_text::new_element::{
    get_text_anchor_ratios, refresh_text_dimensions, RefreshedText, TextLayout,
};
use excali_text::restore_env::TextEnv;
use excali_text::text_element::{ArrowLabelGeometry, NoArrowGeometry};
use excali_text::text_measurements::{CharCountTextMetrics, CharWidthCache};
use serde_json::{json, Map, Value};

use elements::element;

const RESTORE: &str = include_str!("../../excali-core/tests/fixtures/restore-elements.json");

fn object(value: &Value) -> Map<String, Value> {
    value.as_object().expect("an object").clone()
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

fn cases() -> Vec<Value> {
    let fixture: Value = serde_json::from_str(RESTORE).expect("restore fixture parses");
    fixture["cases"].as_array().expect("cases").clone()
}

fn is_refresh(hook: &Value) -> bool {
    hook["hook"] == "refreshTextDimensions"
}

/// excali-core's recorded `refreshTextDimensions` hooks, answered by
/// [`TextEnv`] from the same arguments: the text as restore passed it, its
/// container (the restored element the fixture names) and the restored
/// elements.
#[test]
fn restore_hooks_are_reproduced() {
    let mut hooks = 0;
    let mut containers = std::collections::BTreeSet::new();
    for case in cases() {
        let Some(recorded) = case["hooks"].as_array() else {
            continue;
        };
        let elements: Vec<Map<String, Value>> = case["result"]
            .as_array()
            .map(|r| r.iter().map(object).collect())
            .unwrap_or_default();
        for hook in recorded.iter().filter(|h| is_refresh(h)) {
            hooks += 1;
            let text = object(&hook["element"]);
            let container = elements.iter().find(|e| e["id"] == hook["other"]);
            if let Some(c) = container {
                containers.insert(c["type"].as_str().unwrap_or_default().to_owned());
            }
            assert_eq!(
                container.is_some(),
                !hook["other"].is_null(),
                "{}: container {}",
                case["id"],
                hook["other"]
            );
            let mut env = TextEnv::new(TestEnv::default(), CharCountTextMetrics);
            let answer = env.refresh_text_dimensions(TextDimensionsRequest {
                text: &text,
                container,
                elements: &elements,
            });
            let got = answer.map_or(Value::Null, Value::Object);
            if container.is_some_and(|c| c["type"] == "arrow") {
                // an arrow label's box is excali-editor's (ex-511): without
                // an ArrowLabelGeometry the label keeps its stored size, and
                // excali-editor's tests/restore_arrow_labels.rs reproduces
                // these hooks with SceneArrowGeometry
                assert_eq!(got, Value::Null, "{} {}", case["id"], text["id"]);
                continue;
            }
            assert!(
                same(&got, &hook["result"]),
                "{} {}: got {got}, upstream {}",
                case["id"],
                text["id"],
                hook["result"]
            );
        }
    }
    assert!(
        hooks >= 14,
        "the restore fixture records {hooks} refresh hooks"
    );
    for ty in ["rectangle", "ellipse", "diamond", "line", "arrow"] {
        assert!(containers.contains(ty), "no recorded {ty} container");
    }
}

/// Answers `getStickyNoteLayout` from the fixture (excali-editor's
/// `tests/restore_sticky_notes.rs` reproduces it), so
/// that the scenes around the refresh hooks can be restored whole.
struct StickyReplay {
    test: TestEnv,
    layouts: Vec<Value>,
}

impl RestoreEnv for StickyReplay {
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
        let recorded = self.layouts.remove(0);
        assert_eq!(recorded["element"]["id"], request.note["id"]);
        let result = &recorded["result"];
        Some(StickyNoteLayout {
            container: result["container"].as_object()?.clone(),
            text: result["text"].as_object().cloned(),
        })
    }
}

/// The restored scenes around those hooks: `restoreElements` with
/// [`TextEnv`] as its environment writes exactly upstream's elements.
#[test]
fn restore_elements_with_text_env_matches_upstream() {
    let mut checked = 0;
    for case in cases() {
        let Some(recorded) = case["hooks"].as_array() else {
            continue;
        };
        if !recorded.iter().any(is_refresh) || case["call"] != "restoreElements" {
            continue;
        }
        // arrow labels need excali-editor's geometry (restore_arrow_labels.rs)
        if case["id"] == "refresh-arrow-labels" {
            continue;
        }
        checked += 1;
        let opts = &case["opts"];
        let options = RestoreElementsOptions {
            refresh_dimensions: opts["refreshDimensions"] == true,
            repair_bindings: opts["repairBindings"] == true,
            delete_invisible_elements: opts["deleteInvisibleElements"] == true,
        };
        let inner = StickyReplay {
            test: TestEnv::default(),
            layouts: recorded
                .iter()
                .filter(|h| h["hook"] == "getStickyNoteLayout")
                .cloned()
                .collect(),
        };
        let mut env = TextEnv::new(inner, CharCountTextMetrics);
        let elements = case["elements"].as_array().expect("elements").clone();
        let restored = restore_elements(&elements, None, options, &mut env).expect("restores");
        assert!(env.inner.layouts.is_empty(), "{}", case["id"]);
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
    assert!(checked >= 3, "{checked} scenes");
}

/// Every other request goes to the wrapped environment.
#[test]
fn text_env_delegates_the_rest() {
    let mut env = TextEnv::new(TestEnv::default(), CharCountTextMetrics);
    assert_eq!(env.now(), 1.0);
    assert_eq!(env.random_id(), "id0");
    assert_eq!(env.random_id(), "id1");
    let mut plain = TestEnv::default();
    assert_eq!(env.random_integer(), plain.random_integer());
}

// -- the function itself ---------------------------------------------------------------

fn refresh(
    text: &Element,
    container: Option<&Element>,
    new_text: Option<&str>,
    max_width: Option<f64>,
    geometry: &mut dyn ArrowLabelGeometry,
) -> Option<RefreshedText> {
    refresh_in(text, container, &[], new_text, max_width, geometry)
}

/// [`refresh`] in a scene of `elements` (upstream's `elementsMap`).
fn refresh_in(
    text: &Element,
    container: Option<&Element>,
    elements: &[Element],
    new_text: Option<&str>,
    max_width: Option<f64>,
    geometry: &mut dyn ArrowLabelGeometry,
) -> Option<RefreshedText> {
    let mut char_widths = CharWidthCache::new();
    let mut layout = TextLayout {
        provider: &CharCountTextMetrics,
        char_widths: &mut char_widths,
        geometry,
    };
    refresh_text_dimensions(&mut layout, text, container, elements, new_text, max_width)
}

fn free_text(fields: Value) -> Element {
    let mut base = json!({ "x": 10, "y": 20, "width": 50, "height": 25 });
    base.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    element("text", "t", base)
}

#[test]
fn anchor_ratios() {
    use excali_core::element::{TextAlign, VerticalAlign};
    assert_eq!(
        get_text_anchor_ratios(TextAlign::Left, VerticalAlign::Top),
        [0.0, 0.0]
    );
    assert_eq!(
        get_text_anchor_ratios(TextAlign::Center, VerticalAlign::Middle),
        [0.5, 0.5]
    );
    assert_eq!(
        get_text_anchor_ratios(TextAlign::Right, VerticalAlign::Bottom),
        [1.0, 1.0]
    );
}

/// A deleted text gets `undefined`: nothing to assign.
#[test]
fn deleted_text_is_left_alone() {
    let text = free_text(json!({ "isDeleted": true }));
    assert_eq!(refresh(&text, None, None, None, &mut NoArrowGeometry), None);
}

/// The text passed replaces the element's.
#[test]
fn explicit_text() {
    let text = free_text(json!({ "text": "hello" }));
    let got = refresh(&text, None, Some("hi\nthere"), None, &mut NoArrowGeometry).unwrap();
    assert_eq!(got.text, "hi\nthere");
    assert_eq!((got.width, got.height), (50.0, 50.0));
    assert_eq!(got.auto_resize, None);
}

/// Crossing `maxWidth`, a growing free text stops growing: it wraps at
/// `maxWidth` and turns `autoResize` off.
#[test]
fn max_width_starts_wrapping() {
    let text = free_text(json!({ "text": "hello world", "originalText": "hello world" }));
    let got = refresh(&text, None, None, Some(60.0), &mut NoArrowGeometry).unwrap();
    assert_eq!(
        got,
        RefreshedText {
            text: "hello\nworld".into(),
            auto_resize: Some(false),
            width: 60.0,
            height: 50.0,
            x: 10.0,
            y: 20.0,
        }
    );
    let map = got.to_map();
    assert_eq!(
        map.keys().collect::<Vec<_>>(),
        ["text", "autoResize", "width", "height", "x", "y"]
    );
    assert_eq!(
        Value::Object(map),
        json!({ "text": "hello\nworld", "autoResize": false,
        "width": 60, "height": 50, "x": 10, "y": 20 })
    );
}

/// Centred in both directions, the growth is split around the centre,
/// measured against the element's current text.
#[test]
fn max_width_centered_anchor() {
    let text = free_text(json!({
        "text": "hello world", "originalText": "hello world",
        "textAlign": "center", "verticalAlign": "middle"
    }));
    let got = refresh(&text, None, None, Some(60.0), &mut NoArrowGeometry).unwrap();
    // prev 110 x 25, next 60 x 50: x - (60 - 110) / 2, y - (50 - 25) / 2
    assert_eq!((got.x, got.y), (35.0, 7.5));
}

/// Within `maxWidth`, or already wider than it, the text keeps growing.
#[test]
fn max_width_not_crossed() {
    let text = free_text(json!({ "text": "hello", "originalText": "hello" }));
    let got = refresh(&text, None, None, Some(60.0), &mut NoArrowGeometry).unwrap();
    assert_eq!(
        (got.text.as_str(), got.width, got.auto_resize),
        ("hello", 50.0, None)
    );

    let wide = free_text(json!({ "text": "hello world", "width": 80 }));
    let got = refresh(&wide, None, None, Some(60.0), &mut NoArrowGeometry).unwrap();
    assert_eq!((got.text.as_str(), got.width), ("hello world", 110.0));
}

/// A fixed-width text wraps to its own width and keeps it.
#[test]
fn fixed_width_wraps_to_its_width() {
    let text = free_text(json!({ "text": "hello world", "autoResize": false, "width": 60 }));
    let got = refresh(&text, None, None, None, &mut NoArrowGeometry).unwrap();
    assert_eq!(
        (got.text.as_str(), got.width, got.height),
        ("hello\nworld", 60.0, 50.0)
    );
}

/// A text in a rectangle wraps to the container's inner width.
#[test]
fn bound_text_wraps_to_container() {
    let rect = element("rectangle", "r", json!({ "width": 70 }));
    let text = free_text(json!({ "text": "hello world", "containerId": "r" }));
    let got = refresh(&text, Some(&rect), None, None, &mut NoArrowGeometry).unwrap();
    assert_eq!((got.text.as_str(), got.width), ("hello\nworld", 50.0));
}

/// An arrow label's box is where the arrow puts it
/// (`LinearElementEditor.getBoundTextElementPosition`), which the geometry
/// answers; without it there is no answer. The arrow is found in the scene
/// (`getContainerElement(element, elementsMap)`), not from the `container`
/// argument.
#[test]
fn arrow_label_uses_the_geometry() {
    let arrow = element("arrow", "a", json!({}));
    let text = arrow_label();
    let scene = [arrow.clone(), text.clone()];
    let got = refresh_in(
        &text,
        Some(&arrow),
        &scene,
        None,
        None,
        &mut At([100.0, 200.0]),
    )
    .unwrap();
    // max width max(0.7 * 100, 220): no wrap; 20 x 25.
    // x: 0 + (100 - 0) / 2 + (150 - 20) / 2; y: 0 + (200 - 0) / 2 + (225 - 25) / 2
    assert_eq!(
        (got.text.as_str(), got.width, got.height, got.x, got.y),
        ("hi", 20.0, 25.0, 115.0, 200.0)
    );
    assert_eq!(
        refresh_in(
            &text,
            Some(&arrow),
            &scene,
            None,
            None,
            &mut NoArrowGeometry
        ),
        None
    );
}

/// An arrow container missing from the scene is no container to
/// `getElementAbsoluteCoords` (`bounds.ts:259-277` through
/// `getContainerElement`, `textElement.ts:357-371`): the box is the text's
/// own x/y, whatever the geometry would say, and there is an answer without
/// one.
#[test]
fn arrow_label_outside_the_scene_uses_its_own_box() {
    let arrow = element("arrow", "a", json!({}));
    let text = arrow_label();
    // x1, y1 = 0, 0: x: 0 + (0 - 0) / 2 + (50 - 20) / 2; y: 0 + 0 + (25 - 25) / 2
    let expected = ("hi", 20.0, 25.0, 15.0, 0.0);
    for scene in [&[][..], std::slice::from_ref(&text)] {
        let got = refresh_in(
            &text,
            Some(&arrow),
            scene,
            None,
            None,
            &mut At([100.0, 200.0]),
        )
        .unwrap();
        assert_eq!(
            (got.text.as_str(), got.width, got.height, got.x, got.y),
            expected
        );
        let got = refresh_in(&text, Some(&arrow), scene, None, None, &mut NoArrowGeometry).unwrap();
        assert_eq!(
            (got.text.as_str(), got.width, got.height, got.x, got.y),
            expected
        );
    }
}

/// A geometry that puts every arrow label at one point.
struct At([f64; 2]);

impl ArrowLabelGeometry for At {
    fn bound_text_element_position(
        &mut self,
        _arrow: &Element,
        _text: &Element,
        _elements: &[Element],
    ) -> Option<[f64; 2]> {
        Some(self.0)
    }
}

/// "hi" at 0, 0 in a 50 x 25 box, centred, labelling arrow `a`.
fn arrow_label() -> Element {
    element(
        "text",
        "t",
        json!({ "text": "hi", "originalText": "hi", "width": 50, "height": 25,
                "textAlign": "center", "verticalAlign": "middle", "containerId": "a" }),
    )
}

/// Positions that come out non-finite fall back to the element's.
#[test]
fn non_finite_position_falls_back() {
    let mut t = free_text(json!({ "text": "hello" }));
    t.base.angle = excali_core::element::Radians(f64::INFINITY);
    let got = refresh(&t, None, Some("hello world"), None, &mut NoArrowGeometry).unwrap();
    assert_eq!((got.x, got.y), (10.0, 20.0));
}
