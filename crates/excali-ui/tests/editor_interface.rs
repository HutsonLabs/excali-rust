//! The form factor rules (ex-701): `common/src/editorInterface.ts`'s
//! breakpoints, `isMobileBreakpoint`, `isTabletBreakpoint`,
//! `getFormFactor` and `deriveStylesPanelMode` against upstream over the
//! grid of editor sizes in `tests/fixtures/styles-panel.json`
//! (`tools/goldens/styles-panel.mjs`), and the stored desktop UI mode.

use excali_ui::editor_interface::{
    derive_styles_panel_mode, get_form_factor, is_mobile_breakpoint, is_tablet_breakpoint,
    load_desktop_ui_mode_preference, DesktopUiMode, FormFactor, StylesPanelMode,
    DESKTOP_UI_MODE_STORAGE_KEY, MQ_MAX_HEIGHT_LANDSCAPE, MQ_MAX_MOBILE, MQ_MAX_TABLET,
    MQ_MAX_WIDTH_LANDSCAPE, MQ_MIN_TABLET, MQ_MIN_WIDTH_DESKTOP, MQ_RIGHT_SIDEBAR_MIN_WIDTH,
};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/styles-panel.json")).expect("fixture parses")
}

#[test]
fn breakpoints_match_upstream() {
    let f = fixture();
    let c = &f["formFactor"]["constants"];
    for (name, value) in [
        ("MQ_MAX_MOBILE", MQ_MAX_MOBILE),
        ("MQ_MAX_WIDTH_LANDSCAPE", MQ_MAX_WIDTH_LANDSCAPE),
        ("MQ_MAX_HEIGHT_LANDSCAPE", MQ_MAX_HEIGHT_LANDSCAPE),
        ("MQ_MIN_TABLET", MQ_MIN_TABLET),
        ("MQ_MAX_TABLET", MQ_MAX_TABLET),
        ("MQ_MIN_WIDTH_DESKTOP", MQ_MIN_WIDTH_DESKTOP),
        ("MQ_RIGHT_SIDEBAR_MIN_WIDTH", MQ_RIGHT_SIDEBAR_MIN_WIDTH),
    ] {
        assert_eq!(c[name].as_f64(), Some(value), "{name}");
    }
}

#[test]
fn form_factor_matches_upstream() {
    let f = fixture();
    let sizes = f["formFactor"]["sizes"].as_array().unwrap();
    assert!(sizes.len() >= 800);
    for s in sizes {
        let (w, h) = (s["width"].as_f64().unwrap(), s["height"].as_f64().unwrap());
        assert_eq!(
            is_mobile_breakpoint(w, h),
            s["mobile"].as_bool().unwrap(),
            "{w}x{h}"
        );
        assert_eq!(
            is_tablet_breakpoint(w, h),
            s["tablet"].as_bool().unwrap(),
            "{w}x{h}"
        );
        assert_eq!(
            get_form_factor(w, h).as_str(),
            s["formFactor"].as_str().unwrap(),
            "{w}x{h}"
        );
    }
}

/// The acceptance rule: a tablet is min(w, h) >= 600 and max(w, h) <=
/// 1180, in either orientation, unless the phone rule holds first.
#[test]
fn tablet_rule() {
    assert_eq!(get_form_factor(768.0, 1024.0), FormFactor::Tablet);
    assert_eq!(get_form_factor(1024.0, 768.0), FormFactor::Tablet);
    assert_eq!(get_form_factor(820.0, 1180.0), FormFactor::Tablet);
    assert_eq!(get_form_factor(1180.0, 600.0), FormFactor::Tablet);
    assert_eq!(get_form_factor(1181.0, 820.0), FormFactor::Desktop);
    assert_eq!(get_form_factor(1024.0, 599.0), FormFactor::Desktop);
    assert_eq!(get_form_factor(599.0, 800.0), FormFactor::Phone);
    assert_eq!(get_form_factor(844.0, 390.0), FormFactor::Phone);
    assert_eq!(get_form_factor(1440.0, 900.0), FormFactor::Desktop);
}

#[test]
fn styles_panel_mode_matches_upstream() {
    let f = fixture();
    let modes = f["formFactor"]["modes"].as_array().unwrap();
    assert_eq!(modes.len(), 6);
    for m in modes {
        let form_factor = match m["formFactor"].as_str().unwrap() {
            "phone" => FormFactor::Phone,
            "tablet" => FormFactor::Tablet,
            _ => FormFactor::Desktop,
        };
        let desktop = DesktopUiMode::parse(m["desktopUIMode"].as_str().unwrap()).unwrap();
        assert_eq!(
            derive_styles_panel_mode(form_factor, desktop).as_str(),
            m["mode"].as_str().unwrap(),
            "{m}"
        );
    }
    assert_eq!(
        derive_styles_panel_mode(FormFactor::Tablet, DesktopUiMode::Full),
        StylesPanelMode::Compact
    );
}

/// `loadDesktopUIModePreference` (`editorInterface.ts:186-202`): only
/// "compact" and "full" are read back from `excalidraw.desktopUIMode`.
#[test]
fn desktop_ui_mode_preference() {
    assert_eq!(DESKTOP_UI_MODE_STORAGE_KEY, "excalidraw.desktopUIMode");
    assert_eq!(
        load_desktop_ui_mode_preference(Some("compact")),
        Some(DesktopUiMode::Compact)
    );
    assert_eq!(
        load_desktop_ui_mode_preference(Some("full")),
        Some(DesktopUiMode::Full)
    );
    assert_eq!(load_desktop_ui_mode_preference(Some("Compact")), None);
    assert_eq!(load_desktop_ui_mode_preference(Some("mobile")), None);
    assert_eq!(load_desktop_ui_mode_preference(None), None);
    assert_eq!(DesktopUiMode::Compact.as_str(), "compact");
}
