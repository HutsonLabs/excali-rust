//! The commands, `plugin:excali|<name>` in the webview. Each runs off the
//! async runtime's workers (dialogs and the network block).

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{Map, Value};
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, Runtime};

use crate::error::{Error, Result};
use crate::files::{self, DialogSpec, ExportOptions, Exported, FileKind, Opened};
use crate::Excali;

async fn blocking<R: Runtime, T: Send + 'static>(
    app: AppHandle<R>,
    f: impl FnOnce(&AppHandle<R>, &Excali<R>) -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app, &app.state::<Excali<R>>()))
        .await
        .map_err(|e| Error::Io(e.to_string()))?
}

/// Where `save` and `export` write: a path an earlier dialog returned, or
/// a save dialog suggesting `name`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveTarget {
    pub path: Option<PathBuf>,
    /// The suggested name, without its extension.
    pub name: Option<String>,
}

/// The path to write, `None` when the dialog was cancelled.
fn target_path<R: Runtime>(
    app: &AppHandle<R>,
    state: &Excali<R>,
    target: &SaveTarget,
    spec: impl FnOnce(Option<&str>) -> DialogSpec,
) -> Result<Option<PathBuf>> {
    if let Some(path) = &target.path {
        return state.granted.check(path).map(Some);
    }
    let picked = state
        .dialogs
        .save_file(app, &spec(target.name.as_deref()))?;
    if let Some(path) = &picked {
        state.granted.grant(path);
    }
    Ok(picked)
}

/// `open({ kind? })`: an open dialog for a scene (the default) or a
/// library; the picked file's path, name and JSON text, or `null`.
#[tauri::command]
pub(crate) async fn open<R: Runtime>(
    app: AppHandle<R>,
    kind: Option<FileKind>,
) -> Result<Option<Opened>> {
    blocking(app, move |app, state| {
        let spec = DialogSpec::open(kind.unwrap_or(FileKind::Scene));
        let Some(path) = state.dialogs.pick_file(app, &spec)? else {
            return Ok(None);
        };
        state.granted.grant(&path);
        files::open_file(&path).map(Some)
    })
    .await
}

/// `save({ text, kind?, path?, name? })`: writes the JSON text to `path`
/// (one a dialog returned) or where a save dialog says; the path, or
/// `null`.
#[tauri::command]
pub(crate) async fn save<R: Runtime>(
    app: AppHandle<R>,
    text: String,
    kind: Option<FileKind>,
    path: Option<PathBuf>,
    name: Option<String>,
) -> Result<Option<PathBuf>> {
    let kind = kind.unwrap_or(FileKind::Scene);
    if !matches!(kind, FileKind::Scene | FileKind::Library) {
        return Err(Error::Usage(format!(
            "save writes a \"scene\" or \"library\"; export writes {kind:?}"
        )));
    }
    blocking(app, move |app, state| {
        let target = SaveTarget { path, name };
        let Some(path) = target_path(app, state, &target, |n| DialogSpec::save(kind, n, false))?
        else {
            return Ok(None);
        };
        files::write(&path, text.as_bytes())?;
        Ok(Some(path))
    })
    .await
}

/// `export({ scene, format, options?, save? })`: the scene exported as
/// `"png"` or `"svg"` without a webview. Without `save`, the PNG bytes
/// (an `ArrayBuffer`) or the SVG text; with it, the file is written and
/// the path (or `null`) returned.
#[tauri::command]
pub(crate) async fn export<R: Runtime>(
    app: AppHandle<R>,
    scene: String,
    format: FileKind,
    options: Option<ExportOptions>,
    save: Option<SaveTarget>,
) -> Result<Response> {
    blocking(app, move |app, state| {
        let options = options.unwrap_or_default();
        let (exported, embeds) = files::export(&scene, format, &options, &state.export)?;
        let Some(target) = save else {
            return Ok(match exported {
                Exported::Png(bytes) => Response::new(bytes),
                Exported::Svg(text) => Response::new(json(&text)?),
            });
        };
        let path = target_path(app, state, &target, |n| DialogSpec::save(format, n, embeds))?;
        if let Some(path) = &path {
            files::write(path, &exported.into_bytes())?;
        }
        Ok(Response::new(json(&path)?))
    })
    .await
}

fn json<T: serde::Serialize>(value: &T) -> Result<tauri::ipc::InvokeResponseBody> {
    serde_json::to_string(value)
        .map(tauri::ipc::InvokeResponseBody::Json)
        .map_err(|e| Error::Io(e.to_string()))
}

/// `library_fetch({ url })`: the library at an allow-listed `https` URL,
/// as text; what a host answers `<excali-editor>`'s `library-fetch` with.
#[tauri::command]
pub(crate) async fn library_fetch<R: Runtime>(app: AppHandle<R>, url: String) -> Result<String> {
    blocking(app, move |_, state| state.fetcher.fetch(&url)).await
}

/// `validate({ text, name? })`: `excali validate --json`'s report of the
/// text, read as a file called `name` (`scene.excalidraw` by default).
#[tauri::command]
pub(crate) async fn validate<R: Runtime>(
    app: AppHandle<R>,
    text: String,
    name: Option<String>,
) -> Result<Map<String, Value>> {
    blocking(app, move |_, _| files::validate(&text, name.as_deref())).await
}
