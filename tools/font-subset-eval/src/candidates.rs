//! The candidates: four subsetters and two WOFF2 encoders, each called the
//! way upstream calls HarfBuzz.
//!
//! Upstream's worker decompresses the face's woff2, subsets it with
//! harfbuzzjs 0.3.6's `hb_subset_or_fail` and compresses the result to woff2
//! again (`subset/subset-shared.chunk.ts:44-57`). The subset input is
//! HarfBuzz's default (`hb_subset_input_create_or_fail`) with the requested
//! unicodes and every layout feature kept, "the equivalent of
//! --font-features=*" (`subset/harfbuzz/harfbuzz-bindings.ts:74-81, 116-120`).
//! Each subsetter here gets the same input, as far as its API has one.

/// A subsetter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Subsetter {
    /// `hb-subset` 0.3.0: Rust bindings to HarfBuzz 8.2.2's hb-subset, the
    /// C++ library harfbuzzjs compiles to wasm.
    HbSubset,
    /// `skera` 0.7.0: fontations' port of hb-subset to Rust (formerly klippa).
    Skera,
    /// `allsorts` 0.17.0: YesLogic's font parser and subsetter.
    Allsorts,
    /// `subsetter` 0.2.6: typst's subsetter, for PDF embedding.
    Subsetter,
}

impl Subsetter {
    pub const ALL: [Subsetter; 4] = [
        Subsetter::HbSubset,
        Subsetter::Skera,
        Subsetter::Allsorts,
        Subsetter::Subsetter,
    ];

    /// The crate and the version pinned in Cargo.toml.
    pub fn name(self) -> &'static str {
        match self {
            Subsetter::HbSubset => "hb-subset 0.3.0",
            Subsetter::Skera => "skera 0.7.0",
            Subsetter::Allsorts => "allsorts 0.17.0",
            Subsetter::Subsetter => "subsetter 0.2.6",
        }
    }

    /// Short id, used in candidate names.
    pub fn id(self) -> &'static str {
        match self {
            Subsetter::HbSubset => "hb-subset",
            Subsetter::Skera => "skera",
            Subsetter::Allsorts => "allsorts",
            Subsetter::Subsetter => "subsetter",
        }
    }
}

/// A WOFF2 encoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Encoder {
    /// `woofwoof` 1.0.2: Google's woff2 encoder (C++, the library upstream's
    /// wasm is built from) over the Rust brotli crate.
    Woofwoof,
    /// `ttf2woff2` 0.13.3: a WOFF2 encoder in Rust.
    Ttf2woff2,
}

impl Encoder {
    pub const ALL: [Encoder; 2] = [Encoder::Woofwoof, Encoder::Ttf2woff2];

    pub fn name(self) -> &'static str {
        match self {
            Encoder::Woofwoof => "woofwoof 1.0.2",
            Encoder::Ttf2woff2 => "ttf2woff2 0.13.3",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Encoder::Woofwoof => "woofwoof",
            Encoder::Ttf2woff2 => "ttf2woff2",
        }
    }
}

/// The sfnt (TrueType or OpenType) bytes of a font file: WOFF2 and WOFF
/// decoded with wuff, as `excali_text::font_store` decodes the vendored
/// files; anything else returned as it is.
pub fn decode(font: &[u8]) -> Result<Vec<u8>, String> {
    match font.get(..4) {
        Some(b"wOF2") => wuff::decompress_woff2(font).map_err(|e| format!("woff2 decode: {e:?}")),
        Some(b"wOFF") => wuff::decompress_woff1(font).map_err(|e| format!("woff decode: {e:?}")),
        _ => Ok(font.to_vec()),
    }
}

/// Subsets `sfnt` to `code_points` with `subsetter`.
pub fn subset(subsetter: Subsetter, sfnt: &[u8], code_points: &[u32]) -> Result<Vec<u8>, String> {
    match subsetter {
        Subsetter::HbSubset => hb_subset(sfnt, code_points),
        Subsetter::Skera => skera(sfnt, code_points),
        Subsetter::Allsorts => allsorts(sfnt, code_points),
        Subsetter::Subsetter => typst_subsetter(sfnt, code_points),
    }
}

/// Encodes `sfnt` as WOFF2 with `encoder`, at brotli quality 11, with the
/// glyf/loca transform, as Google's woff2 does by default
/// (`woff2::WOFF2Params`, the parameters upstream's wasm is built with).
pub fn encode(encoder: Encoder, sfnt: &[u8]) -> Result<Vec<u8>, String> {
    match encoder {
        Encoder::Woofwoof => woofwoof(sfnt),
        Encoder::Ttf2woff2 => ttf2woff2(sfnt),
    }
}

#[allow(dead_code)]
fn not_built(what: &str) -> Result<Vec<u8>, String> {
    Err(format!("{what}: feature not enabled"))
}

#[cfg(feature = "hb-subset")]
fn hb_subset(sfnt: &[u8], code_points: &[u32]) -> Result<Vec<u8>, String> {
    use hb_subset::{Blob, FontFace, SubsetInput};
    let mut input =
        SubsetInput::new().map_err(|e| format!("hb_subset_input_create_or_fail: {e:?}"))?;
    {
        // hb_set_clear then hb_set_invert: every layout feature
        // (harfbuzz-bindings.ts:74-81).
        let features = input.layout_feature_tag_set();
        // SAFETY: the set belongs to `input`, alive for this block.
        unsafe {
            hb_subset::sys::hb_set_clear(features.as_raw());
            hb_subset::sys::hb_set_invert(features.as_raw());
        }
    }
    {
        let mut unicodes = input.unicode_set();
        for &cp in code_points {
            if let Some(c) = char::from_u32(cp) {
                unicodes.insert(c);
            }
        }
    }
    let face = FontFace::new(Blob::from_bytes(sfnt).map_err(|e| format!("hb_blob_create: {e:?}"))?)
        .map_err(|e| format!("hb_face_create: {e:?}"))?;
    let out = input
        .subset_font(&face)
        .map_err(|e| format!("hb_subset_or_fail: {e:?}"))?;
    let bytes = out.underlying_blob().to_vec();
    if bytes.is_empty() {
        return Err("hb_subset_or_fail: empty subset".into());
    }
    Ok(bytes)
}

#[cfg(not(feature = "hb-subset"))]
fn hb_subset(_: &[u8], _: &[u32]) -> Result<Vec<u8>, String> {
    not_built("hb-subset")
}

#[cfg(feature = "skera")]
fn skera(sfnt: &[u8], code_points: &[u32]) -> Result<Vec<u8>, String> {
    use skera::{Plan, SubsetFlags};
    use skrifa::raw::collections::IntSet;
    use skrifa::raw::types::{GlyphId, NameId, Tag};
    use skrifa::FontRef;

    let font = FontRef::new(sfnt).map_err(|e| format!("FontRef: {e}"))?;
    let unicodes: IntSet<u32> = code_points.iter().copied().collect();
    // hb_subset_input_create_or_fail's defaults (hb-subset-input.cc), as
    // skera's own command line spells them out (skera 0.7.0 src/main.rs).
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
    // Every layout feature, as upstream asks for.
    let mut features = IntSet::<Tag>::empty();
    features.invert();
    let plan = Plan::new(
        &IntSet::<GlyphId>::empty(),
        &unicodes,
        &font,
        SubsetFlags::default(),
        &drop_tables,
        &scripts,
        &features,
        &name_ids,
        &name_languages,
    );
    skera::subset_font(&font, &plan).map_err(|e| format!("subset_font: {e}"))
}

#[cfg(not(feature = "skera"))]
fn skera(_: &[u8], _: &[u32]) -> Result<Vec<u8>, String> {
    not_built("skera")
}

/// The glyphs of `code_points` in `sfnt`'s cmap, .notdef first, sorted and
/// deduplicated: what allsorts and subsetter take instead of unicodes.
pub fn glyph_ids(sfnt: &[u8], code_points: &[u32]) -> Result<Vec<u16>, String> {
    let face = ttf_parser::Face::parse(sfnt, 0).map_err(|e| format!("parse: {e}"))?;
    let mut ids: Vec<u16> = code_points
        .iter()
        .filter_map(|&cp| char::from_u32(cp))
        .filter_map(|c| face.glyph_index(c))
        .map(|g| g.0)
        .collect();
    ids.push(0);
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

#[cfg(feature = "allsorts")]
fn allsorts(sfnt: &[u8], code_points: &[u32]) -> Result<Vec<u8>, String> {
    use allsorts::binary::read::ReadScope;
    use allsorts::font_data::FontData;
    use allsorts::subset::{subset, CmapTarget, SubsetProfile};
    use allsorts::tag;

    let ids = glyph_ids(sfnt, code_points)?;
    let font = ReadScope::new(sfnt)
        .read::<FontData<'_>>()
        .map_err(|e| format!("read: {e}"))?;
    let provider = font
        .table_provider(0)
        .map_err(|e| format!("table_provider: {e}"))?;
    // The Minimal profile (the tables OpenType requires) and the hinting
    // tables. allsorts writes no GSUB, GPOS or GDEF.
    let profile = SubsetProfile::Custom(vec![
        tag::CMAP,
        tag::HEAD,
        tag::HHEA,
        tag::HMTX,
        tag::MAXP,
        tag::NAME,
        tag::OS_2,
        tag::POST,
        tag::CVT,
        tag::FPGM,
        tag::PREP,
    ]);
    // A Unicode cmap: allsorts' default picks Mac Roman when every glyph
    // fits, which "browsers reject" (allsorts' own CmapTarget docs).
    subset(&provider, &ids, &profile, CmapTarget::Unicode).map_err(|e| format!("subset: {e}"))
}

#[cfg(not(feature = "allsorts"))]
fn allsorts(_: &[u8], _: &[u32]) -> Result<Vec<u8>, String> {
    not_built("allsorts")
}

#[cfg(feature = "subsetter")]
fn typst_subsetter(sfnt: &[u8], code_points: &[u32]) -> Result<Vec<u8>, String> {
    let ids = glyph_ids(sfnt, code_points)?;
    let remapper = subsetter::GlyphRemapper::new_from_glyphs_sorted(&ids);
    subsetter::subset(sfnt, 0, &remapper).map_err(|e| format!("subset: {e}"))
}

#[cfg(not(feature = "subsetter"))]
fn typst_subsetter(_: &[u8], _: &[u32]) -> Result<Vec<u8>, String> {
    not_built("subsetter")
}

#[cfg(feature = "woofwoof")]
fn woofwoof(sfnt: &[u8]) -> Result<Vec<u8>, String> {
    woofwoof::compress(sfnt, "", 11, true).ok_or_else(|| "woofwoof::compress failed".to_string())
}

#[cfg(not(feature = "woofwoof"))]
fn woofwoof(_: &[u8]) -> Result<Vec<u8>, String> {
    not_built("woofwoof")
}

#[cfg(feature = "ttf2woff2")]
fn ttf2woff2(sfnt: &[u8]) -> Result<Vec<u8>, String> {
    // Single-threaded (threads: None), so the output is deterministic.
    let options = ttf2woff2::EncodeOptions::default();
    ttf2woff2::encode_with_options(sfnt, options).map_err(|e| format!("ttf2woff2: {e}"))
}

#[cfg(not(feature = "ttf2woff2"))]
fn ttf2woff2(_: &[u8]) -> Result<Vec<u8>, String> {
    not_built("ttf2woff2")
}
