//! `exportToSvg`'s `appState` argument (`scene/export.ts:293-321`) and the
//! scene it embeds (`serializeAsJSON(elements, appState, files, "local")`,
//! `data/json.ts:52-75`).

use excali_scene::export::{serialize_as_json, FrameRendering, SvgExportAppState};
use serde_json::{json, Map, Value};

fn object(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn new_is_the_editors_defaults() {
    let s = SvgExportAppState::new("#ffffff");
    assert!(!s.export_background);
    assert_eq!(s.export_padding, None);
    assert_eq!(s.export_scale, None);
    assert!(!s.export_with_dark_mode);
    assert!(!s.export_embed_scene);
    assert_eq!(s.frame_rendering, None);
    assert_eq!(
        Value::Object(s.to_app_state()),
        json!({
            "exportBackground": false,
            "exportWithDarkMode": false,
            "viewBackgroundColor": "#ffffff",
            "exportEmbedScene": false,
        })
    );
}

#[test]
fn from_app_state_reads_as_javascript_does() {
    let s = SvgExportAppState::from_app_state(&object(json!({
        "exportBackground": 1,
        "exportPadding": null,
        "exportScale": "2",
        "viewBackgroundColor": "#abcdef",
        "exportWithDarkMode": "",
        "exportEmbedScene": [],
        "frameRendering": { "enabled": 1, "clip": 0, "name": "yes" },
    })));
    assert!(s.export_background);
    assert_eq!(s.export_padding, Some(0.0));
    assert_eq!(s.export_scale, Some(2.0));
    assert_eq!(s.view_background_color, "#abcdef");
    assert!(!s.export_with_dark_mode);
    assert!(s.export_embed_scene);
    assert_eq!(
        s.frame_rendering,
        Some(FrameRendering {
            enabled: true,
            clip: false,
            name: true,
            outline: false,
        })
    );
    // `appState.frameRendering ?? null` then `|| default`
    let off = SvgExportAppState::from_app_state(&object(json!({ "frameRendering": null })));
    assert_eq!(off.frame_rendering, None);
    assert_eq!(off.view_background_color, "");
}

#[test]
fn the_embedded_app_state_is_the_exported_keys_in_the_objects_order() {
    // utils' exportToSvg passes a whole AppState: cleanAppStateForExport
    // keeps the keys with `export: true`, in the object's order
    let mut s = SvgExportAppState::from_app_state(&object(json!({
        "gridSize": 20,
        "viewBackgroundColor": "#000000",
        "exportScale": 3,
        "gridModeEnabled": true,
        "zoom": { "value": 2 },
    })));
    s.view_background_color = "#fafafa".into();
    let app_state = s.to_app_state();
    assert_eq!(
        app_state.keys().collect::<Vec<_>>(),
        [
            "gridSize",
            "viewBackgroundColor",
            "exportScale",
            "gridModeEnabled",
            "zoom",
            "exportBackground",
            "exportWithDarkMode",
            "exportEmbedScene"
        ]
    );
    let scene: Value =
        serde_json::from_str(&serialize_as_json(&[], &app_state, None, "https://x")).unwrap();
    assert_eq!(
        scene,
        json!({
            "type": "excalidraw",
            "version": 2,
            "source": "https://x",
            "elements": [],
            "appState": { "gridSize": 20, "viewBackgroundColor": "#fafafa", "gridModeEnabled": true },
            "files": {},
        })
    );
}
