//! Why a command failed. The webview sees the message: a command's promise
//! rejects with it as a string, as `tauri-plugin-dialog` and
//! `tauri-plugin-fs` reject.

use std::fmt;

use excali_cli::error::Failure;
use excali_core::library_url::LibraryUrlError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The file or text is not a scene or library, or does not load
    /// (`excali validate`'s exit code 1).
    Invalid(String),
    /// A file could not be read or written.
    Io(String),
    /// The scene loaded but cannot be exported (`excali`'s exit code 4).
    Export(String),
    /// The arguments are wrong: an unknown format or file kind.
    Usage(String),
    /// A library URL the allow-list refuses: upstream's
    /// `validateLibraryUrl` error (`library.ts:497-528`).
    LibraryUrl(LibraryUrlError),
    /// A library URL that passed the allow-list could not be fetched.
    Fetch(String),
    /// A path the webview names that no dialog of this plugin returned.
    NotGranted(String),
    /// `tauri-plugin-dialog` is not registered on the app.
    NoDialog,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Invalid(m) | Error::Io(m) | Error::Export(m) | Error::Usage(m) => f.write_str(m),
            Error::LibraryUrl(e) => write!(f, "{e}"),
            Error::Fetch(m) => write!(f, "Could not fetch the library: {m}"),
            Error::NotGranted(path) => write!(
                f,
                "{path}: not a path an excali open or save dialog returned"
            ),
            Error::NoDialog => f.write_str(
                "tauri-plugin-dialog is not registered: add .plugin(tauri_plugin_dialog::init()) to the app",
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<Failure> for Error {
    fn from(failure: Failure) -> Error {
        match failure {
            Failure::Invalid(m) => Error::Invalid(m),
            Failure::Usage(m) => Error::Usage(m),
            Failure::Io(m) => Error::Io(m),
            Failure::Export(m) => Error::Export(m),
        }
    }
}

impl From<LibraryUrlError> for Error {
    fn from(e: LibraryUrlError) -> Error {
        Error::LibraryUrl(e)
    }
}

impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
