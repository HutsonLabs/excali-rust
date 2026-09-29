//! The icon set (ex-517): every icon upstream's `components/icons.tsx`
//! exports, as the static SVG strings React renders from it, held to
//! `tests/fixtures/icons.json` (`tools/goldens/icons.mjs` renders both the
//! fixture and `src/icons/generated.rs` at the pin): the 207 icons in
//! export order, their kind, size preset and right-to-left mirroring, the
//! markup parsed into the DOM builder's tree and serialized back, and the
//! primitives' icons drawn from the set.

use std::collections::BTreeSet;

use excali_scene::shape::Theme;
use excali_ui::icons::{self, icon, Icon, Markup, Preset, ICONS};
use excali_ui::primitives::icons::{close_icon, eye_closed_icon, eye_icon};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/icons.json")).unwrap()
}

fn preset_name(preset: Option<Preset>) -> Value {
    match preset {
        None => Value::Null,
        Some(Preset::Tabler) => "tabler".into(),
        Some(Preset::ModifiedTabler) => "modifiedTabler".into(),
        Some(Preset::ArrowheadPreview) => "arrowheadPreview".into(),
    }
}

fn as_json(icon: &Icon) -> Value {
    let mut v = serde_json::json!({
        "name": icon.name,
        "mirror": icon.mirror,
        "preset": preset_name(icon.preset),
    });
    let o = v.as_object_mut().unwrap();
    match icon.markup {
        Markup::Static(markup) => {
            o.insert("kind".into(), "static".into());
            o.insert("markup".into(), markup.into());
        }
        Markup::Themed { light, dark } => {
            o.insert("kind".into(), "themed".into());
            o.insert("light".into(), light.into());
            o.insert("dark".into(), dark.into());
        }
        Markup::Paths(paths) => {
            o.insert("kind".into(), "paths".into());
            o.insert("paths".into(), paths.to_vec().into());
        }
    }
    v
}

#[test]
fn the_icons_are_upstreams_in_export_order() {
    let f = fixture();
    let expected = f["icons"].as_array().unwrap();
    assert_eq!(ICONS.len(), 207);
    assert_eq!(expected.len(), ICONS.len());
    for (icon, want) in ICONS.iter().zip(expected) {
        assert_eq!(&as_json(icon), want, "{}", icon.name);
    }
}

#[test]
fn names_are_unique_and_looked_up() {
    let names: BTreeSet<_> = ICONS.iter().map(|i| i.name).collect();
    assert_eq!(names.len(), ICONS.len());
    for i in ICONS {
        assert!(std::ptr::eq(icon(i.name).unwrap(), i), "{}", i.name);
    }
    assert!(icon("createIcon").is_none());
    assert!(icon("iconFillColor").is_none());
    assert!(icon("NoSuchIcon").is_none());
    // Upstream names are kept, case pairs included.
    assert_eq!(icon("HelpIcon").unwrap().name, icons::HelpIcon.name);
    assert_eq!(icon("helpIcon").unwrap().name, icons::helpIcon.name);
}

#[test]
fn markup_by_theme() {
    // handlerColor (icons.tsx:18-19).
    let group = &icons::GroupIcon;
    assert!(group.svg(Theme::Light).unwrap().contains("fill=\"#fff\""));
    assert!(group.svg(Theme::Dark).unwrap().contains("fill=\"#1e1e1e\""));
    let plus = &icons::PlusIcon;
    assert_eq!(plus.svg(Theme::Light), plus.svg(Theme::Dark));
    assert!(icons::bucketFillIconSvgPaths.svg(Theme::Light).is_none());
    assert!(icons::bucketFillIconSvgPaths
        .element(Theme::Light)
        .is_none());
    match icons::eyeDropperIconSvgPaths.markup {
        Markup::Paths(paths) => assert_eq!(paths.len(), 2),
        _ => panic!("eyeDropperIconSvgPaths is a path list"),
    }
}

#[test]
fn markup_parses_into_the_dom_tree_and_back() {
    for i in ICONS {
        for theme in [Theme::Light, Theme::Dark] {
            let (Some(svg), Some(el)) = (i.svg(theme), i.element(theme)) else {
                assert!(matches!(i.markup, Markup::Paths(_)), "{}", i.name);
                continue;
            };
            assert_eq!(el.to_html(), svg, "{} {theme:?}", i.name);
            assert_eq!(el.is_svg(), el.tag() != "div", "{}", i.name);
        }
    }
    let empty = icons::emptyIcon.element(Theme::Light).unwrap();
    assert_eq!(empty.tag(), "div");
    assert_eq!(empty.attribute("style"), Some("width:1rem;height:1rem"));
}

#[test]
fn mirror_for_rtl_is_preserved() {
    let mirrored: Vec<_> = ICONS.iter().filter(|i| i.mirror).map(|i| i.name).collect();
    assert_eq!(
        mirrored,
        ["questionCircle", "clone", "GroupIcon", "UngroupIcon"]
    );
    for i in ICONS {
        if let Some(el) = i.element(Theme::Light) {
            let class = el.attribute("class").unwrap_or("");
            assert_eq!(class == "rtl-mirror", i.mirror, "{}", i.name);
        }
    }
}

#[test]
fn presets_set_the_view_box() {
    let count = |p| ICONS.iter().filter(|i| i.preset == Some(p)).count();
    assert_eq!(count(Preset::Tabler), 108);
    assert_eq!(count(Preset::ModifiedTabler), 47);
    assert_eq!(count(Preset::ArrowheadPreview), 15);
    assert_eq!(Preset::Tabler.size(), (24, 24));
    assert_eq!(Preset::ModifiedTabler.size(), (20, 20));
    assert_eq!(Preset::ArrowheadPreview.size(), (40, 20));
    for i in ICONS {
        let Some(preset) = i.preset else { continue };
        let el = i.element(Theme::Light).unwrap();
        let (w, h) = preset.size();
        assert_eq!(
            el.attribute("viewBox"),
            Some(format!("0 0 {w} {h}").as_str()),
            "{}",
            i.name
        );
    }
    // tablerIconProps (icons.tsx:53-61).
    let plus = icons::PlusIcon.element(Theme::Light).unwrap();
    for (k, v) in [
        ("fill", "none"),
        ("stroke-width", "2"),
        ("stroke", "currentColor"),
        ("stroke-linecap", "round"),
        ("stroke-linejoin", "round"),
    ] {
        assert_eq!(plus.attribute(k), Some(v), "{k}");
    }
}

#[test]
fn the_primitives_icons_come_from_the_set() {
    assert_eq!(
        close_icon().to_html(),
        icons::CloseIcon.svg(Theme::Light).unwrap()
    );
    assert_eq!(
        eye_icon().to_html(),
        icons::eyeIcon.svg(Theme::Light).unwrap()
    );
    assert_eq!(
        eye_closed_icon().to_html(),
        icons::eyeClosedIcon.svg(Theme::Light).unwrap()
    );
}
