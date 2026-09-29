//! Text element sizing on edit against upstream's own output:
//! `tests/fixtures/text-element-sizing.json`, written by
//! `tools/goldens/text-element-sizing.mjs` from `newTextElement`,
//! `refreshTextDimensions` and `getTextAnchorRatios`
//! (`packages/element/src/newElement.ts:297-581`), `getBoundTextMaxWidth`
//! (`packages/element/src/textElement.ts:540-569`) and the autoResize action
//! (`packages/excalidraw/actions/actionTextAutoResize.ts`) at the pinned
//! commit, under two line-width providers.
//!
//! An arrow label's box is where the arrow puts it; the fixture records
//! that point (`labelOrigin`) and the test hands it to
//! `refresh_text_dimensions` through an [`ArrowLabelGeometry`], the way
//! `excali-editor` will.
//!
//! Geometry is compared exactly, rotated text included: its position goes
//! through `Math.cos`/`Math.sin`, which `excali_math::js` computes as V8
//! does on every platform (ex-009).

use excali_core::element::{Element, FontFamily, TextAlign, TextFields, VerticalAlign};
use excali_text::new_element::{
    get_text_anchor_ratios, new_text_element, refresh_text_dimensions, text_auto_resize,
    NewTextElementOptions, RefreshedText, TextLayout,
};
use excali_text::text_element::{get_bound_text_max_width, ArrowLabelGeometry, NoArrowGeometry};
use excali_text::text_measurements::{CharCountTextMetrics, CharWidthCache, TextMetricsProvider};
use serde_json::{Map, Value};

fn goldens() -> &'static Value {
    static GOLDENS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    GOLDENS.get_or_init(|| {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/text-element-sizing.json"
        );
        serde_json::from_str(&std::fs::read_to_string(path).expect("text-element-sizing.json"))
            .expect("text-element-sizing.json parses")
    })
}

/// The fixture's `scaled` metric: each UTF-16 code unit `u` is
/// `3 + (u * 7) % 11` wide, less 0.5 for every adjacent pair whose sum is a
/// multiple of 5, the sum scaled by the font size over 20.
struct ScaledMetrics;

impl TextMetricsProvider for ScaledMetrics {
    fn get_line_width(&self, text: &str, font: &str) -> f64 {
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut width = 0.0;
        for (i, &u) in units.iter().enumerate() {
            width += f64::from(3 + (u32::from(u) * 7) % 11);
            if i > 0 && (u32::from(units[i - 1]) + u32::from(u)) % 5 == 0 {
                width -= 0.5;
            }
        }
        width * excali_core::json::parse_float(font) / 20.0
    }
}

fn provider(metric: &str) -> &'static dyn TextMetricsProvider {
    match metric {
        "chars10" => &CharCountTextMetrics,
        "scaled" => &ScaledMetrics,
        other => panic!("unknown metric {other}"),
    }
}

/// An arrow that puts its label at a fixed point, recording what it was
/// asked to place.
struct LabelAt {
    origin: Option<[f64; 2]>,
    asked: Vec<(String, String)>,
}

impl ArrowLabelGeometry for LabelAt {
    fn bound_text_element_position(
        &mut self,
        arrow: &Element,
        text: &Element,
        _elements: &[Element],
    ) -> Option<[f64; 2]> {
        self.asked
            .push((arrow.base.id.clone(), text.base.id.clone()));
        self.origin
    }
}

/// `refreshTextDimensions(text, container, elements, nextText, maxWidth)`
/// with no arrow geometry and a fresh width cache.
fn refresh(
    provider: &dyn TextMetricsProvider,
    text: &Element,
    container: Option<&Element>,
    next_text: Option<&str>,
    max_width: Option<f64>,
) -> Option<RefreshedText> {
    let mut char_widths = CharWidthCache::default();
    let mut geometry = NoArrowGeometry;
    let mut layout = TextLayout {
        provider,
        char_widths: &mut char_widths,
        geometry: &mut geometry,
    };
    let elements: Vec<Element> = container.into_iter().chain([text]).cloned().collect();
    refresh_text_dimensions(
        &mut layout,
        text,
        container,
        &elements,
        next_text,
        max_width,
    )
}

fn num(value: &Value) -> f64 {
    value.as_f64().unwrap_or_else(|| panic!("number: {value}"))
}

fn template(name: &str) -> Map<String, Value> {
    goldens()["templates"][name]
        .as_object()
        .unwrap_or_else(|| panic!("template {name}"))
        .clone()
}

/// The text template with a case's fields laid over it.
fn text_element(fields: &Value) -> Element {
    let mut map = template("text");
    for (k, v) in fields.as_object().expect("text fields") {
        map.insert(k.clone(), v.clone());
    }
    Element::from_map(map).expect("text element")
}

fn container(name: &str) -> Element {
    let mut map = template(name);
    map.insert("id".into(), Value::from(name));
    Element::from_map(map).unwrap_or_else(|e| panic!("container {name}: {e}"))
}

fn text_align(value: &Value) -> TextAlign {
    serde_json::from_value(value.clone()).expect("textAlign")
}

fn vertical_align(value: &Value) -> VerticalAlign {
    serde_json::from_value(value.clone()).expect("verticalAlign")
}

fn text_fields(element: &Element) -> &TextFields {
    match &element.kind {
        excali_core::element::ElementKind::Text(t) => t,
        _ => panic!("not a text"),
    }
}

fn assert_num(what: &str, actual: f64, expected: f64) {
    assert!(
        actual == expected || (actual.is_nan() && expected.is_nan()),
        "{what}: {actual} != {expected}"
    );
}

/// `update` against the keys upstream assigned (`result`).
fn assert_update(what: &str, update: &RefreshedText, expected: &Value) {
    assert_eq!(
        update.text,
        expected["text"].as_str().expect("text"),
        "{what}: text"
    );
    assert_eq!(
        update.auto_resize,
        expected
            .get("autoResize")
            .map(|v| v.as_bool().expect("autoResize")),
        "{what}: autoResize"
    );
    for (key, actual) in [
        ("width", update.width),
        ("height", update.height),
        ("x", update.x),
        ("y", update.y),
    ] {
        assert_num(&format!("{what}: {key}"), actual, num(&expected[key]));
    }
}

#[test]
fn anchor_ratios_follow_the_alignment() {
    let anchors = goldens()["anchors"].as_array().expect("anchors");
    assert_eq!(anchors.len(), 9);
    for a in anchors {
        let ratios = get_text_anchor_ratios(
            text_align(&a["textAlign"]),
            vertical_align(&a["verticalAlign"]),
        );
        assert_eq!(ratios[0], num(&a["ratios"]["x"]), "{a}");
        assert_eq!(ratios[1], num(&a["ratios"]["y"]), "{a}");
    }
}

#[test]
fn bound_text_max_width_per_container_type() {
    let cases = goldens()["boundTextMaxWidth"].as_array().expect("cases");
    assert!(!cases.is_empty());
    for case in cases {
        let kind = case["type"].as_str().expect("type");
        let mut map = template(if kind == "embeddable" {
            "rectangle"
        } else {
            kind
        });
        map.insert("type".into(), Value::from(kind));
        map.insert("width".into(), case["width"].clone());
        let container = Element::from_map(map).expect("container");
        let text = case["fontSize"]
            .as_f64()
            .map(|font_size| text_element(&serde_json::json!({ "fontSize": font_size })));
        assert_eq!(
            get_bound_text_max_width(&container, text.as_ref()),
            num(&case["maxWidth"]),
            "{case}"
        );
    }
}

fn create_options(opts: &Value) -> NewTextElementOptions {
    let get = |k: &str| opts.get(k);
    NewTextElementOptions {
        text: get("text")
            .and_then(Value::as_str)
            .expect("text")
            .to_owned(),
        original_text: get("originalText").map(|v| v.as_str().expect("originalText").to_owned()),
        x: num(&opts["x"]),
        y: num(&opts["y"]),
        font_size: get("fontSize").map(num),
        font_family: get("fontFamily").map(|v| FontFamily(v.as_u64().expect("family") as u32)),
        text_align: get("textAlign").map(text_align),
        vertical_align: get("verticalAlign").map(vertical_align),
        container_id: get("containerId").map(|v| v.as_str().expect("containerId").to_owned()),
        line_height: get("lineHeight").map(num),
        auto_resize: get("autoResize").map(|v| v.as_bool().expect("autoResize")),
        label_position: None,
        base_font_size: get("baseFontSize").map(num),
    }
}

#[test]
fn new_text_element_positions_the_box_from_its_anchor() {
    let cases = goldens()["create"].as_array().expect("create");
    assert!(cases.len() > 400);
    for case in cases {
        let what = case["opts"].to_string();
        let created = new_text_element(
            &create_options(&case["opts"]),
            provider(case["metric"].as_str().unwrap()),
        );
        let e = &case["element"];
        for (key, actual) in [
            ("x", created.x),
            ("y", created.y),
            ("width", created.width),
            ("height", created.height),
        ] {
            assert_num(&format!("{what}: {key}"), actual, num(&e[key]));
        }
        let t = &created.fields;
        assert_eq!(t.text, e["text"].as_str().unwrap(), "{what}: text");
        assert_eq!(
            t.original_text,
            e["originalText"].as_str().unwrap(),
            "{what}: originalText"
        );
        assert_eq!(t.font_size, num(&e["fontSize"]), "{what}: fontSize");
        assert_eq!(
            u64::from(t.font_family.0),
            e["fontFamily"].as_u64().unwrap(),
            "{what}: fontFamily"
        );
        assert_eq!(t.line_height, num(&e["lineHeight"]), "{what}: lineHeight");
        assert_eq!(
            t.text_align,
            text_align(&e["textAlign"]),
            "{what}: textAlign"
        );
        assert_eq!(
            t.vertical_align,
            vertical_align(&e["verticalAlign"]),
            "{what}: verticalAlign"
        );
        assert_eq!(
            t.auto_resize,
            e["autoResize"].as_bool().unwrap(),
            "{what}: autoResize"
        );
        assert_eq!(
            t.container_id.as_deref(),
            e["containerId"].as_str(),
            "{what}: containerId"
        );
        assert_eq!(
            t.base_font_size,
            e["baseFontSize"].as_f64(),
            "{what}: baseFontSize"
        );
        assert_eq!(t.label_position, Some(None), "{what}: labelPosition");
    }
}

#[test]
fn refresh_reproduces_upstream_geometry_for_every_edit() {
    let cases = goldens()["refresh"].as_array().expect("refresh");
    assert!(cases.len() > 2000);
    let mut modes = std::collections::BTreeSet::new();
    let mut arrow_asks = 0;
    for case in cases {
        let what = case.to_string();
        let mode = case["mode"].as_str().unwrap();
        modes.insert(mode);
        let element = text_element(&case["text"]);
        let container = case
            .get("container")
            .map(|c| container(c.as_str().unwrap()));
        let label_origin = case.get("labelOrigin").map(|o| [num(&o[0]), num(&o[1])]);
        // the scene upstream was handed: the container, then the text
        let elements: Vec<Element> = container.iter().cloned().chain([element.clone()]).collect();
        let mut char_widths = CharWidthCache::default();
        let mut geometry = LabelAt {
            origin: label_origin,
            asked: Vec::new(),
        };
        let mut layout = TextLayout {
            provider: provider(case["metric"].as_str().unwrap()),
            char_widths: &mut char_widths,
            geometry: &mut geometry,
        };
        let update = refresh_text_dimensions(
            &mut layout,
            &element,
            container.as_ref(),
            &elements,
            case.get("nextText").map(|t| t.as_str().unwrap()),
            case.get("maxWidth").map(num),
        );
        // only an arrow's label is placed by the arrow (a free, growing,
        // centred text grows about its centre and never asks)
        if label_origin.is_none() {
            assert!(geometry.asked.is_empty(), "{what}: {:?}", geometry.asked);
        } else {
            arrow_asks += geometry.asked.len();
            for (arrow, text) in &geometry.asked {
                assert_eq!(arrow, "arrow", "{what}");
                assert_eq!(text, &element.base.id, "{what}");
            }
        }
        match (&update, &case["result"]) {
            (None, Value::Null) => {}
            (Some(update), expected @ Value::Object(_)) => assert_update(&what, update, expected),
            (actual, expected) => panic!("{what}: {actual:?} != {expected}"),
        }
    }
    assert_eq!(modes.len(), 13, "{modes:?}");
    assert!(arrow_asks > 0, "no arrow label was placed by the geometry");
}

#[test]
fn typing_session_grows_and_wraps_like_the_editor() {
    let sessions = goldens()["sessions"].as_array().expect("sessions");
    assert!(!sessions.is_empty());
    for session in sessions {
        let provider = provider(session["metric"].as_str().unwrap());
        let mut element = text_element(&session["text"]);
        let typed: Vec<char> = session["typed"].as_str().unwrap().chars().collect();
        let max_width = session.get("maxWidth").map(num);
        let steps = session["steps"].as_array().unwrap();
        assert_eq!(steps.len(), typed.len());
        let mut char_widths = CharWidthCache::default();
        let mut geometry = NoArrowGeometry;
        let mut layout = TextLayout {
            provider,
            char_widths: &mut char_widths,
            geometry: &mut geometry,
        };
        for (i, expected) in steps.iter().enumerate() {
            let so_far: String = typed[..=i].iter().collect();
            let scene = [element.clone()];
            let update = refresh_text_dimensions(
                &mut layout,
                &element,
                None,
                &scene,
                Some(&so_far),
                max_width,
            )
            .expect("a live text");
            if let excali_core::element::ElementKind::Text(t) = &mut element.kind {
                t.original_text = so_far.clone();
            }
            update.apply(&mut element);
            let what = format!("{session} step {i}");
            let t = text_fields(&element);
            assert_eq!(t.text, expected["text"].as_str().unwrap(), "{what}: text");
            assert_eq!(
                t.auto_resize,
                expected["autoResize"].as_bool().unwrap(),
                "{what}: autoResize"
            );
            assert_eq!(t.original_text, so_far, "{what}: originalText");
            for (key, actual) in [
                ("x", element.base.x),
                ("y", element.base.y),
                ("width", element.base.width),
                ("height", element.base.height),
            ] {
                assert_num(&format!("{what}: {key}"), actual, num(&expected[key]));
            }
        }
    }
}

#[test]
fn auto_resize_unwraps_around_the_anchor() {
    let cases = goldens()["autoResize"].as_array().expect("autoResize");
    assert!(!cases.is_empty());
    for case in cases {
        let element = text_element(&case["text"]);
        let update =
            text_auto_resize(&element, provider(case["metric"].as_str().unwrap())).expect("a text");
        assert_update(&case.to_string(), &update, &case["result"]);
    }
}

/// `textWysiwyg.test.tsx` "keeps the $textAlign anchor when unwrapping" and
/// "keeps the $verticalAlign anchor when unwrapping": the edge the
/// alignment pins stays put while the box changes size.
#[test]
fn auto_resize_keeps_the_pinned_edge() {
    let base = serde_json::json!({
        "x": 100, "y": 100, "width": 300, "height": 50,
        "text": "this is it my friends\nald aksdl askdlasdk",
        "originalText": "this is it my friends ald aksdl askdlasdk",
        "autoResize": false,
    });
    for align in ["left", "center", "right"] {
        for valign in ["top", "middle", "bottom"] {
            let mut fields = base.clone();
            fields["textAlign"] = Value::from(align);
            fields["verticalAlign"] = Value::from(valign);
            let mut element = text_element(&fields);
            let before = element.base.clone();
            let update = text_auto_resize(&element, &CharCountTextMetrics).unwrap();
            update.apply(&mut element);
            assert!(text_fields(&element).auto_resize);
            assert!((element.base.width - 300.0).abs() > 0.5);
            let edge = |x: f64, w: f64| match align {
                "left" => x,
                "center" => x + w / 2.0,
                _ => x + w,
            };
            let vedge = |y: f64, h: f64| match valign {
                "top" => y,
                "middle" => y + h / 2.0,
                _ => y + h,
            };
            assert!(
                (edge(element.base.x, element.base.width) - edge(before.x, before.width)).abs()
                    < 1e-4
            );
            assert!(
                (vedge(element.base.y, element.base.height) - vedge(before.y, before.height)).abs()
                    < 1e-4
            );
        }
    }
}

/// `textWysiwyg.test.tsx` "should keep width when editing a wrapped text": a
/// fixed-width text keeps its width and grows taller as text is added.
#[test]
fn editing_a_wrapped_text_keeps_its_width() {
    let mut element = text_element(&serde_json::json!({
        "x": 0, "y": 0, "width": 80, "height": 50,
        "text": "Excalidraw\nEditor", "originalText": "Excalidraw\nEditor",
        "autoResize": false,
    }));
    let next = "Excalidraw\nEditor is great!";
    let update = refresh(&CharCountTextMetrics, &element, None, Some(next), None).unwrap();
    update.apply(&mut element);
    assert_eq!(element.base.width, 80.0);
    assert!(element.base.height > 50.0);
    assert_eq!(update.auto_resize, None);
}

/// `textWysiwyg.test.tsx` "should keep a right-aligned text's right edge
/// where it stops growing": crossing the view's width, a right-aligned text
/// starts wrapping with its right edge where it was.
#[test]
fn a_right_aligned_text_stops_growing_at_its_right_edge() {
    let mut element = text_element(&serde_json::json!({
        "x": 300, "y": 10, "width": 50, "height": 25,
        "text": "short", "originalText": "short", "textAlign": "right",
    }));
    let right = element.base.x + element.base.width;
    let line = "Excalidraw is an opensource virtual collaborative whiteboard for sketching hand-drawn like diagrams!";
    let update = refresh(
        &CharCountTextMetrics,
        &element,
        None,
        Some(line),
        Some(700.0),
    )
    .unwrap();
    assert_eq!(update.auto_resize, Some(false));
    assert!(update.text.contains('\n'));
    update.apply(&mut element);
    assert_eq!(element.base.width, 700.0);
    assert!((element.base.x + element.base.width - right).abs() < 1e-9);
    assert!(!text_fields(&element).auto_resize);
}

/// A text that is not a text, or a deleted one, is left alone.
#[test]
fn only_live_text_is_refreshed() {
    let rect = container("rectangle");
    assert!(refresh(&CharCountTextMetrics, &rect, None, None, None).is_none());
    assert!(text_auto_resize(&rect, &CharCountTextMetrics).is_none());
}
