//! The paths the webview may name: those this plugin's dialogs returned.
//! A command never writes a path the user did not pick, so a compromised
//! page cannot overwrite arbitrary files through `save` or `export`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::{Error, Result};

#[derive(Debug, Default)]
pub struct Granted(Mutex<HashSet<PathBuf>>);

impl Granted {
    /// Allow `path` from now on.
    pub fn grant(&self, path: &Path) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.to_owned());
    }

    /// `path`, when a dialog returned it.
    pub fn check(&self, path: &Path) -> Result<PathBuf> {
        let granted = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if granted.contains(path) {
            Ok(path.to_owned())
        } else {
            Err(Error::NotGranted(path.display().to_string()))
        }
    }
}
