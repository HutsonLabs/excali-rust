//! The file operations behind the commands, without Tauri: what a dialog
//! offers, reading a picked file, the validation report and the headless
//! export. Each is `excali-cli`'s code path, so the plugin and `excali`
//! answer the same for the same file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use excali_cli::cli::validation_report;
use excali_cli::env::CliEnv;
use excali_cli::export::{embeds_scene, export_svg, render_png, ExportSettings};
use excali_cli::input::{
    load_scene, load_scene_or_library, parse_file_contents, read_file, InputFile,
};

use crate::error::{Error, Result};

/// What a file dialog is for; each has upstream's description, extensions
/// and default name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    /// A scene: `.excalidraw`, or a `.json`, `.png` or `.svg` with one.
    Scene,
    /// A library: `.excalidrawlib` or `.json`.
    Library,
    /// A PNG export.
    Png,
    /// An SVG export.
    Svg,
}

/// A file dialog's filter and suggested name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogSpec {
    /// The filter's name: upstream's `fileOpen` / `fileSave` description.
    pub description: &'static str,
    /// The filter's extensions, without the dot.
    pub extensions: Vec<&'static str>,
    /// The suggested file name (save dialogs), with its extension.
    pub file_name: Option<String>,
}

impl DialogSpec {
    /// The open dialog: "Excalidraw files" of `loadFromJSON`
    /// (`data/json.ts:106-112`), with the extensions upstream lists there
    /// (commented out in the browser only for iOS Safari's sake), and
    /// "Excalidraw library files" of the library menu's import
    /// (`LibraryMenuHeaderContent.tsx:161-167`).
    pub fn open(kind: FileKind) -> DialogSpec {
        let (description, extensions) = match kind {
            FileKind::Scene => ("Excalidraw files", vec!["excalidraw", "json", "png", "svg"]),
            FileKind::Library => ("Excalidraw library files", vec!["excalidrawlib", "json"]),
            FileKind::Png => ("Export to PNG", vec!["png"]),
            FileKind::Svg => ("Export to SVG", vec!["svg"]),
        };
        DialogSpec {
            description,
            extensions,
            file_name: None,
        }
    }

    /// The save dialog for a file called `name` (without its extension):
    /// `saveAsJSON`'s "Excalidraw file" (`data/json.ts:93-98`),
    /// `saveLibraryAsJSON`'s "Excalidraw library file" named "library"
    /// (`json.ts:147-160`), and `exportCanvas`' "Export to PNG" / "Export
    /// to SVG", whose extension is `excalidraw.png` / `excalidraw.svg` when
    /// the scene is embedded (`data/index.ts:139-150, 187-193`).
    pub fn save(kind: FileKind, name: Option<&str>, embeds_scene: bool) -> DialogSpec {
        let (description, extension, default_name) = match kind {
            FileKind::Scene => ("Excalidraw file", "excalidraw", "Untitled"),
            FileKind::Library => ("Excalidraw library file", "excalidrawlib", "library"),
            FileKind::Png if embeds_scene => ("Export to PNG", "excalidraw.png", "Untitled"),
            FileKind::Png => ("Export to PNG", "png", "Untitled"),
            FileKind::Svg if embeds_scene => ("Export to SVG", "excalidraw.svg", "Untitled"),
            FileKind::Svg => ("Export to SVG", "svg", "Untitled"),
        };
        let name = name.filter(|n| !n.is_empty()).unwrap_or(default_name);
        DialogSpec {
            description,
            // A filter holds the last extension; the name carries the rest.
            extensions: vec![extension.rsplit('.').next().unwrap_or(extension)],
            file_name: Some(format!("{name}.{extension}")),
        }
    }
}

/// A file the open dialog picked.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Opened {
    pub path: PathBuf,
    /// The file name.
    pub name: String,
    /// The scene or library JSON: a `.png` or `.svg` file's embedded scene
    /// (`parseFileContents`, `data/blob.ts:108-136`), any other file's
    /// text. What `<excali-editor>`'s `load` and `importLibrary` take.
    pub text: String,
}

/// Read `path` as upstream reads a picked file.
pub fn open_file(path: &Path) -> Result<Opened> {
    let file = read_file(path)?;
    let text = parse_file_contents(&file)?
        .ok_or_else(|| Error::Invalid(excali_cli::input::IMAGE_NOT_CONTAINS_SCENE.to_owned()))?;
    Ok(Opened {
        path: path.to_owned(),
        name: file.name,
        text,
    })
}

/// The text given as a file called `name` (`scene.excalidraw` when
/// `None`): the name picks the MIME type as `getMimeType` does.
fn input(text: &str, name: Option<&str>) -> InputFile {
    InputFile {
        name: name.unwrap_or("scene.excalidraw").to_owned(),
        bytes: text.as_bytes().to_vec(),
    }
}

/// `excali validate --json`'s report of `text`: a scene's live elements by
/// type, texts and files, or a library's items and elements.
pub fn validate(text: &str, name: Option<&str>) -> Result<Map<String, Value>> {
    let mut env = CliEnv::from_environment();
    let loaded = load_scene_or_library(&input(text, name), &mut env)?;
    Ok(validation_report(&loaded).0)
}

/// The export dialog's choices, as `excali render` and `excali export`
/// take them; absent keys keep the scene's values.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportOptions {
    pub scale: Option<f64>,
    pub padding: Option<f64>,
    pub background: Option<bool>,
    pub dark_mode: Option<bool>,
    pub embed_scene: Option<bool>,
    /// Export this frame (its element id) alone.
    pub frame: Option<String>,
    /// Keep an SVG's fonts as URLs rather than inlined subsets.
    #[serde(default)]
    pub skip_inlining_fonts: bool,
}

/// An exported image.
#[derive(Clone, Debug, PartialEq)]
pub enum Exported {
    Png(Vec<u8>),
    Svg(String),
}

impl Exported {
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Exported::Png(bytes) => bytes,
            Exported::Svg(text) => text.into_bytes(),
        }
    }
}

/// Where the export reads fonts from and what it names as its source.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportConfig {
    pub fonts_dir: PathBuf,
    pub source: String,
}

impl Default for ExportConfig {
    /// `excali`'s: the workspace's vendored fonts, this repository.
    fn default() -> Self {
        let settings = ExportSettings::new();
        ExportConfig {
            fonts_dir: settings.fonts_dir,
            source: settings.source,
        }
    }
}

fn settings(options: &ExportOptions, config: &ExportConfig) -> ExportSettings {
    ExportSettings {
        scale: options.scale,
        padding: options.padding,
        background: options.background,
        dark: options.dark_mode,
        embed_scene: options.embed_scene,
        frame: options.frame.clone(),
        source: config.source.clone(),
        fonts_dir: config.fonts_dir.clone(),
        inline_fonts: !options.skip_inlining_fonts,
    }
}

/// The scene `text` exported as `kind` (PNG or SVG) with `options`: the
/// editor's "Export image" (`exportCanvas`, `data/index.ts:98-192`) as
/// `excali render` / `excali export` run it. Also whether the scene is
/// embedded, which names the file (`.excalidraw.png`).
pub fn export(
    text: &str,
    kind: FileKind,
    options: &ExportOptions,
    config: &ExportConfig,
) -> Result<(Exported, bool)> {
    let mut env = CliEnv::from_environment();
    let scene = load_scene(&input(text, None), &mut env)?;
    let settings = settings(options, config);
    let embeds = embeds_scene(&scene, &settings);
    let exported = match kind {
        FileKind::Png => Exported::Png(render_png(&scene, &settings)?),
        FileKind::Svg => Exported::Svg(export_svg(&scene, &settings)?),
        FileKind::Scene | FileKind::Library => {
            return Err(Error::Usage(format!(
                "cannot export to {kind:?}; use \"png\" or \"svg\""
            )))
        }
    };
    Ok((exported, embeds))
}

/// Write `bytes` to `path`.
pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).map_err(|e| Error::Io(format!("{}: {e}", path.display())))
}
