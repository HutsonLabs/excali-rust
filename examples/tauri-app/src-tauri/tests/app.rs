//! The example app on Tauri's mock runtime: its own context (the
//! configuration, and the capabilities compiled from `capabilities/`), its
//! own setup and plugins, and the calls `ui/app.js` makes, through the IPC
//! and the ACL of the window the editor runs in. The file dialogs are the
//! smoke dialogs (`EXCALI_EXAMPLE_SMOKE`), which answer paths in a
//! directory, so open, save and export write real files.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager, WebviewWindow, WebviewWindowBuilder};

const APP: &str = env!("CARGO_MANIFEST_DIR");

fn app_dir() -> PathBuf {
    Path::new(APP).parent().unwrap().to_path_buf()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn config() -> Value {
    read_json(&Path::new(APP).join("tauri.conf.json"))
}

/// A fresh directory for the smoke dialogs.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "excali-tauri-example-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn mock_app(smoke: &Path) -> (App<MockRuntime>, WebviewWindow<MockRuntime>) {
    started(Some(smoke.to_path_buf()))
}

/// The app built and through its first event loop turn, which runs setup
/// (where the excali plugin is registered), and the `main` window.
fn started(smoke: Option<PathBuf>) -> (App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let mut app = excali_tauri_example::builder(mock_builder(), smoke)
        .build(excali_tauri_example::context())
        .unwrap();
    // One turn, not a loop, so the busy-loop the deprecation warns of
    // cannot happen; run_return would block until the app exits.
    #[allow(deprecated)]
    app.run_iteration(|_, _| {});
    // setup opened the window tauri.conf.json declares, which the
    // capabilities name.
    let window = app.get_webview_window("main").unwrap();
    (app, window)
}

fn invoke(
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: Value,
) -> Result<InvokeResponseBody, Value> {
    tauri::test::get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
}

fn invoke_json(window: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Value {
    match invoke(window, cmd, args) {
        Ok(body) => body.deserialize::<Value>().unwrap(),
        Err(e) => panic!("{cmd} was refused: {e}"),
    }
}

fn scene_fixture() -> String {
    std::fs::read_to_string(app_dir().join("smoke/open.excalidraw")).unwrap()
}

#[test]
fn the_window_loads_the_ui_with_the_web_runtime_under_a_csp_that_allows_wasm() {
    let config = config();
    assert_eq!(config["build"]["frontendDist"], "../ui");
    // Tauri serves the frontend itself; no dev server.
    assert!(config["build"].get("devUrl").is_none());
    assert_eq!(config["app"]["withGlobalTauri"], true);
    let windows = config["app"]["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0]["label"], "main");
    let csp = &config["app"]["security"]["csp"];
    let script = csp["script-src"].as_str().unwrap();
    assert!(script.contains("'wasm-unsafe-eval'"), "{script}");
    assert!(!script.contains("'unsafe-eval'"), "{script}");
    assert!(!script.contains("'unsafe-inline'"), "{script}");
    // ui/index.html and ui/app.js exist; ui/excali/ is the web build.
    let ui = app_dir().join("ui");
    let index = std::fs::read_to_string(ui.join("index.html")).unwrap();
    assert!(index.contains(r#"<script type="module" src="./app.js">"#));
    assert!(index.contains(r#"href="./excali/excali.css""#));
    let app_js = std::fs::read_to_string(ui.join("app.js")).unwrap();
    assert!(app_js.contains(r#"from "./excali/excali_editor.js""#));
    // The runtime script puts the web build where app.js imports it.
    for command in ["beforeDevCommand", "beforeBuildCommand"] {
        assert_eq!(
            config["build"][command], "sh scripts/web-runtime.sh",
            "{command}"
        );
    }
}

#[test]
fn the_bundle_ships_the_fonts_exports_read() {
    let config = config();
    let resources = config["bundle"]["resources"].as_object().unwrap();
    assert_eq!(resources.len(), 1);
    let (from, to) = resources.iter().next().unwrap();
    assert_eq!(to, "fonts/");
    let fonts = Path::new(APP).join(from);
    assert_eq!(
        fonts.canonicalize().unwrap(),
        app_dir()
            .join("../../crates/excali-text/assets/fonts")
            .canonicalize()
            .unwrap()
    );
    assert!(fonts.join("manifest.json").is_file());
    // Without the bundle (cargo tauri dev, tests), the workspace's copy.
    assert_eq!(
        excali_tauri_example::fonts_dir(None)
            .canonicalize()
            .unwrap(),
        fonts.canonicalize().unwrap()
    );
    let bundled = scratch("resources");
    std::fs::create_dir_all(bundled.join("fonts")).unwrap();
    std::fs::write(bundled.join("fonts/manifest.json"), "{}").unwrap();
    assert_eq!(
        excali_tauri_example::fonts_dir(Some(&bundled)),
        bundled.join("fonts")
    );
}

#[test]
fn the_app_ships_the_plugins_capability_unchanged() {
    let shipped =
        read_json(&app_dir().join("../../crates/tauri-plugin-excali/capabilities/excali.json"));
    let copied = read_json(&Path::new(APP).join("capabilities/excali.json"));
    assert_eq!(copied, shipped);
    let capabilities: Vec<_> = std::fs::read_dir(Path::new(APP).join("capabilities"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(capabilities, ["excali.json"]);
}

#[test]
fn open_save_and_export_through_the_windows_acl() {
    let dir = scratch("roundtrip");
    std::fs::write(dir.join("open.excalidraw"), scene_fixture()).unwrap();
    let (_app, window) = mock_app(&dir);

    // Open: the dialog picks open.excalidraw.
    let opened = invoke_json(&window, "plugin:excali|open", json!({}));
    assert_eq!(opened["name"], "open.excalidraw");
    let path = opened["path"].as_str().unwrap().to_owned();
    let scene: Value = serde_json::from_str(opened["text"].as_str().unwrap()).unwrap();
    assert_eq!(scene["type"], "excalidraw");
    assert_eq!(scene["elements"].as_array().unwrap().len(), 3);

    // Save back to the opened file (granted by the open), no dialog.
    let saved = invoke_json(
        &window,
        "plugin:excali|save",
        json!({ "text": opened["text"], "path": path }),
    );
    assert_eq!(saved, json!(path));

    // Save as: the dialog's suggested name, in the smoke directory.
    let saved_as = invoke_json(
        &window,
        "plugin:excali|save",
        json!({ "text": opened["text"], "name": "open" }),
    );
    assert_eq!(
        PathBuf::from(saved_as.as_str().unwrap()),
        dir.join("open.excalidraw")
    );

    // Export PNG and SVG to files, headless, with the fonts the app points
    // the plugin at.
    let png = invoke_json(
        &window,
        "plugin:excali|export",
        json!({ "scene": opened["text"], "format": "png", "save": { "name": "open" } }),
    );
    let png = PathBuf::from(png.as_str().unwrap());
    assert_eq!(png, dir.join("open.png"));
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&png).unwrap()));
    let reader = decoder.read_info().unwrap();
    assert!(reader.info().width > 0 && reader.info().height > 0);

    let svg = invoke_json(
        &window,
        "plugin:excali|export",
        json!({ "scene": opened["text"], "format": "svg", "save": { "name": "open" } }),
    );
    let svg = std::fs::read_to_string(svg.as_str().unwrap()).unwrap();
    assert!(svg.contains("<svg"), "{}", &svg[..svg.len().min(80)]);
    // The text element's font is inlined from the fonts directory.
    assert!(svg.contains("@font-face"), "no inlined font");

    // A path no dialog returned is refused.
    let refused = invoke(
        &window,
        "plugin:excali|save",
        json!({ "text": "{}", "path": dir.join("elsewhere.excalidraw") }),
    );
    assert!(refused.is_err());
}

#[test]
fn only_the_main_window_may_call_the_plugin() {
    let dir = scratch("acl");
    let (app, _main) = mock_app(&dir);
    let other = WebviewWindowBuilder::new(&app, "other", Default::default())
        .build()
        .unwrap();
    let refused = invoke(
        &other,
        "plugin:excali|validate",
        json!({ "text": scene_fixture() }),
    )
    .unwrap_err();
    assert!(refused.to_string().contains("not allowed"), "{refused}");
    let main = app.get_webview_window("main").unwrap();
    let report = invoke_json(
        &main,
        "plugin:excali|validate",
        json!({ "text": scene_fixture() }),
    );
    assert_eq!(report["kind"], "scene", "{report}");
    assert_eq!(report["elements"], 3, "{report}");
}

#[test]
fn smoke_mode_is_off_unless_asked_for() {
    let (_app, window) = started(None);
    assert_eq!(invoke_json(&window, "smoke", json!({})), json!(false));
    let dir = scratch("smoke-on");
    let (_app, window) = mock_app(&dir);
    assert_eq!(invoke_json(&window, "smoke", json!({})), json!(true));
}

#[test]
fn the_app_has_the_workspaces_calendar_version() {
    let root = std::fs::read_to_string(app_dir().join("../../Cargo.toml")).unwrap();
    let workspace = root
        .split("[workspace.package]")
        .nth(1)
        .and_then(|s| s.lines().find(|l| l.starts_with("version = ")))
        .unwrap();
    let manifest = std::fs::read_to_string(Path::new(APP).join("Cargo.toml")).unwrap();
    assert!(
        manifest.lines().any(|l| l == workspace),
        "Cargo.toml: want {workspace}"
    );
    assert_eq!(
        format!("version = {}", config()["version"]),
        workspace,
        "tauri.conf.json"
    );
}
