//! The values the styles panel's controls show: upstream's `getFormValue`
//! and `reduceToCommonValue` (`actions/actionProperties.tsx:229-270`,
//! `common/src/utils.ts:1160-1182`) and what each action's
//! `PanelComponent` reads through them, over the scenes of
//! excali-ui's `tests/fixtures/action-panels.json` (upstream-built
//! elements, `tools/goldens/action-panels.mjs`). The rendered panels are
//! compared with upstream's in excali-ui's `tests/action_panels.rs`; these
//! pin the rules one by one.

use excali_core::app_state::AppState;
use excali_core::element::{
    Arrowhead, Element, FillStyle, FontFamily, StrokeStyle, StrokeVariability, StrokeWidthKey,
    TextAlign, VerticalAlign,
};
use excali_editor::actions::{
    align_enabled, bucket_fill_color, distribute_enabled, form_arrow_type, form_arrowhead,
    form_fill_style, form_font_family, form_font_size, form_opacity, form_roughness,
    form_roundness, form_stroke_style, form_stroke_variability, form_stroke_width_key,
    form_text_align, form_vertical_align, get_form_value, group_enabled, link_panel_state,
    linear_editor_target, polygon_toggle, reduce_to_common_value, selected_fill_styles,
    ActionContext, ActionEnv, AppProps, ArrowType, ArrowheadPosition, EdgeRoundness,
};
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("../../excali-ui/tests/fixtures/action-panels.json");

fn scene(name: &str) -> Vec<Element> {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("action-panels.json parses");
    fixture["scenes"][name]
        .as_array()
        .expect("a scene")
        .iter()
        .map(|e| Element::from_map(e.as_object().unwrap().clone()).expect("an element"))
        .collect()
}

struct World {
    elements: Vec<Element>,
    state: AppState,
    props: AppProps,
    env: ActionEnv,
}

impl World {
    /// The extra scene with `ids` selected and `patch` over the default
    /// app state; `editing` is the text being edited.
    fn new(ids: &[&str], patch: Value, editing: Option<&str>) -> World {
        let elements = scene("extra");
        let mut state = AppState::default();
        let selected: serde_json::Map<String, Value> =
            ids.iter().map(|id| (id.to_string(), json!(true))).collect();
        state.insert("selectedElementIds", Value::Object(selected));
        for (k, v) in patch.as_object().unwrap() {
            state.insert(k.clone(), v.clone());
        }
        if let Some(id) = editing {
            let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
            let raw = fixture["scenes"]["extra"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["id"] == json!(id))
                .unwrap()
                .clone();
            state.insert("editingTextElement", raw);
        }
        World {
            elements,
            state,
            props: AppProps::default(),
            env: ActionEnv::default(),
        }
    }

    fn select(ids: &[&str]) -> World {
        World::new(ids, json!({}), None)
    }

    fn ctx(&self) -> ActionContext<'_> {
        ActionContext {
            elements: &self.elements,
            app_state: &self.state,
            props: &self.props,
            env: &self.env,
        }
    }
}

#[test]
fn reduce_to_common_value_needs_one_non_null_value() {
    let w = World::select(&[]);
    let by_id = |ids: &[&str]| -> Vec<&Element> {
        ids.iter()
            .map(|id| w.elements.iter().find(|e| e.base.id == *id).unwrap())
            .collect()
    };
    let fill = |e: &Element| Some(e.base.fill_style);
    assert_eq!(reduce_to_common_value(by_id(&["z1", "z2"]), fill), Some(FillStyle::Zigzag));
    assert_eq!(reduce_to_common_value(by_id(&["z1", "h1"]), fill), None);
    assert_eq!(reduce_to_common_value(Vec::<&Element>::new(), fill), None);
    // a null value anywhere is no common value
    let link = |e: &Element| e.base.link.clone();
    assert_eq!(reduce_to_common_value(by_id(&["lk1", "z1"]), link), None);
    assert_eq!(
        reduce_to_common_value(by_id(&["lk1"]), link),
        Some("https://example.com".to_string())
    );
}

#[test]
fn get_form_value_reads_the_edited_text_then_the_selection_then_the_default() {
    // editing: the edited text's value, unless falsy (roughness 0 is)
    let w = World::new(&[], json!({ "currentItemRoughness": 2 }), Some("tx1"));
    assert_eq!(form_roughness(&w.ctx()), Some(2.0));
    let w = World::new(&[], json!({}), Some("tx2"));
    assert_eq!(form_text_align(&w.ctx()), Some(TextAlign::Right));
    // the selection's common value, its default with a selection
    let got = get_form_value(
        &World::select(&["o1", "o2"]).ctx(),
        |e| Some(e.base.opacity),
        |_| true,
        |has_selection| (!has_selection).then_some(100.0),
    );
    assert_eq!(got, None);
    // opacity 0 is a value: `??` falls back on null only
    assert_eq!(form_opacity(&World::select(&["o1"]).ctx()), Some(0.0));
    let w = World::new(&[], json!({ "currentItemOpacity": 40 }), None);
    assert_eq!(form_opacity(&w.ctx()), Some(40.0));
}

#[test]
fn fill_style() {
    assert_eq!(form_fill_style(&World::select(&["z1", "z2"]).ctx()), Some(FillStyle::Zigzag));
    assert_eq!(form_fill_style(&World::select(&["z1", "h1"]).ctx()), None);
    // text has no fill style: the predicate leaves it out
    assert_eq!(form_fill_style(&World::select(&["z1", "tx1"]).ctx()), Some(FillStyle::Zigzag));
    assert_eq!(
        selected_fill_styles(&World::select(&["z1", "tx1", "h1"]).ctx()),
        vec![FillStyle::Zigzag, FillStyle::Hachure]
    );
    let w = World::new(&[], json!({ "currentItemFillStyle": "cross-hatch" }), None);
    assert_eq!(form_fill_style(&w.ctx()), Some(FillStyle::CrossHatch));
    assert!(selected_fill_styles(&w.ctx()).is_empty());
}

#[test]
fn stroke_width_style_sloppiness_and_pressure() {
    assert_eq!(form_stroke_width_key(&World::select(&["o2"]).ctx()), Some(StrokeWidthKey::Bold));
    assert_eq!(form_stroke_width_key(&World::select(&["o3"]).ctx()), Some(StrokeWidthKey::Thin));
    assert_eq!(form_stroke_width_key(&World::select(&["o2", "o3"]).ctx()), None);
    let w = World::new(&[], json!({ "currentItemStrokeWidthKey": "bold" }), None);
    assert_eq!(form_stroke_width_key(&w.ctx()), Some(StrokeWidthKey::Bold));
    assert_eq!(form_stroke_style(&World::select(&["o2"]).ctx()), Some(StrokeStyle::Dashed));
    assert_eq!(form_roughness(&World::select(&["o2"]).ctx()), Some(0.0));
    assert_eq!(form_roughness(&World::select(&["o2", "o3"]).ctx()), None);
    assert_eq!(
        form_stroke_variability(&World::select(&["fd1"]).ctx()),
        Some(StrokeVariability::Constant)
    );
    // mixed: `?? appState.currentItemStrokeVariability`
    let w = World::new(&["fd1", "fd2"], json!({ "currentItemStrokeVariability": "variable" }), None);
    assert_eq!(form_stroke_variability(&w.ctx()), Some(StrokeVariability::Variable));
}

#[test]
fn text_properties() {
    // a note's label shows its base font size
    assert_eq!(form_font_size(&World::select(&["sn1"]).ctx()), Some(28.0));
    assert_eq!(form_font_size(&World::select(&["tx1"]).ctx()), Some(16.0));
    assert_eq!(form_font_size(&World::select(&["tx1", "tx2"]).ctx()), None);
    let w = World::new(&[], json!({ "currentItemFontSize": 0 }), None);
    assert_eq!(form_font_size(&w.ctx()), Some(20.0));
    assert_eq!(form_font_family(&World::select(&["tx2"]).ctx()), Some(FontFamily(6)));
    assert_eq!(form_font_family(&World::select(&["c2"]).ctx()), form_font_family(&World::select(&["tx1"]).ctx()));
    assert_eq!(form_text_align(&World::select(&["c2"]).ctx()), Some(TextAlign::Right));
    // vertical alignment: a container's label, or a bound text
    assert_eq!(form_vertical_align(&World::select(&["c2"]).ctx()), Some(VerticalAlign::Top));
    assert_eq!(form_vertical_align(&World::select(&["tx1"]).ctx()), None);
    assert_eq!(form_vertical_align(&World::select(&[]).ctx()), Some(VerticalAlign::Middle));
}

#[test]
fn edges_arrows_and_polygons() {
    // a legacy roundness anywhere in the targets shows neither
    assert_eq!(form_roundness(&World::select(&["lr1"]).ctx()), None);
    assert_eq!(form_roundness(&World::select(&["lr2"]).ctx()), Some(EdgeRoundness::Round));
    assert_eq!(form_roundness(&World::select(&["p3"]).ctx()), Some(EdgeRoundness::Sharp));
    let w = World::new(&[], json!({ "currentItemRoundness": "round" }), None);
    assert_eq!(form_roundness(&w.ctx()), Some(EdgeRoundness::Round));
    let ar1 = World::select(&["ar1"]);
    assert_eq!(form_arrowhead(&ar1.ctx(), ArrowheadPosition::Start), Some(Arrowhead::Circle));
    assert_eq!(form_arrowhead(&ar1.ctx(), ArrowheadPosition::End), Some(Arrowhead::CardinalityOne));
    assert_eq!(form_arrowhead(&World::select(&["ar1", "ar2"]).ctx(), ArrowheadPosition::Start), None);
    assert_eq!(form_arrow_type(&World::select(&["ar4"]).ctx()), Some(ArrowType::Elbow));
    assert_eq!(form_arrow_type(&World::select(&["ar3"]).ctx()), Some(ArrowType::Round));
    assert_eq!(form_arrow_type(&World::select(&["ar1"]).ctx()), Some(ArrowType::Sharp));
    assert_eq!(form_arrow_type(&World::select(&["ar1", "ar4"]).ctx()), None);
    // the polygon toggle shows over polygons only, checked when all are
    assert_eq!(polygon_toggle(&World::select(&["p1", "p2"]).ctx()), Some(true));
    assert_eq!(polygon_toggle(&World::select(&["p1", "p3"]).ctx()), None);
    assert_eq!(polygon_toggle(&World::select(&["p4"]).ctx()), None);
    assert_eq!(polygon_toggle(&World::select(&[]).ctx()), None);
    assert_eq!(
        linear_editor_target(&World::select(&["ar1"]).ctx()).map(|e| e.base.id.clone()),
        Some("ar1".to_string())
    );
    assert!(linear_editor_target(&World::select(&[]).ctx()).is_none());
}

#[test]
fn gates_links_and_bucket_fill() {
    assert!(align_enabled(&World::select(&["z1", "z2"]).ctx()));
    assert!(!align_enabled(&World::select(&["z1"]).ctx()));
    assert!(!distribute_enabled(&World::select(&["z1", "z2"]).ctx()));
    assert!(distribute_enabled(&World::select(&["z1", "z2", "h1"]).ctx()));
    assert!(group_enabled(&World::select(&["z1", "z2"]).ctx()));
    assert!(!group_enabled(&World::select(&["z1"]).ctx()));
    // the title reads the scene's first element (an embeddable here)
    let link = link_panel_state(&World::select(&["lk1"]).ctx());
    assert_eq!(link.label, "labels.link.edit");
    assert_eq!(link.title, "labels.link.labelEmbed");
    assert!(link.checked);
    let link = link_panel_state(&World::select(&["z1"]).ctx());
    assert_eq!(link.label, "labels.link.create");
    assert!(!link.checked);
    let w = World::new(&[], json!({ "currentItemBackgroundColor": "transparent" }), None);
    assert_eq!(bucket_fill_color(&w.ctx()), "#b2f2bb");
    let w = World::new(&[], json!({ "currentItemBackgroundColor": "#ffc9c9" }), None);
    assert_eq!(bucket_fill_color(&w.ctx()), "#ffc9c9");
}
