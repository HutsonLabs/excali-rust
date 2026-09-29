//! The command line: arguments, what each command prints and writes, and
//! the exit code ([`crate::error::code`]).

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use serde_json::{json, Map, Value};

use excali_core::document::LoadedScene;
use excali_core::element::ElementKind;

use crate::env::CliEnv;
use crate::error::{code, Failure};
use crate::export::{embeds_scene, export_svg, render_png, ExportSettings, DEFAULT_SOURCE};
use crate::fonts::{load_fonts, FONTS_DIR_VAR};
use crate::input::{load_library, load_scene, load_scene_or_library, read_file, Loaded};
use crate::library;
use crate::preview;

/// Validate, export and manage Excalidraw files.
#[derive(Debug, Parser)]
#[command(name = "excali", version, arg_required_else_help = true)]
#[command(help_template = "{usage-heading} {usage}\n\n{about}\n\n{all-args}")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check that files load as Excalidraw loads them: a scene
    /// (.excalidraw, .json), a scene embedded in a .png or .svg, or a
    /// library (.excalidrawlib). Gzip files are read through.
    Validate {
        /// Print a JSON array with one report per file.
        #[arg(long)]
        json: bool,
        #[arg(required = true, value_name = "FILE")]
        files: Vec<PathBuf>,
    },
    /// Render a scene to PNG, as the editor's "Export image" does.
    Render(ExportArgs),
    /// Export a scene to SVG, as the editor's "Export image" does.
    Export {
        #[command(flatten)]
        args: ExportArgs,
        /// Do not inline the fonts (exportToSvg's skipInliningFonts).
        #[arg(long)]
        no_fonts: bool,
    },
    /// List, merge and preview libraries (.excalidrawlib).
    #[command(subcommand, arg_required_else_help = true)]
    Lib(LibCommand),
}

#[derive(Debug, Subcommand)]
pub enum LibCommand {
    /// List a library's items: id, status, element count and name.
    List {
        /// Print a JSON array of the items.
        #[arg(long)]
        json: bool,
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
    /// Import each library in turn into the first, as the editor imports
    /// a library (new items first, items already present skipped), and
    /// write the result.
    Merge {
        #[arg(required = true, num_args = 1.., value_name = "FILE")]
        files: Vec<PathBuf>,
        /// The library to write ("-" for standard output).
        #[arg(short, long, value_name = "FILE")]
        output: PathBuf,
        /// The `source` written in the file.
        #[arg(long, env = "EXCALIDRAW_EXPORT_SOURCE", default_value = DEFAULT_SOURCE)]
        source: String,
    },
    /// Draw a library's items onto the preview image the publish dialog
    /// makes: rows of six 128 px boxes, each item scaled to fit its box.
    Preview {
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// The preview image to write, a PNG ("-" for standard output).
        #[arg(short, long, value_name = "FILE")]
        output: PathBuf,
        /// Also write each item's own PNG into this directory
        /// (item-0001.png, ...).
        #[arg(long, value_name = "DIR")]
        items: Option<PathBuf>,
        /// Print a JSON report of the items and the image size.
        #[arg(long)]
        json: bool,
        /// The font directory (default: the fonts this binary was built with).
        #[arg(long, env = FONTS_DIR_VAR, value_name = "DIR")]
        fonts: Option<PathBuf>,
    },
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    /// The scene: .excalidraw, .json, or a .png or .svg with a scene.
    #[arg(value_name = "FILE")]
    pub input: PathBuf,
    /// Where to write ("-" for standard output). Default: the input's name
    /// with .png or .svg (.excalidraw.png or .excalidraw.svg with
    /// --embed-scene).
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
    /// exportScale: the size multiplier.
    #[arg(long, value_parser = positive)]
    pub scale: Option<f64>,
    /// exportPadding in scene pixels (default 10; 0 for a frame).
    #[arg(long, value_parser = finite, allow_negative_numbers = true)]
    pub padding: Option<f64>,
    /// Leave the background transparent.
    #[arg(long)]
    pub no_background: bool,
    /// Export in the dark theme.
    #[arg(long)]
    pub dark: bool,
    /// Embed the scene, so Excalidraw can open the image.
    #[arg(long)]
    pub embed_scene: bool,
    /// Export this frame alone, cropped to it.
    #[arg(long, value_name = "ID")]
    pub frame: Option<String>,
    /// The `source` of an embedded scene.
    #[arg(long, env = "EXCALIDRAW_EXPORT_SOURCE", default_value = DEFAULT_SOURCE)]
    pub source: String,
    /// The font directory (default: the fonts this binary was built with).
    #[arg(long, env = FONTS_DIR_VAR, value_name = "DIR")]
    pub fonts: Option<PathBuf>,
}

fn positive(s: &str) -> Result<f64, String> {
    match s.parse::<f64>() {
        Ok(x) if x.is_finite() && x > 0.0 => Ok(x),
        _ => Err(format!("{s:?} is not a positive number")),
    }
}

fn finite(s: &str) -> Result<f64, String> {
    match s.parse::<f64>() {
        Ok(x) if x.is_finite() => Ok(x),
        _ => Err(format!("{s:?} is not a number")),
    }
}

impl ExportArgs {
    fn settings(&self, inline_fonts: bool) -> ExportSettings {
        let mut settings = ExportSettings::new();
        settings.scale = self.scale;
        settings.padding = self.padding;
        settings.background = self.no_background.then_some(false);
        settings.dark = self.dark.then_some(true);
        settings.embed_scene = self.embed_scene.then_some(true);
        settings.frame = self.frame.clone();
        settings.source = self.source.clone();
        if let Some(dir) = &self.fonts {
            settings.fonts_dir = dir.clone();
        }
        settings.inline_fonts = inline_fonts;
        settings
    }
}

/// The input's name without its extensions (`.gz`, then `.excalidraw`,
/// `.json`, `.png`, `.svg`, and an `.excalidraw` left before an image's),
/// with `extension`, beside the input.
fn default_output(input: &Path, extension: &str) -> PathBuf {
    let name = input
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut stem = name.strip_suffix(".gz").unwrap_or(&name);
    for ext in [".excalidraw", ".json", ".png", ".svg"] {
        if let Some(s) = stem.strip_suffix(ext) {
            stem = s;
            break;
        }
    }
    let stem = stem.strip_suffix(".excalidraw").unwrap_or(stem);
    input.with_file_name(format!("{stem}.{extension}"))
}

fn write_output(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    if path == Path::new("-") {
        let mut out = std::io::stdout().lock();
        return out
            .write_all(bytes)
            .and_then(|()| out.flush())
            .map_err(|e| Failure::Io(format!("standard output: {e}")));
    }
    std::fs::write(path, bytes).map_err(|e| Failure::io(path, &e))
}

fn load_input(path: &Path, env: &mut CliEnv) -> Result<LoadedScene, Failure> {
    let file = read_file(path)?;
    load_scene(&file, env).map_err(|f| prefixed(path, f))
}

/// `f` with the file named.
fn prefixed(path: &Path, f: Failure) -> Failure {
    let message = format!("{}: {}", path.display(), f.message());
    match f {
        Failure::Invalid(_) => Failure::Invalid(message),
        Failure::Usage(_) => Failure::Usage(message),
        Failure::Io(_) => Failure::Io(message),
        Failure::Export(_) => Failure::Export(message),
    }
}

fn export_image(args: &ExportArgs, svg: bool, inline_fonts: bool) -> Result<(), Failure> {
    let mut env = CliEnv::from_environment();
    let scene = load_input(&args.input, &mut env)?;
    let settings = args.settings(inline_fonts);
    let output = match &args.output {
        Some(path) => path.clone(),
        None => {
            let ext = match (svg, embeds_scene(&scene, &settings)) {
                (false, false) => "png",
                (false, true) => "excalidraw.png",
                (true, false) => "svg",
                (true, true) => "excalidraw.svg",
            };
            let path = default_output(&args.input, ext);
            if path == args.input {
                return Err(Failure::Usage(format!(
                    "{}: the output would replace the input; name one with -o",
                    path.display()
                )));
            }
            path
        }
    };
    let bytes = if svg {
        export_svg(&scene, &settings)?.into_bytes()
    } else {
        render_png(&scene, &settings)?
    };
    write_output(&output, &bytes)
}

fn plural(n: usize, one: &str) -> String {
    if n == 1 {
        format!("{n} {one}")
    } else {
        format!("{n} {one}s")
    }
}

/// A file's report: `Ok` with the JSON report and the plain line, or the
/// failure.
fn validate_one(path: &Path) -> Result<(Map<String, Value>, String), Failure> {
    let mut env = CliEnv::from_environment();
    let file = read_file(path)?;
    let mut report = Map::new();
    report.insert("path".into(), json!(path.display().to_string()));
    let (summary, line) = validation_report(&load_scene_or_library(&file, &mut env)?);
    report.extend(summary);
    Ok((report, line))
}

/// What `validate` reports of a loaded file, as JSON and as the plain
/// line: a scene's live elements (by type), texts and files, or a
/// library's items and their elements.
pub fn validation_report(loaded: &Loaded) -> (Map<String, Value>, String) {
    let mut report = Map::new();
    let line = match loaded {
        Loaded::Scene(scene) => {
            let live: Vec<_> = scene
                .elements
                .iter()
                .filter(|e| !e.base.is_deleted)
                .collect();
            let mut types = Map::new();
            let mut texts = Vec::new();
            for element in &live {
                let t = element.kind.element_type().as_str();
                let n = types.get(t).and_then(Value::as_u64).unwrap_or(0);
                types.insert(t.to_owned(), json!(n + 1));
                if let ElementKind::Text(text) = &element.kind {
                    texts.push(Value::String(text.text.clone()));
                }
            }
            let files = scene.files.as_object().map_or(0, Map::len);
            report.insert("kind".into(), json!("scene"));
            report.insert("elements".into(), json!(live.len()));
            report.insert("types".into(), Value::Object(types));
            report.insert("texts".into(), Value::Array(texts));
            report.insert("files".into(), json!(files));
            format!(
                "scene, {}, {}",
                plural(live.len(), "element"),
                plural(files, "file")
            )
        }
        Loaded::Library(items) => {
            let elements: usize = items.iter().map(|i| i.elements.len()).sum();
            report.insert("kind".into(), json!("library"));
            report.insert("items".into(), json!(items.len()));
            report.insert("elements".into(), json!(elements));
            format!(
                "library, {}, {}",
                plural(items.len(), "item"),
                plural(elements, "element")
            )
        }
    };
    (report, line)
}

fn validate(files: &[PathBuf], as_json: bool) -> u8 {
    let mut status = code::OK;
    let mut reports = Vec::new();
    for path in files {
        match validate_one(path) {
            Ok((report, line)) => {
                if !as_json {
                    println!("{}: {line}", path.display());
                }
                reports.push(Value::Object(report));
            }
            Err(failure) => {
                // An I/O failure outranks an invalid file.
                status = status.max(failure.code());
                let message = match &failure {
                    // Io messages already name the path.
                    Failure::Io(m) => m.clone(),
                    f => format!("{}: {}", path.display(), f.message()),
                };
                eprintln!("excali: {message}");
                reports.push(json!({
                    "path": path.display().to_string(),
                    "error": failure.message(),
                }));
            }
        }
    }
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&Value::Array(reports)).unwrap_or_default()
        );
    }
    status
}

fn lib_list(path: &Path, as_json: bool) -> Result<(), Failure> {
    let mut env = CliEnv::from_environment();
    let file = read_file(path)?;
    let items = load_library(&file, &mut env).map_err(|f| prefixed(path, f))?;
    let text = if as_json {
        let list: Vec<Value> = items.iter().map(library::item_summary).collect();
        let mut text = serde_json::to_string_pretty(&Value::Array(list)).unwrap_or_default();
        text.push('\n');
        text
    } else {
        items.iter().map(|i| library::item_line(i) + "\n").collect()
    };
    write_output(Path::new("-"), text.as_bytes())
}

fn lib_merge(files: &[PathBuf], output: &Path, source: &str) -> Result<(), Failure> {
    let mut env = CliEnv::from_environment();
    let mut libraries = Vec::new();
    for path in files {
        let file = read_file(path)?;
        libraries.push(load_library(&file, &mut env).map_err(|f| prefixed(path, f))?);
    }
    let merged = library::merge(&libraries);
    write_output(output, library::write(&merged, source).as_bytes())
}

struct PreviewArgs<'a> {
    file: &'a Path,
    output: &'a Path,
    items: Option<&'a Path>,
    json: bool,
    fonts: Option<&'a Path>,
}

fn lib_preview(args: &PreviewArgs<'_>) -> Result<(), Failure> {
    let mut env = CliEnv::from_environment();
    let file = read_file(args.file)?;
    let items = load_library(&file, &mut env).map_err(|f| prefixed(args.file, f))?;
    if items.is_empty() {
        return Err(prefixed(
            args.file,
            Failure::Export(preview::NO_ITEMS.to_owned()),
        ));
    }
    let fonts_dir = args
        .fonts
        .map_or_else(crate::fonts::built_in_fonts_dir, Path::to_path_buf);
    let all: Vec<_> = items
        .iter()
        .flat_map(|i| i.elements.iter().cloned())
        .collect();
    let fonts = load_fonts(&fonts_dir, &all)?;
    let app_state = preview::default_export_app_state()?;
    let mut canvases = Vec::with_capacity(items.len());
    for item in &items {
        canvases.push(
            preview::item_canvas(item, &app_state, &fonts, &mut env)
                .map_err(|f| prefixed(args.file, f))?,
        );
    }
    let sheet = preview::sheet(&canvases).map_err(|f| prefixed(args.file, f))?;
    let png = |pixmap: &excali_raster::tiny_skia::Pixmap| {
        excali_raster::encode_png(pixmap, None).map_err(|e| Failure::Export(e.to_string()))
    };

    let mut reports = Vec::with_capacity(items.len());
    let mut lines = String::new();
    if let Some(dir) = args.items {
        std::fs::create_dir_all(dir).map_err(|e| Failure::io(dir, &e))?;
    }
    for (index, (item, canvas)) in items.iter().zip(&canvases).enumerate() {
        let mut report = Map::new();
        report.insert("index".into(), json!(index));
        report.insert("id".into(), json!(item.id));
        report.insert("name".into(), json!(item.name));
        report.insert("width".into(), json!(canvas.width()));
        report.insert("height".into(), json!(canvas.height()));
        if let Some(dir) = args.items {
            let path = dir.join(format!("item-{:04}.png", index + 1));
            std::fs::write(&path, png(&canvas.pixmap)?).map_err(|e| Failure::io(&path, &e))?;
            report.insert("file".into(), json!(path.display().to_string()));
        }
        reports.push(Value::Object(report));
        lines.push_str(&format!(
            "{}\t{}x{}\t{}\n",
            item.id,
            canvas.width(),
            canvas.height(),
            item.name.as_deref().unwrap_or_default()
        ));
    }
    write_output(args.output, &png(&sheet)?)?;
    if args.output == Path::new("-") {
        return Ok(());
    }
    let text = if args.json {
        let report = json!({
            "items": reports,
            "width": sheet.width(),
            "height": sheet.height(),
        });
        serde_json::to_string_pretty(&report).unwrap_or_default() + "\n"
    } else {
        lines
    };
    write_output(Path::new("-"), text.as_bytes())
}

/// Run the command line `args` (the program name first); the exit code.
pub fn run<I, T>(args: I) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            return u8::try_from(e.exit_code()).unwrap_or(code::USAGE);
        }
    };
    let result = match &cli.command {
        Command::Validate { json, files } => return validate(files, *json),
        Command::Render(args) => export_image(args, false, true),
        Command::Export { args, no_fonts } => export_image(args, true, !no_fonts),
        Command::Lib(LibCommand::List { json, file }) => lib_list(file, *json),
        Command::Lib(LibCommand::Merge {
            files,
            output,
            source,
        }) => lib_merge(files, output, source),
        Command::Lib(LibCommand::Preview {
            file,
            output,
            items,
            json,
            fonts,
        }) => lib_preview(&PreviewArgs {
            file,
            output,
            items: items.as_deref(),
            json: *json,
            fonts: fonts.as_deref(),
        }),
    };
    match result {
        Ok(()) => code::OK,
        Err(failure) => {
            eprintln!("excali: {failure}");
            failure.code()
        }
    }
}

/// The binary's `main`.
pub fn main() -> ExitCode {
    ExitCode::from(run(std::env::args_os()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_arguments_are_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn default_outputs_replace_the_extensions() {
        let out = |input: &str, ext: &str| default_output(Path::new(input), ext);
        assert_eq!(out("d/a.excalidraw", "png"), Path::new("d/a.png"));
        assert_eq!(out("d/a.json", "svg"), Path::new("d/a.svg"));
        assert_eq!(out("a.excalidraw.png", "svg"), Path::new("a.svg"));
        assert_eq!(
            out("a.excalidraw.svg", "excalidraw.png"),
            Path::new("a.excalidraw.png")
        );
        assert_eq!(out("a.excalidraw.gz", "png"), Path::new("a.png"));
        assert_eq!(out("notes", "png"), Path::new("notes.png"));
    }
}
