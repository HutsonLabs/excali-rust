//! The native file dialogs, behind a trait so a host can supply its own
//! (or a test a scripted one).

use std::path::PathBuf;

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::{Dialog, DialogExt};

use crate::error::{Error, Result};
use crate::files::DialogSpec;

/// Open and save dialogs. `Ok(None)` is a cancelled dialog.
pub trait FileDialogs<R: Runtime>: Send + Sync {
    fn pick_file(&self, app: &AppHandle<R>, spec: &DialogSpec) -> Result<Option<PathBuf>>;
    fn save_file(&self, app: &AppHandle<R>, spec: &DialogSpec) -> Result<Option<PathBuf>>;
}

/// `tauri-plugin-dialog`'s file dialogs; the app registers that plugin.
pub struct NativeDialogs;

fn builder<R: Runtime>(
    app: &AppHandle<R>,
    spec: &DialogSpec,
) -> Result<tauri_plugin_dialog::FileDialogBuilder<R>> {
    if app.try_state::<Dialog<R>>().is_none() {
        return Err(Error::NoDialog);
    }
    let mut dialog = app
        .dialog()
        .file()
        .add_filter(spec.description, &spec.extensions);
    if let Some(name) = &spec.file_name {
        dialog = dialog.set_file_name(name);
    }
    Ok(dialog)
}

fn path(picked: Option<tauri_plugin_dialog::FilePath>) -> Result<Option<PathBuf>> {
    picked
        .map(|p| p.into_path().map_err(|e| Error::Io(e.to_string())))
        .transpose()
}

impl<R: Runtime> FileDialogs<R> for NativeDialogs {
    fn pick_file(&self, app: &AppHandle<R>, spec: &DialogSpec) -> Result<Option<PathBuf>> {
        path(builder(app, spec)?.blocking_pick_file())
    }

    fn save_file(&self, app: &AppHandle<R>, spec: &DialogSpec) -> Result<Option<PathBuf>> {
        path(builder(app, spec)?.blocking_save_file())
    }
}
