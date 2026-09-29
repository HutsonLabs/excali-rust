//! The commands through Tauri's IPC and ACL (the mock runtime), with the
//! permissions and capability this crate ships.

mod support;

use serde_json::{json, Value};
use tauri::ipc::InvokeResponseBody;

use support::{temp_dir, Harness, LIBRARY, SCENE};
use tauri_plugin_excali::files::DialogSpec;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const LIBRARY_URL: &str =
    "https://libraries.excalidraw.com/libraries/excalidraw/basic-shapes.excalidrawlib";

#[test]
fn validate_reports_a_scene_as_excali_validate_does() {
    let h = Harness::new();
    let report = h.call("validate", json!({ "text": SCENE })).unwrap();
    assert_eq!(
        report,
        json!({
            "kind": "scene",
            "elements": 2,
            "types": { "rectangle": 1, "text": 1 },
            "texts": ["Hello"],
            "files": 0
        })
    );
}

#[test]
fn validate_reports_a_library() {
    let h = Harness::new();
    let report = h
        .call(
            "validate",
            json!({ "text": LIBRARY, "name": "shapes.excalidrawlib" }),
        )
        .unwrap();
    assert_eq!(
        report,
        json!({ "kind": "library", "items": 1, "elements": 1 })
    );
}

#[test]
fn validate_rejects_what_is_not_a_scene_with_upstreams_message() {
    let h = Harness::new();
    let message = h
        .call("validate", json!({ "text": "{\"type\":\"other\"}" }))
        .unwrap_err();
    assert_eq!(message, "Error: invalid file");
}

#[test]
fn a_window_the_capability_does_not_name_is_refused() {
    let h = Harness::new();
    let refused = h
        .invoke_in("other", "validate", json!({ "text": SCENE }))
        .unwrap_err();
    let message = refused.as_str().unwrap_or_default();
    assert!(message.contains("validate"), "{refused}");
    assert!(message.contains("not allowed"), "{refused}");
}

#[test]
fn open_reads_the_picked_file_and_a_cancelled_dialog_is_null() {
    let h = Harness::new();
    let dir = temp_dir("open");
    let path = dir.join("drawing.excalidraw");
    std::fs::write(&path, SCENE).unwrap();

    h.dialogs.answer(Some(path.clone()));
    let opened = h.call("open", json!({})).unwrap();
    assert_eq!(
        opened,
        json!({ "path": path, "name": "drawing.excalidraw", "text": SCENE })
    );

    h.dialogs.answer(None);
    assert_eq!(
        h.call("open", json!({ "kind": "library" })).unwrap(),
        Value::Null
    );

    let asked = h.dialogs.asked.lock().unwrap().clone();
    assert_eq!(
        asked,
        vec![
            (
                "open".to_owned(),
                DialogSpec::open(tauri_plugin_excali::FileKind::Scene)
            ),
            (
                "open".to_owned(),
                DialogSpec::open(tauri_plugin_excali::FileKind::Library)
            ),
        ]
    );
    assert_eq!(asked[0].1.description, "Excalidraw files");
    assert_eq!(asked[0].1.extensions, ["excalidraw", "json", "png", "svg"]);
    assert_eq!(asked[1].1.description, "Excalidraw library files");
    assert_eq!(asked[1].1.extensions, ["excalidrawlib", "json"]);
}

#[test]
fn open_decodes_the_scene_embedded_in_an_exported_png() {
    let h = Harness::new();
    let dir = temp_dir("open-png");
    let path = dir.join("drawing.excalidraw.png");
    h.dialogs.answer(Some(path.clone()));
    let written = h
        .call(
            "export",
            json!({
                "scene": SCENE,
                "format": "png",
                "options": { "embedScene": true },
                "save": { "name": "drawing" }
            }),
        )
        .unwrap();
    assert_eq!(written, json!(path));

    h.dialogs.answer(Some(path.clone()));
    let opened = h.call("open", json!({})).unwrap();
    let text = opened["text"].as_str().unwrap();
    let scene: Value = serde_json::from_str(text).unwrap();
    assert_eq!(scene["type"], "excalidraw");
    let ids: Vec<&str> = scene["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["rect", "label"]);
}

#[test]
fn save_writes_where_the_dialog_says() {
    let h = Harness::new();
    let dir = temp_dir("save");
    let path = dir.join("mine.excalidraw");
    h.dialogs.answer(Some(path.clone()));
    let saved = h
        .call("save", json!({ "text": SCENE, "name": "mine" }))
        .unwrap();
    assert_eq!(saved, json!(path));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), SCENE);
    let asked = h.dialogs.asked.lock().unwrap().clone();
    assert_eq!(asked[0].0, "save");
    assert_eq!(asked[0].1.description, "Excalidraw file");
    assert_eq!(asked[0].1.file_name.as_deref(), Some("mine.excalidraw"));
}

#[test]
fn a_library_saves_as_library_excalidrawlib() {
    let h = Harness::new();
    h.dialogs.answer(None);
    let saved = h
        .call("save", json!({ "text": LIBRARY, "kind": "library" }))
        .unwrap();
    assert_eq!(saved, Value::Null);
    let asked = h.dialogs.asked.lock().unwrap().clone();
    assert_eq!(asked[0].1.description, "Excalidraw library file");
    assert_eq!(asked[0].1.extensions, ["excalidrawlib"]);
    assert_eq!(
        asked[0].1.file_name.as_deref(),
        Some("library.excalidrawlib")
    );
}

#[test]
fn save_writes_back_to_an_opened_file_without_a_dialog() {
    let h = Harness::new();
    let dir = temp_dir("save-back");
    let path = dir.join("drawing.excalidraw");
    std::fs::write(&path, SCENE).unwrap();
    h.dialogs.answer(Some(path.clone()));
    h.call("open", json!({})).unwrap();

    let saved = h
        .call("save", json!({ "text": "{}", "path": path }))
        .unwrap();
    assert_eq!(saved, json!(path));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
    assert_eq!(h.dialogs.asked.lock().unwrap().len(), 1);
}

#[test]
fn save_refuses_a_path_no_dialog_returned() {
    let h = Harness::new();
    let dir = temp_dir("save-refused");
    let path = dir.join("elsewhere.excalidraw");
    let message = h
        .call("save", json!({ "text": SCENE, "path": path }))
        .unwrap_err();
    assert_eq!(
        message,
        format!(
            "{}: not a path an excali open or save dialog returned",
            path.display()
        )
    );
    assert!(!path.exists());
}

#[test]
fn save_refuses_an_export_kind() {
    let h = Harness::new();
    let message = h
        .call("save", json!({ "text": SCENE, "kind": "png" }))
        .unwrap_err();
    assert!(message.contains("export"), "{message}");
}

#[test]
fn export_returns_png_bytes_that_decode() {
    let h = Harness::new();
    let body = h
        .invoke(
            "export",
            json!({ "scene": SCENE, "format": "png", "options": { "scale": 2 } }),
        )
        .unwrap();
    let InvokeResponseBody::Raw(bytes) = body else {
        panic!("PNG export is not raw bytes");
    };
    assert!(bytes.starts_with(PNG_SIGNATURE));
    let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
    let reader = decoder.read_info().unwrap();
    let info = reader.info();
    // (120 + 2 × 10 padding) × 2 wide: the rectangle is the widest element.
    assert_eq!(info.width, 280);
}

#[test]
fn export_returns_the_svg_file_text() {
    let h = Harness::new();
    let svg = h
        .call("export", json!({ "scene": SCENE, "format": "svg" }))
        .unwrap();
    let svg = svg.as_str().unwrap();
    assert!(svg.starts_with(excali_svg_preamble()), "{}", &svg[..80]);
    assert!(svg.contains("<svg"));
}

fn excali_svg_preamble() -> &'static str {
    "<?xml version=\"1.0\" standalone=\"no\"?>\n<!DOCTYPE svg PUBLIC"
}

#[test]
fn export_to_a_file_names_it_as_upstream_does() {
    let h = Harness::new();
    let dir = temp_dir("export-file");
    let path = dir.join("drawing.svg");
    h.dialogs.answer(Some(path.clone()));
    h.dialogs.answer(None);
    let written = h
        .call(
            "export",
            json!({ "scene": SCENE, "format": "svg", "save": { "name": "drawing" } }),
        )
        .unwrap();
    assert_eq!(written, json!(path));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with(excali_svg_preamble()));

    let cancelled = h
        .call(
            "export",
            json!({
                "scene": SCENE,
                "format": "png",
                "options": { "embedScene": true },
                "save": {}
            }),
        )
        .unwrap();
    assert_eq!(cancelled, Value::Null);

    let asked = h.dialogs.asked.lock().unwrap().clone();
    assert_eq!(asked[0].1.description, "Export to SVG");
    assert_eq!(asked[0].1.file_name.as_deref(), Some("drawing.svg"));
    assert_eq!(asked[1].1.description, "Export to PNG");
    assert_eq!(asked[1].1.extensions, ["png"]);
    assert_eq!(
        asked[1].1.file_name.as_deref(),
        Some("Untitled.excalidraw.png")
    );
}

#[test]
fn export_of_an_empty_scene_fails_as_excali_render_does() {
    let h = Harness::new();
    let empty = r#"{"type":"excalidraw","version":2,"elements":[],"appState":{}}"#;
    let message = h
        .call("export", json!({ "scene": empty, "format": "png" }))
        .unwrap_err();
    assert_eq!(message, "Cannot export empty canvas.");
}

#[test]
fn export_refuses_unknown_options() {
    let h = Harness::new();
    let message = h
        .call(
            "export",
            json!({ "scene": SCENE, "format": "png", "options": { "zoom": 3 } }),
        )
        .unwrap_err();
    assert!(message.contains("zoom"), "{message}");
}

#[test]
fn library_fetch_answers_an_allow_listed_url() {
    let h = Harness::new();
    h.network.serve(LIBRARY_URL, 200, None, LIBRARY);
    let text = h
        .call("library_fetch", json!({ "url": LIBRARY_URL }))
        .unwrap();
    assert_eq!(text, json!(LIBRARY));
}

#[test]
fn library_fetch_refuses_a_url_off_the_list_without_a_request() {
    let h = Harness::new();
    let url = "https://example.com/x.excalidrawlib";
    let message = h.call("library_fetch", json!({ "url": url })).unwrap_err();
    // The editor's message for the same URL (tests/web/specs/editor.spec.mjs).
    assert_eq!(
        message,
        "Invalid or disallowed library URL: \"https://example.com/x.excalidrawlib\""
    );
    assert!(h.network.requested.lock().unwrap().is_empty());
}

#[test]
fn library_fetch_takes_the_hosts_allow_list() {
    let url = "https://libs.example.org/team/shapes.excalidrawlib";
    let h = Harness::with(|b| b.library_urls(["libs.example.org/team"]));
    h.network.serve(url, 200, None, LIBRARY);
    assert_eq!(
        h.call("library_fetch", json!({ "url": url })).unwrap(),
        json!(LIBRARY)
    );
    assert!(h
        .call("library_fetch", json!({ "url": LIBRARY_URL }))
        .is_err());
}

#[test]
fn library_fetch_follows_redirects_on_the_list_only() {
    let h = Harness::new();
    let raw = "https://raw.githubusercontent.com/excalidraw/excalidraw-libraries/main/libraries/x.excalidrawlib";
    h.network.serve(LIBRARY_URL, 302, Some(raw), "");
    h.network.serve(raw, 200, None, LIBRARY);
    assert_eq!(
        h.call("library_fetch", json!({ "url": LIBRARY_URL }))
            .unwrap(),
        json!(LIBRARY)
    );

    let away = "https://excalidraw.com/away.excalidrawlib";
    h.network.serve(away, 301, Some("//evil.example/lib"), "");
    let message = h.call("library_fetch", json!({ "url": away })).unwrap_err();
    assert_eq!(
        message,
        "Invalid or disallowed library URL: \"https://evil.example/lib\""
    );
    assert!(!h
        .network
        .requested
        .lock()
        .unwrap()
        .iter()
        .any(|u| u.contains("evil")));
}

#[test]
fn library_fetch_refuses_http_and_reports_failures() {
    let h = Harness::new();
    let http = "http://libraries.excalidraw.com/x.excalidrawlib";
    assert_eq!(
        h.call("library_fetch", json!({ "url": http })).unwrap_err(),
        format!("Invalid or disallowed library URL: \"{http}\"")
    );
    let missing = "https://excalidraw.com/missing.excalidrawlib";
    h.network.serve(missing, 404, None, "not found");
    assert_eq!(
        h.call("library_fetch", json!({ "url": missing }))
            .unwrap_err(),
        format!("Could not fetch the library: {missing}: HTTP 404")
    );
}

#[test]
fn the_plugin_says_when_tauri_plugin_dialog_is_missing() {
    // The native dialogs, on an app without tauri-plugin-dialog.
    let h = Harness::with(|b| b.dialogs(tauri_plugin_excali::NativeDialogs));
    let message = h.call("open", json!({})).unwrap_err();
    assert_eq!(
        message,
        "tauri-plugin-dialog is not registered: add .plugin(tauri_plugin_dialog::init()) to the app"
    );
}
