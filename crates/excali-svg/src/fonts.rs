//! What goes in each `@font-face` rule's `url()`.
//!
//! Upstream fetches the face's woff2 file, subsets it to the characters the
//! scene uses (HarfBuzz in a worker) and inlines the result as
//! `data:font/woff2;base64,…`; when no url can be fetched it writes the
//! file's url instead (`ExcalidrawFontFace.getContent`,
//! `packages/excalidraw/fonts/ExcalidrawFontFace.ts:53-86`). A
//! [`FontContent`] answers that for a face; [`FontFiles`] inlines the
//! vendored file whole. Subsetting is the open ex-408 spike: a whole file
//! renders the same glyphs, in a larger document.

use std::path::PathBuf;

use excali_scene::display::FontFaceSource;

/// The content of a face's `src: url(…)`.
pub trait FontContent {
    fn content(&self, face: &FontFaceSource) -> String;
}

/// Font content from the vendored font directory (`excali-text`'s
/// `assets/fonts`, or a copy of it): the face's file as a base64 data URL
/// (`font/woff2`, or `font/ttf` for the TrueType Liberation Sans of
/// ADR-004), or upstream's url for the file when it cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFiles {
    dir: PathBuf,
}

impl FontFiles {
    /// The font directory, holding `<Family>/<file>` as the manifest lists.
    pub fn new(dir: impl Into<PathBuf>) -> FontFiles {
        FontFiles { dir: dir.into() }
    }
}

impl FontContent for FontFiles {
    fn content(&self, face: &FontFaceSource) -> String {
        match std::fs::read(self.dir.join(&face.file)) {
            Ok(bytes) => {
                let mime = match face.format {
                    "truetype" => "font/ttf",
                    _ => "font/woff2",
                };
                format!("data:{mime};base64,{}", base64(&bytes))
            }
            Err(_) => face.fallback_url.clone(),
        }
    }
}

/// Base64 (RFC 4648, standard alphabet, padded), as `btoa` writes a byte
/// string.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let sextet = |shift: u32| char::from(ALPHABET[((n >> shift) & 0x3f) as usize]);
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if chunk.len() > 1 { sextet(6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(0) } else { '=' });
    }
    out
}
