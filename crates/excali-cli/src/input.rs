//! Reading a file as upstream's file loading does
//! (`packages/excalidraw/data/blob.ts`).
//!
//! - [`mime_type`] is `getMimeType` of the file name (`blob.ts:82-104`);
//! - [`parse_file_contents`] is `parseFileContents` (`blob.ts:32-80`): the
//!   scene embedded in a PNG (`decodePngMetadata`) or an SVG
//!   (`decodeSvgBase64Payload`), else the text;
//! - [`load_scene_or_library`] is `loadSceneOrLibraryFromBlob`
//!   (`blob.ts:138-196`) with no local state, and for a library the items
//!   restored as an import restores them (`parseLibraryJSON`,
//!   `blob.ts:218-228`), so a library that cannot be imported fails here.
//!
//! A gzip file (`1f 8b`) is read through, its name without `.gz`: the
//! library catalogue in `fixtures/libraries` is stored that way. This is
//! the CLI's convenience; upstream's loader has no such step.

use std::path::Path;

use excali_core::app_state::AppStateEnv;
use excali_core::document::{load_scene_json, LoadSceneError, LoadedScene};
use excali_core::library::{parse_library_json, LibraryError, LibraryItem, LibraryItemStatus};
use excali_core::png::{decode_png_metadata, DecodePngMetadataError};
use excali_core::restore::RestoreEnv;
use excali_core::svg_payload::{decode_svg_base64_payload, SvgPayloadError};

use crate::error::Failure;

/// `ImageSceneDataError("Image doesn't contain scene")`.
pub const IMAGE_NOT_CONTAINS_SCENE: &str = "Image doesn't contain scene";
/// `ImageSceneDataError("Error: cannot restore image")`.
pub const CANNOT_RESTORE_IMAGE: &str = "Error: cannot restore image";
/// What `loadSceneOrLibraryFromBlob` throws for anything else.
pub const INVALID_FILE: &str = "Error: invalid file";

/// The `MIME_TYPES` `getMimeType` gives a file name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MimeType {
    /// `.excalidraw`, `.json`.
    Json,
    Png,
    /// `.jpg`, `.jpeg`.
    Jpg,
    Svg,
    Excalidrawlib,
    /// Anything else (`""`).
    Unknown,
}

impl MimeType {
    /// `isSupportedImageFile`: one of `IMAGE_MIME_TYPES`.
    pub fn is_image(self) -> bool {
        matches!(self, MimeType::Png | MimeType::Jpg | MimeType::Svg)
    }
}

/// `getMimeType(name)` (`blob.ts:82-104`), by extension.
pub fn mime_type(name: &str) -> MimeType {
    let ends = |ext: &str| name.ends_with(ext);
    if ends(".excalidraw") || ends(".json") {
        MimeType::Json
    } else if ends(".png") {
        MimeType::Png
    } else if ends(".jpg") || ends(".jpeg") {
        MimeType::Jpg
    } else if ends(".svg") {
        MimeType::Svg
    } else if ends(".excalidrawlib") {
        MimeType::Excalidrawlib
    } else {
        MimeType::Unknown
    }
}

/// A file read from disk.
#[derive(Clone, Debug)]
pub struct InputFile {
    /// The file name, without a `.gz` the bytes were decompressed from.
    pub name: String,
    pub bytes: Vec<u8>,
}

impl InputFile {
    pub fn mime_type(&self) -> MimeType {
        mime_type(&self.name)
    }
}

/// Read `path`, decompressing a gzip file.
pub fn read_file(path: &Path) -> Result<InputFile, Failure> {
    let bytes = std::fs::read(path).map_err(|e| Failure::io(path, &e))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let bytes = excali_core::encode::inflate(&bytes)
            .map_err(|e| Failure::Invalid(format!("{INVALID_FILE} (gzip: {e})")))?;
        let name = name.strip_suffix(".gz").unwrap_or(&name).to_owned();
        return Ok(InputFile { name, bytes });
    }
    Ok(InputFile { name, bytes })
}

/// `parseFileContents(blob)`: the scene text of an image, or the file's
/// text (`blob.text()`, UTF-8 with replacement). `Ok(None)` is upstream's
/// `undefined` (a compressed payload that ends early), which no JSON
/// parse accepts.
pub fn parse_file_contents(file: &InputFile) -> Result<Option<String>, Failure> {
    match file.mime_type() {
        MimeType::Png => match decode_png_metadata(&file.bytes) {
            Ok(text) => Ok(text),
            Err(DecodePngMetadataError::Invalid) => {
                Err(Failure::Invalid(IMAGE_NOT_CONTAINS_SCENE.to_owned()))
            }
            Err(e) => Err(Failure::Invalid(format!("{CANNOT_RESTORE_IMAGE} ({e})"))),
        },
        MimeType::Svg => {
            let text = String::from_utf8_lossy(&file.bytes);
            match decode_svg_base64_payload(&text) {
                Ok(text) => Ok(Some(text)),
                Err(SvgPayloadError::Incomplete) => Ok(None),
                Err(SvgPayloadError::Invalid) => {
                    Err(Failure::Invalid(IMAGE_NOT_CONTAINS_SCENE.to_owned()))
                }
                Err(SvgPayloadError::Failed(why)) => {
                    Err(Failure::Invalid(format!("{CANNOT_RESTORE_IMAGE} ({why})")))
                }
            }
        }
        _ => Ok(Some(String::from_utf8_lossy(&file.bytes).into_owned())),
    }
}

/// What a file holds.
#[derive(Debug)]
pub enum Loaded {
    Scene(Box<LoadedScene>),
    Library(Vec<LibraryItem>),
}

/// The failure for text `JSON.parse` rejects: an image without a scene,
/// else an invalid file.
fn unparsable(file: &InputFile, why: &dyn std::fmt::Display) -> Failure {
    if file.mime_type().is_image() {
        Failure::Invalid(IMAGE_NOT_CONTAINS_SCENE.to_owned())
    } else {
        Failure::Invalid(format!("{INVALID_FILE} ({why})"))
    }
}

/// `loadSceneOrLibraryFromBlob(file, null, null)`, a library's items
/// restored with the default status (unpublished).
pub fn load_scene_or_library(
    file: &InputFile,
    env: &mut dyn RestoreEnv,
) -> Result<Loaded, Failure> {
    let Some(text) = parse_file_contents(file)? else {
        return Err(unparsable(file, &"no scene data"));
    };
    match load_scene_json(&text, env, &AppStateEnv::default()) {
        Ok(scene) => Ok(Loaded::Scene(Box::new(scene))),
        Err(LoadSceneError::Json(e)) => Err(unparsable(file, &e)),
        Err(LoadSceneError::NotAScene) => {
            match parse_library_json(&text, LibraryItemStatus::Unpublished, env) {
                Ok(items) => Ok(Loaded::Library(items)),
                Err(LibraryError::InvalidLibrary) => Err(Failure::Invalid(INVALID_FILE.to_owned())),
                Err(e) => Err(Failure::Invalid(e.to_string())),
            }
        }
        Err(e) => {
            let cause = std::error::Error::source(&e)
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            Err(Failure::Invalid(format!("{e}{cause}")))
        }
    }
}

/// `loadFromBlob`: a scene, or `Error: invalid file` for a library.
pub fn load_scene(file: &InputFile, env: &mut dyn RestoreEnv) -> Result<LoadedScene, Failure> {
    match load_scene_or_library(file, env)? {
        Loaded::Scene(scene) => Ok(*scene),
        Loaded::Library(_) => Err(Failure::Invalid(format!(
            "{INVALID_FILE} (a library, not a scene)"
        ))),
    }
}

/// `loadLibraryFromBlob(blob, "unpublished")` (`blob.ts:230-235`):
/// `parseLibraryJSON` of the file's contents.
pub fn load_library(
    file: &InputFile,
    env: &mut dyn RestoreEnv,
) -> Result<Vec<LibraryItem>, Failure> {
    let Some(text) = parse_file_contents(file)? else {
        return Err(unparsable(file, &"no scene data"));
    };
    parse_library_json(&text, LibraryItemStatus::Unpublished, env)
        .map_err(|e| Failure::Invalid(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_types_follow_the_extension() {
        assert_eq!(mime_type("a.excalidraw"), MimeType::Json);
        assert_eq!(mime_type("a.json"), MimeType::Json);
        assert_eq!(mime_type("a.excalidraw.png"), MimeType::Png);
        assert_eq!(mime_type("a.jpeg"), MimeType::Jpg);
        assert_eq!(mime_type("a.excalidraw.svg"), MimeType::Svg);
        assert_eq!(mime_type("a.excalidrawlib"), MimeType::Excalidrawlib);
        assert_eq!(mime_type("a.txt"), MimeType::Unknown);
        assert_eq!(mime_type("excalidraw"), MimeType::Unknown);
    }

    #[test]
    fn a_png_whose_chunks_are_broken_cannot_be_restored() {
        let file = InputFile {
            name: "x.png".into(),
            bytes: b"not a png".to_vec(),
        };
        let e = parse_file_contents(&file).unwrap_err();
        assert!(e.message().starts_with(CANNOT_RESTORE_IMAGE), "{e}");
    }

    #[test]
    fn a_text_file_is_read_as_utf8() {
        let file = InputFile {
            name: "x.excalidraw".into(),
            bytes: "{\"a\": \"\u{e9}\"}".as_bytes().to_vec(),
        };
        assert_eq!(
            parse_file_contents(&file).unwrap().unwrap(),
            "{\"a\": \"\u{e9}\"}"
        );
    }
}
