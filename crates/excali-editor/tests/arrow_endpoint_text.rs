//! Text bound to a free arrow endpoint (ex-713) against upstream:
//! `packages/element/src/arrowEndpointText.ts`, `dragNewTextElement`
//! (`packages/element/src/dragElements.ts:227-292`) and the `arrowEndpoint`
//! option of `App.startTextEditing` (`components/App.tsx:7044-7294`).
//!
//! Fixture: `tests/fixtures/arrow-endpoint-text.json`, upstream's own
//! functions at the pinned commit (`tools/goldens/arrow-endpoint-text-fixtures.mjs`).
//! The session tests replay the geometry and binding assertions of
//! `packages/excalidraw/tests/arrowEndpointTextBinding.test.tsx` through
//! [`start_text_editing`]. Text measures 10 px per UTF-16 code unit, as in
//! upstream's tests.

use std::collections::VecDeque;

use excali_core::app_state::AppState;
use excali_core::element::{
    Element, ElementBase, ElementKind, LinearFields, TextAlign, VerticalAlign,
};
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use excali_editor::arrow_endpoint_text::{
    drag_new_text_element, get_endpoint_bound_text_drag_anchor,
    get_text_binding_for_arrow_endpoint, get_unbound_arrow_endpoint_at_point,
    is_endpoint_bound_text, ArrowEndpoint, TEXT_AUTOWRAP_THRESHOLD,
};
use excali_editor::binding::{update_bound_elements, BindingEnd, BindingEnv};
use excali_editor::scene::{MutationEnv, Scene};
use excali_editor::session::Session;
use excali_editor::store::HistoryEnv;
use excali_editor::text_editing::{
    start_text_editing, StartTextEditing, TextEditingContext, TextEditingHost, TextEditor,
    TextTarget,
};
use excali_editor::text_layout::TextLayouter;
use excali_scene::bounds::ElementsMap;
use excali_text::text_measurements::{CharCountTextMetrics, CharWidthCache, TextMetricsProvider};
use serde_json::{json, Map, Value};

const NOW: f64 = 1_700_000_000_000.0;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/arrow-endpoint-text.json")).unwrap()
}

fn element(value: &Value) -> Element {
    Element::from_map(value.as_object().unwrap().clone()).unwrap()
}

fn elements(value: &Value) -> Vec<Element> {
    value.as_array().unwrap().iter().map(element).collect()
}

fn end(value: &Value) -> BindingEnd {
    match value.as_str().unwrap() {
        "start" => BindingEnd::Start,
        "end" => BindingEnd::End,
        other => panic!("{other}"),
    }
}

fn end_name(end: BindingEnd) -> &'static str {
    match end {
        BindingEnd::Start => "start",
        BindingEnd::End => "end",
    }
}

fn point(value: &Value) -> [f64; 2] {
    [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
}

fn align_name(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

fn vertical_name(align: VerticalAlign) -> &'static str {
    match align {
        VerticalAlign::Top => "top",
        VerticalAlign::Middle => "middle",
        VerticalAlign::Bottom => "bottom",
    }
}

// -- the fixture -------------------------------------------------------------------------

#[test]
fn free_endpoint_under_the_pointer_matches_upstream() {
    let f = fixture();
    let cases = f["endpointAt"].as_array().unwrap();
    assert!(cases.len() >= 30);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let els = elements(&case["elements"]);
        let map = ElementsMap::new(els.iter());
        let found = get_unbound_arrow_endpoint_at_point(
            point(&case["pointer"]),
            &els,
            &map,
            case["zoom"].as_f64().unwrap(),
        );
        let got =
            found.map(|e| json!({ "arrow": e.arrow_id, "startOrEnd": end_name(e.start_or_end) }));
        assert_eq!(got.unwrap_or(Value::Null), case["result"], "{id}");
    }
}

#[test]
fn text_binding_for_an_endpoint_matches_upstream() {
    let f = fixture();
    for case in f["bindings"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let arrow = element(&case["arrow"]);
        let map = ElementsMap::new([&arrow]);
        let got = get_text_binding_for_arrow_endpoint(
            &arrow,
            end(&case["startOrEnd"]),
            &map,
            case["targetStrokeWidth"].as_f64().unwrap(),
        );
        let expected = &case["result"];
        let Some(got) = got else {
            assert!(expected.is_null(), "{id}: expected {expected}");
            continue;
        };
        assert!(!expected.is_null(), "{id}: expected null");
        assert_eq!(got.fixed_point, point(&expected["fixedPoint"]), "{id}");
        assert_eq!(align_name(got.text_align), expected["textAlign"], "{id}");
        assert_eq!(
            vertical_name(got.vertical_align),
            expected["verticalAlign"],
            "{id}"
        );
        // bit for bit: the same operations in the same order
        assert_eq!(got.anchor, point(&expected["anchor"]), "{id}");
    }
}

#[test]
fn endpoint_bound_text_matches_upstream() {
    let f = fixture();
    for case in f["boundText"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let els = elements(&case["elements"]);
        let map = ElementsMap::new(els.iter());
        let text = map.get(case["text"].as_str().unwrap()).unwrap();
        assert_eq!(
            is_endpoint_bound_text(text, &map),
            case["result"].as_bool().unwrap(),
            "{id}"
        );
    }
}

#[test]
fn drag_anchor_matches_upstream() {
    let f = fixture();
    for case in f["dragAnchors"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let text = element(&case["text"]);
        let got = get_endpoint_bound_text_drag_anchor(&text);
        assert_eq!(
            got.anchor_ratio,
            case["result"]["anchorRatio"].as_f64().unwrap(),
            "{id}"
        );
        assert_eq!(
            got.anchor_x,
            case["result"]["anchorX"].as_f64().unwrap(),
            "{id}"
        );
    }
}

#[test]
fn drag_new_text_element_matches_upstream() {
    let f = fixture();
    for case in f["drags"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let text = element(&case["text"]);
        let got = drag_new_text_element(
            &text,
            case["anchorX"].as_f64().unwrap(),
            case["anchorRatio"].as_f64().unwrap(),
            case["pointerX"].as_f64().unwrap(),
            case["nextY"].as_f64(),
            case["zoom"].as_f64().unwrap(),
            &CharCountTextMetrics,
        );
        let mut out = Map::new();
        out.insert("x".into(), json!(got.x));
        if let Some(y) = got.y {
            out.insert("y".into(), json!(y));
        }
        out.insert("width".into(), json!(got.width));
        if let Some(auto_resize) = got.auto_resize {
            out.insert("autoResize".into(), json!(auto_resize));
        }
        let expected = case["result"].as_object().unwrap();
        assert_eq!(out.len(), expected.len(), "{id}: {out:?} vs {expected:?}");
        for (key, value) in expected {
            let got = &out[key];
            match value.as_f64() {
                Some(n) => assert_eq!(got.as_f64(), Some(n), "{id}.{key}"),
                None => assert_eq!(got, value, "{id}.{key}"),
            }
        }
    }
}

#[test]
fn autowrap_threshold_is_upstreams() {
    // `TEXT_AUTOWRAP_THRESHOLD` (`common/src/constants.ts:24`)
    assert_eq!(TEXT_AUTOWRAP_THRESHOLD, 36.0);
}

// -- startTextEditing with an arrow endpoint ---------------------------------------------

struct Env {
    ids: VecDeque<String>,
    nonce: f64,
    char_widths: CharWidthCache,
}

impl ChangeStamp for Env {
    fn version_nonce(&mut self) -> f64 {
        self.nonce += 1.0;
        self.nonce
    }

    fn updated(&mut self) -> f64 {
        NOW
    }
}

impl HistoryEnv for Env {
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        (&CharCountTextMetrics, &mut self.char_widths)
    }

    fn random_id(&mut self) -> String {
        self.ids.pop_front().unwrap_or_else(|| "extra".into())
    }

    fn redraw_text_bounding_box(
        &mut self,
        _: &mut SceneElementsMap,
        _: &str,
        _: &str,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// The binding layout of `updateBoundElements`, so the arrow follows the
/// text as upstream's does.
struct Binder {
    nonce: f64,
    char_widths: CharWidthCache,
}

impl MutationEnv for Binder {
    fn random_integer(&mut self) -> f64 {
        self.nonce += 1.0;
        self.nonce
    }

    fn now(&mut self) -> f64 {
        NOW
    }
}

impl BindingEnv for Binder {
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        (&CharCountTextMetrics, &mut self.char_widths)
    }
}

struct Host {
    hit_text: Option<String>,
}

impl TextEditingHost for Host {
    fn text_element_at(&self, _: &[Element], _: f64, _: f64) -> Option<String> {
        self.hit_text.clone()
    }

    fn update_bound_elements(
        &mut self,
        _: &mut dyn ChangeStamp,
        elements: &mut SceneElementsMap,
        id: &str,
    ) -> Result<(), String> {
        let mut scene = Scene::new(elements.values().cloned().collect());
        let mut env = Binder {
            nonce: 0.0,
            char_widths: CharWidthCache::new(),
        };
        update_bound_elements(&mut scene, &mut env, id, None, None);
        for element in scene.elements() {
            if let Some(slot) = elements.get_mut(&element.base.id) {
                if slot != element {
                    *slot = element.clone();
                }
            }
        }
        Ok(())
    }
}

struct App {
    session: Session<Env>,
    layouter: TextLayouter<CharCountTextMetrics>,
    host: Host,
}

impl App {
    fn new(elements: Vec<Element>, state: Value) -> App {
        let env = Env {
            ids: (0..8).map(|i| format!("text{i}")).collect(),
            nonce: 0.0,
            char_widths: CharWidthCache::new(),
        };
        let mut session = Session::new(env, AppState::default());
        session
            .initialize_scene(elements, state.as_object().unwrap().clone())
            .unwrap();
        App {
            session,
            layouter: TextLayouter::new(CharCountTextMetrics),
            host: Host { hit_text: None },
        }
    }

    fn ctx(&mut self) -> TextEditingContext<'_, Env, CharCountTextMetrics> {
        TextEditingContext {
            session: &mut self.session,
            layouter: &mut self.layouter,
            host: &mut self.host,
        }
    }

    fn get(&self, id: &str) -> Element {
        self.session
            .elements()
            .iter()
            .find(|e| e.base.id == id)
            .unwrap()
            .clone()
    }

    fn texts(&self) -> Vec<Element> {
        self.session
            .elements()
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Text(_)) && !e.base.is_deleted)
            .cloned()
            .collect()
    }

    /// The text tool's click at (x, y) (`bindTextAt`,
    /// `arrowEndpointTextBinding.test.tsx:79-86`): the endpoint under the
    /// pointer resolved as `AppTextTool` does, then `startTextEditing`.
    fn click_text_tool(&mut self, x: f64, y: f64, auto_edit: bool) -> Option<TextEditor> {
        let elements: Vec<Element> = self
            .session
            .elements()
            .iter()
            .filter(|e| !e.base.is_deleted)
            .cloned()
            .collect();
        let map = ElementsMap::new(elements.iter());
        let endpoint = get_unbound_arrow_endpoint_at_point([x, y], &elements, &map, 1.0);
        let mut args = StartTextEditing::at(x, y);
        args.auto_edit = auto_edit;
        args.arrow_endpoint = endpoint;
        start_text_editing(&mut self.ctx(), &args).unwrap()
    }

    fn bind_text_at(&mut self, x: f64, y: f64, text: &str) -> TextEditor {
        let mut editor = self.click_text_tool(x, y, true).unwrap();
        self.type_in(&mut editor, text);
        editor
    }

    fn type_in(&mut self, editor: &mut TextEditor, text: &str) {
        let len = text.encode_utf16().count();
        let mut ctx = TextEditingContext {
            session: &mut self.session,
            layouter: &mut self.layouter,
            host: &mut self.host,
        };
        editor.input(&mut ctx, text, (len, len)).unwrap();
    }
}

/// `createArrow(id, [startX, startY], [endX, endY])`
/// (`arrowEndpointTextBinding.test.tsx:27-47`).
fn create_arrow(id: &str, [sx, sy]: [f64; 2], [ex, ey]: [f64; 2]) -> Element {
    let mut base = ElementBase::new(id, sx, sy, 1.0, NOW);
    base.width = (ex - sx).abs();
    base.height = (ey - sy).abs();
    Element::new(
        base,
        ElementKind::Arrow(excali_core::element::ArrowFields::new(
            LinearFields::new(vec![[0.0, 0.0], [ex - sx, ey - sy]]),
            false,
        )),
    )
}

/// `endpointOf(arrow, which)` (`arrowEndpointTextBinding.test.tsx:60-63`).
fn endpoint_of(arrow: &Element, which: BindingEnd) -> [f64; 2] {
    let ElementKind::Arrow(a) = &arrow.kind else {
        panic!("not an arrow")
    };
    let p = match which {
        BindingEnd::Start => a.linear.points[0],
        BindingEnd::End => *a.linear.points.last().unwrap(),
    };
    [arrow.base.x + p[0], arrow.base.y + p[1]]
}

/// `expectPointsClose` (`arrowEndpointTextBinding.test.tsx:70-76`):
/// `toBeCloseTo(_, 1)`.
fn assert_points_close(actual: [f64; 2], expected: [f64; 2]) {
    for i in 0..2 {
        assert!(
            (actual[i] - expected[i]).abs() < 0.05,
            "{actual:?} vs {expected:?}"
        );
    }
}

fn binding_of(arrow: &Element, which: BindingEnd) -> Option<Value> {
    let ElementKind::Arrow(a) = &arrow.kind else {
        panic!("not an arrow")
    };
    let b = match which {
        BindingEnd::Start => &a.linear.start_binding,
        BindingEnd::End => &a.linear.end_binding,
    };
    b.as_ref().map(|b| serde_json::to_value(b).unwrap())
}

fn text_fields(text: &Element) -> &excali_core::element::TextFields {
    match &text.kind {
        ElementKind::Text(t) => t,
        _ => panic!("not a text"),
    }
}

fn state() -> Value {
    json!({})
}

/// `arrowEndpointTextBinding.test.tsx:255-306` ("placement strategy").
#[test]
fn placement_strategy_binds_the_side_the_arrow_points_at() {
    let cases = [
        (
            [100.0, 300.0],
            [100.0, 100.0],
            [0.5001, 1.0],
            "center",
            "bottom",
        ),
        (
            [100.0, 100.0],
            [300.0, 100.0],
            [0.0, 0.5001],
            "left",
            "middle",
        ),
        (
            [100.0, 100.0],
            [100.0, 300.0],
            [0.5001, 0.0],
            "center",
            "top",
        ),
        (
            [300.0, 100.0],
            [100.0, 100.0],
            [1.0, 0.5001],
            "right",
            "middle",
        ),
    ];
    for (from, to, fixed_point, text_align, vertical_align) in cases {
        let mut app = App::new(vec![create_arrow("arrow", from, to)], state());
        app.bind_text_at(to[0], to[1], "label");
        let [text] = app.texts().try_into().unwrap();
        assert_eq!(
            binding_of(&app.get("arrow"), BindingEnd::End),
            Some(json!({ "elementId": text.base.id, "fixedPoint": fixed_point, "mode": "orbit" }))
        );
        let t = text_fields(&text);
        assert_eq!(align_name(t.text_align), text_align);
        assert_eq!(vertical_name(t.vertical_align), vertical_align);
        assert_eq!(t.container_id, None);
        assert_eq!(
            serde_json::to_value(&text.base.bound_elements).unwrap(),
            json!([{ "id": "arrow", "type": "arrow" }])
        );
    }
}

/// `arrowEndpointTextBinding.test.tsx:308-321`.
#[test]
fn binds_the_start_point_under_the_cursor() {
    let mut app = App::new(
        vec![create_arrow("arrow", [100.0, 100.0], [300.0, 100.0])],
        state(),
    );
    app.bind_text_at(100.0, 100.0, "label");
    let [text] = app.texts().try_into().unwrap();
    let arrow = app.get("arrow");
    let start = binding_of(&arrow, BindingEnd::Start).unwrap();
    assert_eq!(start["elementId"], json!(text.base.id));
    assert_eq!(binding_of(&arrow, BindingEnd::End), None);
    assert_eq!(start["fixedPoint"], json!([1.0, 0.5001]));
    assert_eq!(text_fields(&text).text_align, TextAlign::Right);
}

/// `arrowEndpointTextBinding.test.tsx:325-335`.
#[test]
fn the_arrow_does_not_move_when_the_text_is_created_and_typed_into() {
    let mut app = App::new(
        vec![create_arrow("arrow", [100.0, 300.0], [100.0, 100.0])],
        state(),
    );
    let tip_before = endpoint_of(&app.get("arrow"), BindingEnd::End);
    let mut editor = app.bind_text_at(100.0, 100.0, "a label");
    assert_points_close(endpoint_of(&app.get("arrow"), BindingEnd::End), tip_before);
    let version = app.get("arrow").base.version;
    app.type_in(&mut editor, "a much longer label\nspanning two lines");
    // the arrow was laid out again (updateBoundElements) and stayed put
    assert!(app.get("arrow").base.version > version);
    assert_points_close(endpoint_of(&app.get("arrow"), BindingEnd::End), tip_before);
}

/// `arrowEndpointTextBinding.test.tsx:337-350`.
#[test]
fn the_bound_side_midpoint_stays_pinned_as_the_text_grows() {
    let mut app = App::new(
        vec![create_arrow("arrow", [100.0, 300.0], [100.0, 100.0])],
        state(),
    );
    let mut editor = app.bind_text_at(100.0, 100.0, "short");
    let bottom_mid = |t: &Element| [t.base.x + t.base.width / 2.0, t.base.y + t.base.height];
    let anchor_before = bottom_mid(&app.texts()[0]);
    app.type_in(&mut editor, "a much longer label\nspanning two lines");
    let text = app.texts()[0].clone();
    assert!(text.base.width > 0.0);
    assert_points_close(bottom_mid(&text), anchor_before);
}

/// `arrowEndpointTextBinding.test.tsx:357-370`: diagonal arrows keep their
/// tip.
#[test]
fn a_diagonal_arrow_tip_does_not_shift() {
    for from in [[100.0, 100.0], [160.0, 100.0], [100.0, 160.0]] {
        let mut app = App::new(vec![create_arrow("arrow", from, [300.0, 300.0])], state());
        let tip_before = endpoint_of(&app.get("arrow"), BindingEnd::End);
        let mut editor = app.bind_text_at(300.0, 300.0, "x");
        assert_points_close(endpoint_of(&app.get("arrow"), BindingEnd::End), tip_before);
        app.type_in(&mut editor, "a considerably longer label");
        assert_points_close(endpoint_of(&app.get("arrow"), BindingEnd::End), tip_before);
    }
}

/// `arrowEndpointTextBinding.test.tsx:375-397`: the gap is the text's
/// stroke width's, not the arrow's.
#[test]
fn the_tip_does_not_shift_when_the_text_stroke_width_differs() {
    for (arrow_stroke, text_stroke) in [(2.0, "bold"), (4.0, "thin"), (1.0, "medium")] {
        let mut arrow = create_arrow("arrow", [100.0, 300.0], [100.0, 100.0]);
        arrow.base.stroke_width = arrow_stroke;
        let mut app = App::new(
            vec![arrow],
            json!({ "currentItemStrokeWidthKey": text_stroke }),
        );
        let tip_before = endpoint_of(&app.get("arrow"), BindingEnd::End);
        let mut editor = app.bind_text_at(100.0, 100.0, "x");
        assert_ne!(app.texts()[0].base.stroke_width, arrow_stroke);
        assert_points_close(endpoint_of(&app.get("arrow"), BindingEnd::End), tip_before);
        app.type_in(&mut editor, "a considerably longer label");
        assert_points_close(endpoint_of(&app.get("arrow"), BindingEnd::End), tip_before);
    }
}

/// `dragTextAt` (`arrowEndpointTextBinding.test.tsx:402-418`) as
/// `App.arrowText.maybeDragNewText` drives it: the text created without
/// the editor, its width dragged out from the binding's anchor, then the
/// editor opened on it.
fn drag_text_at(app: &mut App, [x, y]: [f64; 2], to_x: f64) -> Element {
    assert!(app.click_text_tool(x, y, false).is_none());
    let text = app.texts()[0].clone();
    let elements = app.session.elements().to_vec();
    let map = ElementsMap::new(elements.iter());
    assert!(is_endpoint_bound_text(&text, &map));
    let anchor = get_endpoint_bound_text_drag_anchor(&text);
    let mut current = text.clone();
    for i in 1..=4 {
        let pointer_x = x + (to_x - x) * i as f64 / 4.0;
        let drag = drag_new_text_element(
            &current,
            anchor.anchor_x,
            anchor.anchor_ratio,
            pointer_x,
            None,
            1.0,
            &CharCountTextMetrics,
        );
        current.base.x = drag.x;
        current.base.width = drag.width;
        if let (Some(auto_resize), ElementKind::Text(t)) = (drag.auto_resize, &mut current.kind) {
            t.auto_resize = auto_resize;
        }
    }
    current
}

/// `arrowEndpointTextBinding.test.tsx:420-432`.
#[test]
fn drag_sizing_sets_a_fixed_width_without_moving_the_anchor() {
    let mut app = App::new(
        vec![create_arrow("arrow", [100.0, 100.0], [300.0, 100.0])],
        state(),
    );
    let text = drag_text_at(&mut app, [300.0, 100.0], 520.0);
    let t = text_fields(&text);
    assert!(!t.auto_resize);
    assert_eq!(t.text_align, TextAlign::Left);
    assert!((text.base.x - 306.0).abs() < 0.5, "{}", text.base.x);
    assert!((text.base.width - 214.0).abs() < 0.5, "{}", text.base.width);
}

/// `arrowEndpointTextBinding.test.tsx:434-444`.
#[test]
fn drag_sizing_grows_leftwards_for_a_right_bound_text() {
    let mut app = App::new(
        vec![create_arrow("arrow", [500.0, 100.0], [300.0, 100.0])],
        state(),
    );
    let text = drag_text_at(&mut app, [300.0, 100.0], 80.0);
    let t = text_fields(&text);
    assert!(!t.auto_resize);
    assert_eq!(t.text_align, TextAlign::Right);
    assert!((text.base.x + text.base.width - 294.0).abs() < 0.5);
}

/// `arrowEndpointTextBinding.test.tsx:446-455`.
#[test]
fn drag_sizing_keeps_a_centred_text_centred_on_the_anchor() {
    let mut app = App::new(
        vec![create_arrow("arrow", [300.0, 100.0], [300.0, 300.0])],
        state(),
    );
    let text = drag_text_at(&mut app, [300.0, 300.0], 430.0);
    let t = text_fields(&text);
    assert!(!t.auto_resize);
    assert_eq!(t.text_align, TextAlign::Center);
    assert!((text.base.x + text.base.width / 2.0 - 300.0).abs() < 0.5);
}

/// `arrowEndpointTextBinding.test.tsx:457-467`.
#[test]
fn drag_sizing_does_not_run_back_over_the_arrow() {
    let mut app = App::new(
        vec![create_arrow("arrow", [100.0, 100.0], [300.0, 100.0])],
        state(),
    );
    let text = drag_text_at(&mut app, [300.0, 100.0], 150.0);
    assert!(text_fields(&text).auto_resize);
    assert!((text.base.x - 306.0).abs() < 0.5);
}

/// `arrowEndpointTextBinding.test.tsx:825-851`: a fresh text instead of
/// adopting the selected one (and :853-888, one under the pointer).
#[test]
fn creates_a_fresh_text_instead_of_adopting_a_selected_or_hit_one() {
    let faraway = excali_core::element::Element::from_map(
        json!({
            "id": "faraway", "type": "text", "x": 500, "y": 500, "width": 70, "height": 25,
            "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
            "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
            "opacity": 100, "groupIds": [], "frameId": null, "index": "a1", "roundness": null,
            "seed": 1, "version": 1, "versionNonce": 0, "isDeleted": false,
            "boundElements": null, "updated": NOW, "link": null, "locked": false,
            "text": "faraway", "fontSize": 20, "fontFamily": 5, "textAlign": "left",
            "verticalAlign": "top", "containerId": null, "originalText": "faraway",
            "autoResize": true, "lineHeight": 1.25
        })
        .as_object()
        .unwrap()
        .clone(),
    )
    .unwrap();
    let mut app = App::new(
        vec![
            create_arrow("arrow", [100.0, 300.0], [100.0, 100.0]),
            faraway,
        ],
        json!({ "selectedElementIds": { "faraway": true } }),
    );
    app.host.hit_text = Some("faraway".into());
    app.bind_text_at(100.0, 100.0, "bound");
    let bound = binding_of(&app.get("arrow"), BindingEnd::End).unwrap();
    assert_ne!(bound["elementId"], json!("faraway"));
    let faraway = app.get("faraway");
    assert_eq!(text_fields(&faraway).text, "faraway");
    assert!(faraway.base.bound_elements.unwrap_or_default().is_empty());
}

/// Without an endpoint, `startTextEditing` is unchanged: the arrow stays
/// unbound (`arrowEndpointTextBinding.test.tsx:677-690`, the free text
/// case).
#[test]
fn no_endpoint_no_binding() {
    let mut app = App::new(
        vec![create_arrow("arrow", [100.0, 300.0], [100.0, 100.0])],
        state(),
    );
    let mut args = StartTextEditing::at(400.0, 400.0);
    args.text_element = TextTarget::New;
    assert_eq!(args.arrow_endpoint, None);
    start_text_editing(&mut app.ctx(), &args).unwrap();
    let arrow = app.get("arrow");
    assert_eq!(binding_of(&arrow, BindingEnd::End), None);
    assert_eq!(binding_of(&arrow, BindingEnd::Start), None);
    let _ = ArrowEndpoint {
        arrow_id: "arrow".into(),
        start_or_end: BindingEnd::End,
    };
}
