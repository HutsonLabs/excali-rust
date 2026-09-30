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

use excali_tauri_example::Updates;

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

fn started(smoke: Option<PathBuf>) -> (App<MockRuntime>, WebviewWindow<MockRuntime>) {
    started_with(smoke, Updates::Off)
}

/// The app built and through its first event loop turn, which runs setup
/// (where the excali plugin is registered), and the `main` window.
fn started_with(
    smoke: Option<PathBuf>,
    updates: Updates,
) -> (App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let mut app = excali_tauri_example::builder(mock_builder(), smoke, updates)
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
    let mut capabilities = capabilities;
    capabilities.sort();
    assert_eq!(capabilities, ["app.json", "excali.json"]);
}

/// The app's own commands, each allowed in `main` by `capabilities/app.json`
/// (permissions generated by `build.rs`'s app manifest) and nowhere else.
const APP_COMMANDS: [&str; 4] = ["smoke", "smoke_report", "update_check", "update_install"];

#[test]
fn the_apps_capability_allows_exactly_its_commands_in_main() {
    let app = read_json(&Path::new(APP).join("capabilities/app.json"));
    assert_eq!(app["identifier"], "app");
    assert_eq!(app["windows"], json!(["main"]));
    let want: Vec<String> = APP_COMMANDS
        .iter()
        .map(|c| format!("allow-{}", c.replace('_', "-")))
        .collect();
    assert_eq!(app["permissions"], json!(want));
    // The updater runs natively: no updater:, process: or other plugin
    // permission reaches the page.
    let text = std::fs::read_to_string(Path::new(APP).join("capabilities/app.json")).unwrap();
    for plugin in ["updater:", "process:", "core:", "dialog:", "fs:", "shell:"] {
        assert!(!text.contains(plugin), "{plugin} in capabilities/app.json");
    }
}

#[test]
fn another_window_may_call_none_of_the_apps_commands() {
    let (app, main) = started(None);
    let other = WebviewWindowBuilder::new(&app, "other", Default::default())
        .build()
        .unwrap();
    for cmd in APP_COMMANDS {
        let refused = invoke(&other, cmd, json!({ "report": {} })).unwrap_err();
        assert!(
            refused.to_string().contains("not allowed"),
            "{cmd}: {refused}"
        );
    }
    // In main, each is allowed (and answers for updates off, no smoke).
    assert_eq!(invoke_json(&main, "smoke", json!({})), json!(false));
    assert_eq!(invoke_json(&main, "update_check", json!({})), Value::Null);
    let err = invoke(&main, "update_install", json!({})).unwrap_err();
    assert!(err.to_string().contains("no update"), "{err}");
    let err = invoke(&main, "smoke_report", json!({ "report": {} })).unwrap_err();
    assert!(err.to_string().contains("smoke mode is off"), "{err}");
    // An unknown command is refused too.
    let unknown = invoke(&main, "update_download", json!({})).unwrap_err();
    assert!(!unknown.to_string().is_empty());
}

/// The updater's public key (minisign, key id A58BE9CA1F6D8AE2; the private
/// half signs the `.app.tar.gz` at release, `scripts/release/macos-dmg.sh`).
const UPDATER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEE1OEJFOUNBMUY2RDhBRTIKUldUaWltMGZ5dW1McGNLa2h6YjVJbFN1ZnBVa0N1aXZhbW0yWU9IL3FJUTBVQ3BnLytGc2t3VjYK";

#[test]
fn the_updater_reads_latest_json_from_the_github_releases() {
    let config = config();
    let updater = &config["plugins"]["updater"];
    assert_eq!(updater["pubkey"], UPDATER_PUBKEY);
    assert_eq!(
        updater["endpoints"],
        json!(["https://github.com/HutsonLabs/excali-rust/releases/latest/download/latest.json"])
    );
    // No dangerous transport options.
    assert_eq!(updater.as_object().unwrap().len(), 2, "{updater}");
    // The key decodes to a minisign public key.
    let key = base64_decode(UPDATER_PUBKEY);
    let key = String::from_utf8(key).unwrap();
    assert!(
        key.starts_with("untrusted comment: minisign public key: A58BE9CA1F6D8AE2\n"),
        "{key}"
    );
    // A bundle build writes the .app.tar.gz and its .sig.
    assert_eq!(config["bundle"]["createUpdaterArtifacts"], true);
    // The page never talks to GitHub: the check and the download are native.
    let connect = config["app"]["security"]["csp"]["connect-src"]
        .as_str()
        .unwrap();
    assert_eq!(connect, "'self' ipc: http://ipc.localhost");
}

#[test]
fn the_signing_identity_is_given_at_build_time_and_there_are_no_entitlements() {
    let text = std::fs::read_to_string(Path::new(APP).join("tauri.conf.json")).unwrap();
    for key in ["signingIdentity", "entitlements", "providerShortName"] {
        assert!(!text.contains(key), "{key} in tauri.conf.json");
    }
    assert!(!Path::new(APP).join("entitlements.plist").exists());
}

#[test]
fn updates_are_checked_only_in_a_release_build_outside_smoke_mode() {
    use excali_tauri_example::updates;
    assert_eq!(updates(false, true), Updates::Config);
    assert_eq!(updates(true, true), Updates::Off);
    assert_eq!(updates(false, false), Updates::Off);
    assert_eq!(updates(true, false), Updates::Off);
}

#[test]
fn the_updater_offers_a_newer_release_and_nothing_for_the_same_one() {
    let newer = serve_latest_json("99.1.1");
    let (_app, main) = started_with(None, Updates::From(vec![newer]));
    let found = invoke_json(&main, "update_check", json!({}));
    assert_eq!(
        found,
        json!({ "version": "99.1.1", "current": config()["version"] })
    );

    let same = serve_latest_json(config()["version"].as_str().unwrap());
    let (_app, main) = started_with(None, Updates::From(vec![same]));
    assert_eq!(invoke_json(&main, "update_check", json!({})), Value::Null);
    let err = invoke(&main, "update_install", json!({})).unwrap_err();
    assert!(err.to_string().contains("no update"), "{err}");
}

#[test]
fn a_failed_check_is_an_error_the_page_can_ignore() {
    // Nothing listens there.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let url = format!("http://127.0.0.1:{port}/latest.json")
        .parse()
        .unwrap();
    let (_app, main) = started_with(None, Updates::From(vec![url]));
    assert!(invoke(&main, "update_check", json!({})).is_err());
}

#[test]
fn the_page_offers_the_update_without_blocking_the_editor() {
    let ui = app_dir().join("ui");
    let index = std::fs::read_to_string(ui.join("index.html")).unwrap();
    assert!(index.contains(r#"id="update""#), "no update prompt");
    assert!(index.contains("Install and restart"));
    assert!(index.contains("Later"));
    let app_js = std::fs::read_to_string(ui.join("app.js")).unwrap();
    assert!(app_js.contains(r#"invoke("update_check")"#));
    assert!(app_js.contains(r#"invoke("update_install")"#));
    // Not awaited outside smoke mode: the check runs beside the editor.
    let outside_smoke = app_js.split("if (!smoke) {").nth(1).unwrap();
    let call = outside_smoke.lines().nth(1).unwrap().trim();
    assert_eq!(
        call,
        r#"offerUpdate().catch((e) => console.warn("update check:", e));"#
    );
}

/// A `latest.json` in the shape `scripts/release/macos-dmg.sh` writes, for
/// every platform the tests run on, served over HTTP on localhost (allowed in
/// debug builds) for as long as the test process lives.
fn serve_latest_json(version: &str) -> tauri::Url {
    use std::io::{Read, Write};
    let mut platforms = serde_json::Map::new();
    for os in ["darwin", "linux", "windows"] {
        for arch in ["aarch64", "x86_64"] {
            platforms.insert(
                format!("{os}-{arch}"),
                json!({
                    "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3QK",
                    "url": format!("https://example.invalid/Excali.Example_{version}_{arch}.app.tar.gz"),
                }),
            );
        }
    }
    let body = json!({
        "version": version,
        "pub_date": "2026-09-30T00:00:00Z",
        "platforms": platforms,
    })
    .to_string();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    format!("http://127.0.0.1:{port}/latest.json")
        .parse()
        .unwrap()
}

fn base64_decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.bytes().filter(|&c| c != b'=') {
        let v = ALPHABET.iter().position(|&a| a == c).unwrap() as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
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
