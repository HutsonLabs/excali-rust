//! Tauri plugin exposing open, save, export, library fetch and validation
//! commands to a host app (`site/content/architecture/tauri.md`).
//!
//! Upstream counterpart: none (new). The commands run upstream's code
//! paths as `excali-cli` has them: the file dialogs of `loadFromJSON`,
//! `saveAsJSON`, `saveLibraryAsJSON` and `exportCanvas`
//! (`packages/excalidraw/data/json.ts`, `data/index.ts`), the export of
//! `excali render` / `excali export`, `excali validate`'s report, and the
//! library URL allow-list of `validateLibraryUrl`
//! (`data/library.ts:497-528`).
//!
//! | Command (`plugin:excali\|…`) | Arguments | Resolves to |
//! |---|---|---|
//! | `open` | `kind?`: `"scene"` (default) or `"library"` | `{ path, name, text }` or `null` |
//! | `save` | `text`, `kind?`, `path?`, `name?` | the path written, or `null` |
//! | `export` | `scene`, `format` (`"png"` or `"svg"`), `options?`, `save?: { path?, name? }` | PNG bytes or SVG text; with `save`, the path or `null` |
//! | `library_fetch` | `url` | the library's text |
//! | `validate` | `text`, `name?` | `excali validate --json`'s report |
//!
//! `null` is a cancelled dialog. A path passed to `save` or `export` must
//! be one an `open` or save dialog of this plugin returned ([`scope`]).
//! Errors reject with their message.
//!
//! ```
//! fn register<R: tauri::Runtime>(app: tauri::Builder<R>) -> tauri::Builder<R> {
//!     app.plugin(tauri_plugin_dialog::init())
//!         .plugin(tauri_plugin_excali::init())
//! }
//! ```
//!
//! and the capability `capabilities/excali.json` of this crate
//! (`excali:default`) in the app's `capabilities/`.
//!
//! Targets: native. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008):
//! `excali-cli`, `excali-core` (and the crates beneath `excali-cli`).

use std::path::PathBuf;

use tauri::plugin::TauriPlugin;
use tauri::{Manager, Runtime};

mod commands;
pub mod dialogs;
pub mod error;
pub mod fetch;
pub mod files;
pub mod scope;

pub use commands::SaveTarget;
pub use dialogs::{FileDialogs, NativeDialogs};
pub use error::{Error, Result};
pub use fetch::{HttpResponse, LibraryFetcher, Transport};
pub use files::{ExportConfig, ExportOptions, FileKind};

/// The plugin's name: commands are `plugin:excali|<command>` and
/// permissions `excali:<permission>`.
pub const PLUGIN_NAME: &str = "excali";

/// The commands, as `build.rs` generates their permissions.
pub const COMMANDS: &[&str] = &["open", "save", "export", "library_fetch", "validate"];

/// The plugin's state.
pub struct Excali<R: Runtime> {
    dialogs: Box<dyn FileDialogs<R>>,
    fetcher: LibraryFetcher,
    export: ExportConfig,
    granted: scope::Granted,
}

/// The plugin with `tauri-plugin-dialog`'s dialogs, upstream's
/// `ALLOWED_LIBRARY_URLS` and `excali`'s export settings.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new().build()
}

/// The plugin's settings.
pub struct Builder<R: Runtime> {
    dialogs: Box<dyn FileDialogs<R>>,
    allow_list: Option<Vec<String>>,
    transport: Option<Box<dyn Transport>>,
    export: ExportConfig,
}

impl<R: Runtime> Default for Builder<R> {
    fn default() -> Self {
        Builder::new()
    }
}

impl<R: Runtime> Builder<R> {
    pub fn new() -> Self {
        Builder {
            dialogs: Box::new(NativeDialogs),
            allow_list: None,
            transport: None,
            export: ExportConfig::default(),
        }
    }

    /// Other file dialogs than `tauri-plugin-dialog`'s.
    pub fn dialogs(mut self, dialogs: impl FileDialogs<R> + 'static) -> Self {
        self.dialogs = Box::new(dialogs);
        self
    }

    /// The library URL allow-list, entries as upstream's
    /// `validateLibraryUrl` takes them (`host[/path]`); upstream's
    /// `ALLOWED_LIBRARY_URLS` by default.
    pub fn library_urls<I: IntoIterator<Item = S>, S: Into<String>>(mut self, urls: I) -> Self {
        self.allow_list = Some(urls.into_iter().map(Into::into).collect());
        self
    }

    /// How library URLs are fetched; the network (`ureq`) by default.
    pub fn library_transport(mut self, transport: impl Transport + 'static) -> Self {
        self.transport = Some(Box::new(transport));
        self
    }

    /// The font directory exports read (`excali`'s `--fonts`): the
    /// workspace's vendored fonts by default, so an app that ships the
    /// plugin bundles `crates/excali-text/assets/fonts` as a resource and
    /// points here.
    pub fn fonts_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.export.fonts_dir = dir.into();
        self
    }

    /// The `source` an export writes (`getExportSource()`).
    pub fn export_source(mut self, source: impl Into<String>) -> Self {
        self.export.source = source.into();
        self
    }

    pub fn build(self) -> TauriPlugin<R> {
        let default = LibraryFetcher::default();
        let fetcher = LibraryFetcher::new(
            self.allow_list
                .unwrap_or_else(|| default.allow_list().to_vec()),
            self.transport
                .unwrap_or_else(|| Box::new(fetch::UreqTransport::default())),
        );
        let state = Excali {
            dialogs: self.dialogs,
            fetcher,
            export: self.export,
            granted: scope::Granted::default(),
        };
        tauri::plugin::Builder::new(PLUGIN_NAME)
            .invoke_handler(tauri::generate_handler![
                commands::open,
                commands::save,
                commands::export,
                commands::library_fetch,
                commands::validate
            ])
            .setup(move |app, _api| {
                app.manage(state);
                Ok(())
            })
            .build()
    }
}
