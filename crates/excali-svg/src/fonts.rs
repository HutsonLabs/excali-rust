//! What goes in each `@font-face` rule's `url()`.
//!
//! Upstream fetches the face's woff2 file, subsets it to the characters the
//! scene uses (HarfBuzz in a worker) and inlines the result as
//! `data:font/woff2;base64,…`; when subsetting fails it inlines the whole
//! file, and when no url can be fetched it writes the file's url instead
//! (`ExcalidrawFontFace.getContent`,
//! `packages/excalidraw/fonts/ExcalidrawFontFace.ts:53-86`;
//! `subset/subset-shared.chunk.ts:25-57`). A [`FontContent`] answers that
//! for a face; [`FontFiles`] does what upstream does with the vendored
//! files, subsetting with [`subset_woff2`] (ADR-010).

use std::path::PathBuf;

use excali_scene::display::FontFaceSource;

/// The content of a face's `src: url(…)`.
pub trait FontContent {
    fn content(&self, face: &FontFaceSource) -> String;
}

/// Font content from the vendored font directory (`excali-text`'s
/// `assets/fonts`, or a copy of it): the face's file subset to the face's
/// characters as a `font/woff2` data URL; the whole file (`font/woff2`, or
/// `font/ttf` for the TrueType Liberation Sans of ADR-004) when it cannot be
/// subset, as `subsetToBase64` falls back; upstream's url for the file when
/// it cannot be read.
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
            Ok(bytes) => match subset_woff2(&bytes, &face.characters) {
                Ok(woff2) => format!("data:font/woff2;base64,{}", base64(&woff2)),
                Err(_) => {
                    let mime = match face.format {
                        "truetype" => "font/ttf",
                        _ => "font/woff2",
                    };
                    format!("data:{mime};base64,{}", base64(&bytes))
                }
            },
            Err(_) => face.fallback_url.clone(),
        }
    }
}

/// Why a face could not be subset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubsetError(pub String);

impl std::fmt::Display for SubsetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SubsetError {}

/// `font` (WOFF2, WOFF or TrueType) subset to `characters` and encoded as
/// WOFF2, as upstream's `subsetToBinary` does (woff2 decompress, hb-subset,
/// woff2 compress; `subset/subset-shared.chunk.ts:44-57`).
///
/// The subset input is upstream's: the characters' code points and every
/// layout feature ("the equivalent of --font-features=*",
/// `subset/harfbuzz/harfbuzz-bindings.ts:74-81`), with HarfBuzz's defaults
/// otherwise (`hb_subset_input_create_or_fail`: the tables it drops, name
/// ids 0 to 6 in English, every script). skera 0.7.0, fontations' port of
/// hb-subset, subsets; ttf2woff2 0.13.3 encodes at brotli quality 11 with
/// the glyf/loca transform, as Google's woff2 does. Both are Rust and build
/// for wasm32-unknown-unknown; `tools/font-subset-eval` measured them
/// against upstream's own subsets (ADR-010).
pub fn subset_woff2(font: &[u8], characters: &str) -> Result<Vec<u8>, SubsetError> {
    use skera::{Plan, SubsetFlags};
    use skrifa::raw::collections::IntSet;
    use skrifa::raw::types::{GlyphId, NameId, Tag};
    use skrifa::raw::TableProvider;
    use skrifa::FontRef;

    let sfnt = match font.get(..4) {
        Some(b"wOF2") => {
            wuff::decompress_woff2(font).map_err(|e| SubsetError(format!("woff2: {e:?}")))?
        }
        Some(b"wOFF") => {
            wuff::decompress_woff1(font).map_err(|e| SubsetError(format!("woff: {e:?}")))?
        }
        _ => font.to_vec(),
    };
    let face = FontRef::new(&sfnt).map_err(|e| SubsetError(format!("font: {e}")))?;
    // skera panics where hb_subset_or_fail returns 0: without a maxp table,
    // and with a loca table but no glyf (skera 0.7.0 src/lib.rs:581, 978).
    // A panic aborts a wasm32 module; answer with an error, as upstream's
    // subsetToBase64 catches HarfBuzz's.
    face.maxp().map_err(|e| SubsetError(format!("maxp: {e}")))?;
    if face.loca(None).is_ok() {
        face.glyf().map_err(|e| SubsetError(format!("glyf: {e}")))?;
    }
    // `Array.from(characters).map((char) => char.codePointAt(0))`
    let unicodes: IntSet<u32> = characters.chars().map(u32::from).collect();
    // hb-subset-input.cc's default drop list.
    let drop_tables: IntSet<Tag> = [
        skera::MORX,
        skera::MORT,
        skera::KERX,
        skera::KERN,
        skera::JSTF,
        skera::DSIG,
        Tag::new(b"EBDT"),
        Tag::new(b"EBLC"),
        skera::EBSC,
        Tag::new(b"SVG "),
        Tag::new(b"PCLT"),
        skera::LTSH,
        Tag::new(b"Feat"),
        skera::GLAT,
        skera::GLOC,
        Tag::new(b"Silf"),
        Tag::new(b"Sill"),
    ]
    .into_iter()
    .collect();
    let mut name_ids = IntSet::<NameId>::empty();
    name_ids.insert_range(NameId::from(0)..=NameId::from(6));
    let mut name_languages = IntSet::<u16>::empty();
    name_languages.insert(0x0409);
    let mut scripts = IntSet::<Tag>::empty();
    scripts.invert();
    let mut features = IntSet::<Tag>::empty();
    features.invert();
    let plan = Plan::new(
        &IntSet::<GlyphId>::empty(),
        &unicodes,
        &face,
        SubsetFlags::default(),
        &drop_tables,
        &scripts,
        &features,
        &name_ids,
        &name_languages,
    );
    let subset =
        skera::subset_font(&face, &plan).map_err(|e| SubsetError(format!("subset: {e}")))?;
    // Single-threaded brotli (the default), so the output is the same on
    // every run.
    ttf2woff2::encode_with_options(&subset, ttf2woff2::EncodeOptions::default())
        .map_err(|e| SubsetError(format!("woff2 encode: {e}")))
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
