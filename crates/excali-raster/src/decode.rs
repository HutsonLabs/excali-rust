//! Image files from data URLs: what the browser makes of
//! `img.src = fileData.dataURL` (`loadHTMLImageElement`,
//! `packages/element/src/image.ts:21-32`) for the files `updateImageCache`
//! loads into an export's image cache (`:36-87`).
//!
//! - [`parse_data_url`] reads the URL as the Fetch standard's `data:` URL
//!   processor does (Chrome's `net::DataURL::Parse` follows it): the MIME
//!   type before the comma, a `;base64` body through forgiving base64, any
//!   other body percent-decoded.
//! - [`decode_data_url`] decodes the body. Raster formats are recognised
//!   from their bytes, as Chrome's image decoders sniff them whatever the
//!   declared type: PNG, JPEG, GIF (the first frame), WebP, BMP and ICO,
//!   with the Exif orientation applied (`image-orientation: from-image`,
//!   which also holds for `drawImage`), stored premultiplied as the canvas
//!   keeps bitmaps. SVG is read only when the type is `image/svg+xml`, and
//!   stays a vector ([`DecodedImage::Svg`]): Chrome draws an SVG image at
//!   the resolution it is drawn at.
//! - [`ImageFiles`] is the [`ImageStore`] of an export: every file decoded,
//!   those that failed kept with their error. As upstream's cache, a file
//!   whose `mimeType` is `application/octet-stream` never loads.
//!
//! Formats the port does not decode: AVIF (the `image` crate decodes it
//! only through the C library dav1d) and colour profiles (an embedded ICC
//! profile or PNG `gAMA`/`cHRM` is ignored; the pixels are taken as sRGB).
//! An SVG image's own `<image>` elements load from data URLs only, as an
//! SVG in an `<img>` cannot fetch anything; its text needs the fonts in
//! the caller's [`svg_options`].

use std::collections::HashMap;
use std::fmt;
use std::io::Cursor;
use std::sync::Arc;

use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use base64::engine::DecodePaddingMode;
use base64::Engine;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use resvg::usvg;
use tiny_skia::{IntSize, Pixmap};

use crate::{Image, ImageStore};

/// `MIME_TYPES.svg`.
const SVG_MIME_TYPE: &str = "image/svg+xml";
/// `MIME_TYPES.binary`: a file upstream never loads as an image.
const BINARY_MIME_TYPE: &str = "application/octet-stream";

/// Why a file has no image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Not a `data:` URL, or its base64 body is broken (a network error in
    /// the browser).
    DataUrl(String),
    /// The file's `mimeType` is `application/octet-stream`: "Only images
    /// can be added to ImageCache" (`image.ts:57-59`).
    NotAnImage,
    /// The bytes are no image the browser decodes (the `<img>` fires
    /// `error`).
    Image(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::DataUrl(why) => write!(f, "not a usable data URL: {why}"),
            DecodeError::NotAnImage => write!(f, "Only images can be added to ImageCache"),
            DecodeError::Image(why) => write!(f, "not a decodable image: {why}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// A parsed `data:` URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataUrl {
    /// The MIME type's essence, lower case (`type/subtype`, no
    /// parameters); `text/plain` when there is none or it does not parse.
    pub mime_type: String,
    pub body: Vec<u8>,
}

/// The `data:` URL processor (Fetch standard, section 4.3): the URL
/// parser's removal of tabs and newlines and trimming of C0 controls and
/// spaces, then the MIME type up to the first comma, the body
/// percent-decoded and, with `;base64`, forgiving-base64 decoded.
pub fn parse_data_url(url: &str) -> Result<DataUrl, DecodeError> {
    let url: String = url
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let url = url.trim_matches(|c: char| c <= ' ');
    let rest = match url.get(..5) {
        Some(scheme) if scheme.eq_ignore_ascii_case("data:") => &url[5..],
        _ => return Err(DecodeError::DataUrl("no data: scheme".into())),
    };
    let Some(comma) = rest.find(',') else {
        return Err(DecodeError::DataUrl("no comma".into()));
    };
    let mut mime = rest[..comma].trim_matches(is_ascii_whitespace).to_owned();
    let mut body = percent_decode(&rest.as_bytes()[comma + 1..]);
    if let Some(stripped) = strip_base64_suffix(&mime) {
        body = forgiving_base64_decode(&body)
            .ok_or_else(|| DecodeError::DataUrl("broken base64 body".into()))?;
        mime = stripped;
    }
    if mime.starts_with(';') {
        mime.insert_str(0, "text/plain");
    }
    Ok(DataUrl {
        mime_type: mime_essence(&mime).unwrap_or_else(|| "text/plain".into()),
        body,
    })
}

/// ASCII whitespace (Infra): tab, LF, FF, CR, space.
fn is_ascii_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0c' | '\r' | ' ')
}

/// A MIME type ending in `;`, spaces and `base64` (ASCII
/// case-insensitive), without that ending: the last six characters, then
/// trailing spaces, then the `;`.
fn strip_base64_suffix(mime: &str) -> Option<String> {
    let lower = mime.to_ascii_lowercase();
    let before = lower.strip_suffix("base64")?;
    let before = before.trim_end_matches(' ');
    before.strip_suffix(';')?;
    let kept = mime.len() - "base64".len();
    let mut out = mime[..kept].trim_end_matches(' ').to_owned();
    out.pop();
    Some(out)
}

/// Percent-decoding (URL standard): `%` and two hex digits become the
/// byte; anything else is kept.
fn percent_decode(input: &[u8]) -> Vec<u8> {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' && i + 2 < input.len() {
            if let (Some(h), Some(l)) = (hex(input[i + 1]), hex(input[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(input[i]);
        i += 1;
    }
    out
}

/// Forgiving-base64 decode (Infra): ASCII whitespace removed, one or two
/// `=` of padding when the length is a multiple of four, no other `=`,
/// trailing bits ignored.
fn forgiving_base64_decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut data: Vec<u8> = input
        .iter()
        .copied()
        .filter(|b| !matches!(b, b'\t' | b'\n' | b'\x0c' | b'\r' | b' '))
        .collect();
    if data.len().is_multiple_of(4) {
        for _ in 0..2 {
            if data.last() == Some(&b'=') {
                data.pop();
            }
        }
    }
    if data.len() % 4 == 1 {
        return None;
    }
    const FORGIVING: GeneralPurpose = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_allow_trailing_bits(true)
            .with_decode_padding_mode(DecodePaddingMode::RequireNone),
    );
    FORGIVING.decode(&data).ok()
}

/// A MIME type's essence (MIME Sniffing standard, "parse a MIME type"):
/// the type and subtype, HTTP tokens, lower-cased; `None` when it does not
/// parse.
fn mime_essence(mime: &str) -> Option<String> {
    let is_http_whitespace = |c: char| matches!(c, '\t' | '\n' | '\r' | ' ');
    let mime = mime.trim_matches(is_http_whitespace);
    let (kind, rest) = mime.split_once('/')?;
    let subtype = rest
        .split(';')
        .next()
        .unwrap_or("")
        .trim_end_matches(is_http_whitespace);
    let token = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c))
    };
    (token(kind) && token(subtype)).then(|| format!("{kind}/{subtype}").to_ascii_lowercase())
}

/// A decoded image file.
#[derive(Clone, Debug)]
pub enum DecodedImage {
    /// A raster image, premultiplied, in its displayed orientation.
    Bitmap(Pixmap),
    /// An SVG document, drawn as vectors (its natural size is the tree's
    /// size).
    Svg(Box<usvg::Tree>),
}

impl DecodedImage {
    /// The image as the backend draws it.
    pub fn as_image(&self) -> Image<'_> {
        match self {
            DecodedImage::Bitmap(p) => Image::Bitmap(p.as_ref()),
            DecodedImage::Svg(tree) => Image::Svg(tree),
        }
    }
}

/// The options an SVG image is parsed with: a browser's defaults for an
/// SVG in an `<img>`. `<image>` elements load from data URLs only (an
/// image document fetches nothing), and a document with no size at all is
/// the replaced-element default of 300 by 150. There are no fonts: add
/// faces to `fontdb` for SVG text.
pub fn svg_options() -> usvg::Options<'static> {
    usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        default_size: usvg::Size::from_wh(300.0, 150.0).expect("a valid size"),
        ..usvg::Options::default()
    }
}

/// Decode an image from a data URL, SVG with [`svg_options`].
pub fn decode_data_url(url: &str) -> Result<DecodedImage, DecodeError> {
    decode_data_url_with(url, &svg_options())
}

/// Decode an image from a data URL, SVG with `options`.
pub fn decode_data_url_with(
    url: &str,
    options: &usvg::Options<'_>,
) -> Result<DecodedImage, DecodeError> {
    let url = parse_data_url(url)?;
    decode_bytes(&url.mime_type, &url.body, options)
}

/// Decode `bytes` served as `mime_type` (an essence, as [`DataUrl`] has
/// it).
pub fn decode_bytes(
    mime_type: &str,
    bytes: &[u8],
    options: &usvg::Options<'_>,
) -> Result<DecodedImage, DecodeError> {
    if mime_type == SVG_MIME_TYPE {
        return usvg::Tree::from_data(bytes, options)
            .map(|tree| DecodedImage::Svg(Box::new(tree)))
            .map_err(|e| DecodeError::Image(e.to_string()));
    }
    decode_raster(bytes).map(DecodedImage::Bitmap)
}

/// The raster formats Chrome decodes, found from the bytes.
fn decode_raster(bytes: &[u8]) -> Result<Pixmap, DecodeError> {
    let error = |e: image::ImageError| DecodeError::Image(e.to_string());
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| DecodeError::Image(e.to_string()))?;
    match reader.format() {
        Some(
            ImageFormat::Png
            | ImageFormat::Jpeg
            | ImageFormat::Gif
            | ImageFormat::WebP
            | ImageFormat::Bmp
            | ImageFormat::Ico,
        ) => {}
        Some(other) => {
            return Err(DecodeError::Image(format!(
                "{other:?} is not a browser image format"
            )))
        }
        None => return Err(DecodeError::Image("unrecognised image data".into())),
    }
    let mut decoder = reader.into_decoder().map_err(error)?;
    let orientation = decoder.orientation().map_err(error)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(error)?;
    image.apply_orientation(orientation);
    let rgba = image.into_rgba8();
    let size = IntSize::from_wh(rgba.width(), rgba.height())
        .ok_or_else(|| DecodeError::Image("an empty image".into()))?;
    let mut data = rgba.into_raw();
    for px in data.chunks_exact_mut(4) {
        let c = tiny_skia::ColorU8::from_rgba(px[0], px[1], px[2], px[3]).premultiply();
        px.copy_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Pixmap::from_vec(data, size).ok_or_else(|| DecodeError::Image("an image too large".into()))
}

/// The decoded files of a scene: upstream's export image cache
/// (`updateImageCache`), by file id.
#[derive(Clone, Debug, Default)]
pub struct ImageFiles {
    files: HashMap<String, Result<(String, Arc<DecodedImage>), DecodeError>>,
}

impl ImageFiles {
    /// Decode each `(id, mime_type, data_url)`, the `BinaryFileData`
    /// entries of the files the scene's image elements name. SVG is parsed
    /// with [`svg_options`].
    pub fn decode<'a>(files: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>) -> ImageFiles {
        ImageFiles::decode_with(files, &svg_options())
    }

    /// [`ImageFiles::decode`] with SVG parsed with `options`.
    pub fn decode_with<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>,
        options: &usvg::Options<'_>,
    ) -> ImageFiles {
        let files = files
            .into_iter()
            .map(|(id, mime_type, data_url)| {
                let decoded = if mime_type == BINARY_MIME_TYPE {
                    Err(DecodeError::NotAnImage)
                } else {
                    decode_data_url_with(data_url, options)
                        .map(|image| (mime_type.to_owned(), Arc::new(image)))
                };
                (id.to_owned(), decoded)
            })
            .collect();
        ImageFiles { files }
    }

    /// The file's `mimeType`, when it loaded.
    pub fn mime_type(&self, id: &str) -> Option<&str> {
        match self.files.get(id)? {
            Ok((mime, _)) => Some(mime),
            Err(_) => None,
        }
    }

    /// Why the file did not load, when it did not.
    pub fn error(&self, id: &str) -> Option<&DecodeError> {
        self.files.get(id)?.as_ref().err()
    }

    /// The ids of the files that loaded, in no particular order.
    pub fn loaded(&self) -> impl Iterator<Item = &str> {
        self.files
            .iter()
            .filter(|(_, f)| f.is_ok())
            .map(|(id, _)| id.as_str())
    }
}

impl ImageStore for ImageFiles {
    fn image(&self, id: &str) -> Option<Image<'_>> {
        match self.files.get(id)? {
            Ok((_, image)) => Some(image.as_image()),
            Err(_) => None,
        }
    }
}

impl ImageStore for HashMap<String, DecodedImage> {
    fn image(&self, id: &str) -> Option<Image<'_>> {
        self.get(id).map(DecodedImage::as_image)
    }
}
