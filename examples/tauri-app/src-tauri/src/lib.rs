//! The example Tauri app (ex-606): `<excali-editor>` in the `main` window
//! (`ui/`), with `tauri-plugin-excali` for open, save, export and library
//! fetch (`site/content/architecture/tauri.md`).
//!
//! Smoke mode: with `EXCALI_EXAMPLE_SMOKE=<dir>` the plugin's file dialogs
//! answer paths in `<dir>` instead of asking (open picks
//! `<dir>/open.excalidraw`, a save dialog its suggested name), `ui/app.js`
//! runs open, save, save as and both exports through its own handlers and
//! reports through `smoke_report`, which writes `<dir>/report.json` and
//! quits. `scripts/smoke.sh` runs the built app that way.
//!
//! Updates (ex-805): a release build outside smoke mode checks the
//! updater endpoint in `tauri.conf.json` (`latest.json` on the GitHub
//! releases) natively when the page asks through `update_check`, and
//! `update_install` downloads, verifies against the configured public key,
//! installs and restarts. The page never talks to the network itself.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Runtime, State, Url};
use tauri_plugin_updater::{Update, UpdaterExt};

use tauri_plugin_excali::files::DialogSpec;
use tauri_plugin_excali::FileDialogs;

/// The environment variable that turns smoke mode on.
pub const SMOKE_ENV: &str = "EXCALI_EXAMPLE_SMOKE";

/// How long smoke mode waits for the report before failing.
const SMOKE_TIMEOUT: Duration = Duration::from_secs(120);

/// The app's context: `tauri.conf.json`, the icons, the capabilities
/// under `capabilities/` and the embedded `ui/`.
pub fn context<R: Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

/// The fonts exports read: the bundle's `fonts/` resource (from
/// `crates/excali-text/assets/fonts`, `tauri.conf.json`), or the
/// workspace's copy when there is no bundle (`cargo tauri dev`, tests).
pub fn fonts_dir(resource_dir: Option<&Path>) -> PathBuf {
    if let Some(fonts) = resource_dir
        .map(|dir| dir.join("fonts"))
        .filter(|fonts| fonts.join("manifest.json").is_file())
    {
        return fonts;
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../crates/excali-text/assets/fonts")
}

/// Where updates come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Updates {
    /// No check: `update_check` answers `null` (dev builds, smoke mode).
    Off,
    /// The endpoints of `plugins.updater` in `tauri.conf.json`.
    Config,
    /// Other endpoints (tests serve a `latest.json` on localhost).
    From(Vec<Url>),
}

/// Updates for a run: only a release build outside smoke mode checks, so
/// `cargo tauri dev`, the tests and the smoke run never reach the network.
pub fn updates(smoke: bool, release: bool) -> Updates {
    if release && !smoke {
        Updates::Config
    } else {
        Updates::Off
    }
}

/// The app on `builder`: the dialog and updater plugins, the app's commands
/// and, at setup, `tauri-plugin-excali` pointed at the fonts.
pub fn builder<R: Runtime>(
    builder: tauri::Builder<R>,
    smoke: Option<PathBuf>,
    updates: Updates,
) -> tauri::Builder<R> {
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Smoke(smoke))
        .manage(Updating {
            updates,
            found: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            self::smoke,
            smoke_report,
            update_check,
            update_install
        ])
        .setup(|app| {
            let resources = app.path().resource_dir().ok();
            let smoke = app.state::<Smoke>().0.clone();
            let mut excali =
                tauri_plugin_excali::Builder::new().fonts_dir(fonts_dir(resources.as_deref()));
            if let Some(dir) = smoke {
                excali = excali.dialogs(SmokeDialogs(dir));
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(SMOKE_TIMEOUT);
                    eprintln!("smoke: no report after {SMOKE_TIMEOUT:?}");
                    handle.exit(1);
                });
            }
            app.handle().plugin(excali.build())?;
            Ok(())
        })
}

/// Runs the app.
pub fn run() {
    let smoke = std::env::var_os(SMOKE_ENV).map(PathBuf::from);
    let updates = updates(smoke.is_some(), !cfg!(debug_assertions));
    builder(tauri::Builder::default(), smoke, updates)
        .run(context())
        .expect("error while running the example app");
}

/// The smoke directory, if smoke mode is on.
struct Smoke(Option<PathBuf>);

/// Whether smoke mode is on (`ui/app.js` asks at startup).
#[tauri::command]
fn smoke(state: State<'_, Smoke>) -> bool {
    state.0.is_some()
}

/// Writes the smoke report to `<dir>/report.json` and quits.
#[tauri::command]
fn smoke_report<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Smoke>,
    report: Value,
) -> Result<(), String> {
    let dir = state.0.as_ref().ok_or("smoke mode is off")?;
    let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("report.json"), text).map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

/// The update source and the update the last check found.
struct Updating {
    updates: Updates,
    found: Mutex<Option<Update>>,
}

/// Checks for an update: `{ version, current }` when the endpoint offers a
/// newer version, `null` when it does not or updates are off. A failed check
/// (offline, no `latest.json` yet) is an error the page ignores.
#[tauri::command]
async fn update_check<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Updating>,
) -> Result<Option<Value>, String> {
    let mut updater = app.updater_builder();
    match &state.updates {
        Updates::Off => return Ok(None),
        Updates::Config => {}
        Updates::From(endpoints) => {
            updater = updater
                .endpoints(endpoints.clone())
                .map_err(|e| e.to_string())?;
        }
    }
    let update = updater
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let answer = update
        .as_ref()
        .map(|u| json!({ "version": u.version, "current": u.current_version }));
    *state.found.lock().map_err(|e| e.to_string())? = update;
    Ok(answer)
}

/// Downloads the update the last check found, verifies its signature with
/// the configured public key, installs it and restarts the app.
#[tauri::command]
async fn update_install<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Updating>,
) -> Result<(), String> {
    let update = state
        .found
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("no update to install")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart()
}

/// File dialogs that answer without asking: open picks `open.excalidraw`
/// (`open.excalidrawlib` for a library) in the directory, save the
/// suggested name there.
struct SmokeDialogs(PathBuf);

impl<R: Runtime> FileDialogs<R> for SmokeDialogs {
    fn pick_file(
        &self,
        _: &AppHandle<R>,
        spec: &DialogSpec,
    ) -> tauri_plugin_excali::Result<Option<PathBuf>> {
        let name = if spec.extensions.contains(&"excalidrawlib") {
            "open.excalidrawlib"
        } else {
            "open.excalidraw"
        };
        Ok(Some(self.0.join(name)))
    }

    fn save_file(
        &self,
        _: &AppHandle<R>,
        spec: &DialogSpec,
    ) -> tauri_plugin_excali::Result<Option<PathBuf>> {
        Ok(spec.file_name.as_ref().map(|name| self.0.join(name)))
    }
}
