//! The editor's image export of a loaded scene: `exportCanvas(type,
//! elements, appState, files, {exportBackground, viewBackgroundColor,
//! exportingFrame})` (`packages/excalidraw/data/index.ts:98-192`), as
//! `App.onExportImage` calls it with the scene's state.
//!
//! The export settings are the app state's after loading: a file's
//! `exportBackground`, `exportScale`, `exportWithDarkMode` and
//! `exportEmbedScene` are not restored (`APP_STATE_STORAGE_CONF` marks
//! them browser-only, `appState.ts:153-291`), so a loaded scene has the
//! defaults (background on, scale 1, light, no embedded scene) and
//! [`ExportSettings`] set them as the export dialog does.
//!
//! - The elements are the non-deleted ones; with a frame, the frame and
//!   the elements overlapping it (`prepareElementsForExport` with the
//!   frame alone selected, `data/index.ts:48-95`).
//! - PNG: [`export_canvas_png`] and the raster backend, text drawn from
//!   the font files ([`GlyphText`]), images decoded from the files' data
//!   URLs (upstream's `updateImageCache`).
//! - SVG: `exportToSvg` with exactly the keys `exportCanvas` passes
//!   (`exportBackground`, `exportWithDarkMode`, `viewBackgroundColor`,
//!   `exportPadding`, `exportScale`, `exportEmbedScene`), fonts inlined
//!   from the font files subset to the scene's characters (ADR-010).

use std::path::PathBuf;

use serde_json::{Map, Value};

use excali_core::document::LoadedScene;
use excali_core::element::Element;
use excali_raster::decode::ImageFiles;
use excali_raster::export_png;
use excali_scene::bounds::ElementsMap;
use excali_scene::canvas_export::{
    export_canvas_png, get_elements_overlapping_frame, CanvasExportOptions, CanvasSizing,
    ExportCanvasError,
};
use excali_scene::export::{svg_document, SvgExportAppState, SvgExportOptions};
use excali_svg::{export_to_svg, to_svg_file, FontFiles};

use crate::error::Failure;
use crate::fonts::{load_fonts, GlyphText, Metrics};

/// `getExportSource()` for what the CLI writes: this repository, as a
/// page's origin is upstream's (`EXCALIDRAW_EXPORT_SOURCE` overrides it).
pub const DEFAULT_SOURCE: &str = "https://github.com/HutsonLabs/excali-rust";

/// The export dialog's choices; `None` keeps the loaded scene's value.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportSettings {
    /// `exportScale`.
    pub scale: Option<f64>,
    /// `exportPadding`; `None` is `DEFAULT_EXPORT_PADDING` (10).
    pub padding: Option<f64>,
    /// `exportBackground`.
    pub background: Option<bool>,
    /// `exportWithDarkMode`.
    pub dark: Option<bool>,
    /// `exportEmbedScene`.
    pub embed_scene: Option<bool>,
    /// Export this frame alone.
    pub frame: Option<String>,
    /// `getExportSource()`.
    pub source: String,
    /// The font directory.
    pub fonts_dir: PathBuf,
    /// Inline the fonts in an SVG (`opts.skipInliningFonts` when off).
    pub inline_fonts: bool,
}

impl ExportSettings {
    /// The loaded scene's settings, the built-in fonts, the default source.
    pub fn new() -> ExportSettings {
        ExportSettings {
            scale: None,
            padding: None,
            background: None,
            dark: None,
            embed_scene: None,
            frame: None,
            source: DEFAULT_SOURCE.to_owned(),
            fonts_dir: crate::fonts::built_in_fonts_dir(),
            inline_fonts: true,
        }
    }

    /// The scene's app state with the settings applied.
    fn app_state(&self, scene: &LoadedScene) -> Map<String, Value> {
        let mut state = scene.app_state.as_map().clone();
        let mut set = |key: &str, value: Option<Value>| {
            if let Some(value) = value {
                state.insert(key.to_owned(), value);
            }
        };
        set(
            "exportScale",
            self.scale
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number),
        );
        set("exportBackground", self.background.map(Value::Bool));
        set("exportWithDarkMode", self.dark.map(Value::Bool));
        set("exportEmbedScene", self.embed_scene.map(Value::Bool));
        state
    }
}

impl Default for ExportSettings {
    fn default() -> Self {
        ExportSettings::new()
    }
}

/// Whether `appState.embedScene` is on after the settings: the file name's
/// `excalidraw.png` / `excalidraw.svg` extension.
pub fn embeds_scene(scene: &LoadedScene, settings: &ExportSettings) -> bool {
    truthy(settings.app_state(scene).get("exportEmbedScene"))
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

const EMPTY_CANVAS: &str = "Cannot export empty canvas.";

/// The elements to export and the frame, if one is exported.
fn prepare(
    scene: &LoadedScene,
    frame: Option<&str>,
) -> Result<(Vec<Element>, Option<Element>), Failure> {
    let elements: Vec<Element> = scene
        .elements
        .iter()
        .filter(|e| !e.base.is_deleted)
        .cloned()
        .collect();
    let Some(id) = frame else {
        return Ok((elements, None));
    };
    let frame = elements
        .iter()
        .find(|e| e.base.id == id && e.kind.element_type().is_frame_like())
        .cloned()
        .ok_or_else(|| Failure::Export(format!("no frame with id {id:?}")))?;
    let map = ElementsMap::new(&elements);
    let exported = get_elements_overlapping_frame(&elements, &frame, &map)
        .into_iter()
        .cloned()
        .collect();
    Ok((exported, Some(frame)))
}

fn files_of(scene: &LoadedScene) -> Map<String, Value> {
    scene.files.as_object().cloned().unwrap_or_default()
}

/// The PNG `exportCanvas("png")` saves.
pub fn render_png(scene: &LoadedScene, settings: &ExportSettings) -> Result<Vec<u8>, Failure> {
    let (elements, frame) = prepare(scene, settings.frame.as_deref())?;
    if elements.is_empty() {
        return Err(Failure::Export(EMPTY_CANVAS.to_owned()));
    }
    let app_state = settings.app_state(scene);
    let files = files_of(scene);
    let fonts = load_fonts(&settings.fonts_dir, &elements)?;
    let images = ImageFiles::decode(files.iter().filter_map(|(id, file)| {
        Some((
            id.as_str(),
            file.get("mimeType")?.as_str()?,
            file.get("dataURL")?.as_str()?,
        ))
    }));
    let loads = |id: &str| images.mime_type(id).is_some();
    let options = CanvasExportOptions {
        export_background: truthy(app_state.get("exportBackground")),
        export_padding: settings.padding,
        view_background_color: app_state
            .get("viewBackgroundColor")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        exporting_frame: frame.as_ref(),
        sizing: CanvasSizing::ExportScale,
        text_metrics: &fonts,
        image_loads: &loads,
    };
    let doc = export_canvas_png(&elements, &app_state, &files, &options, &settings.source)
        .map_err(|e| match e {
            ExportCanvasError::EmptyCanvas => Failure::Export(e.to_string()),
        })?;
    export_png(&doc, &images, &mut GlyphText::new(&fonts))
        .map_err(|e| Failure::Export(e.to_string()))
}

/// The `.svg` file `exportCanvas("svg")` saves.
pub fn export_svg(scene: &LoadedScene, settings: &ExportSettings) -> Result<String, Failure> {
    let (elements, frame) = prepare(scene, settings.frame.as_deref())?;
    if elements.is_empty() {
        return Err(Failure::Export(EMPTY_CANVAS.to_owned()));
    }
    let app = settings.app_state(scene);
    let files = files_of(scene);
    let fonts = load_fonts(&settings.fonts_dir, &elements)?;
    let mut state = SvgExportAppState::new(
        app.get("viewBackgroundColor")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    state.export_background = truthy(app.get("exportBackground"));
    state.export_with_dark_mode = truthy(app.get("exportWithDarkMode"));
    state.export_embed_scene = truthy(app.get("exportEmbedScene"));
    state.export_scale = app.get("exportScale").and_then(Value::as_f64);
    state.export_padding = settings.padding;
    let metrics = Metrics(&fonts);
    let mut options = SvgExportOptions::new(&settings.source, &metrics);
    options.exporting_frame = frame.as_ref();
    options.skip_inlining_fonts = !settings.inline_fonts;
    let document = svg_document(&elements, &state, Some(&files), &options);
    let root = export_to_svg(&document, &FontFiles::new(&settings.fonts_dir));
    Ok(to_svg_file(&root))
}
