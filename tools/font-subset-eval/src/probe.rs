//! The wasm32 build probe. `wasm.sh` builds this package for
//! wasm32-unknown-unknown as a cdylib with one candidate's features at a
//! time; [`probe`] keeps that candidate's code in the module, so the
//! module's size, less the size built with no features, is what the
//! candidate adds to a wasm build.

#[cfg(any(
    feature = "hb-subset",
    feature = "skera",
    feature = "allsorts",
    feature = "subsetter"
))]
use crate::candidates::subset;
#[cfg(any(
    feature = "hb-subset",
    feature = "skera",
    feature = "allsorts",
    feature = "subsetter"
))]
use crate::candidates::Subsetter;
#[cfg(any(feature = "woofwoof", feature = "ttf2woff2"))]
use crate::candidates::{encode, Encoder};

/// Subsets and encodes `sfnt` with the enabled candidates; the output's
/// length, or 0 on failure.
pub fn run(sfnt: &[u8], code_points: &[u32]) -> usize {
    #[allow(unused_mut)]
    let mut out = sfnt.to_vec();
    #[cfg(feature = "hb-subset")]
    {
        out = subset(Subsetter::HbSubset, &out, code_points).unwrap_or_default();
    }
    #[cfg(feature = "skera")]
    {
        out = subset(Subsetter::Skera, &out, code_points).unwrap_or_default();
    }
    #[cfg(feature = "allsorts")]
    {
        out = subset(Subsetter::Allsorts, &out, code_points).unwrap_or_default();
    }
    #[cfg(feature = "subsetter")]
    {
        out = subset(Subsetter::Subsetter, &out, code_points).unwrap_or_default();
    }
    #[cfg(feature = "woofwoof")]
    {
        out = encode(Encoder::Woofwoof, &out).unwrap_or_default();
    }
    #[cfg(feature = "ttf2woff2")]
    {
        out = encode(Encoder::Ttf2woff2, &out).unwrap_or_default();
    }
    let _ = code_points;
    out.len()
}

/// The exported entry point of the wasm module.
///
/// # Safety
///
/// `sfnt` must point to `sfnt_len` readable bytes and `code_points` to
/// `code_points_len` readable u32s.
#[no_mangle]
pub unsafe extern "C" fn font_subset_probe(
    sfnt: *const u8,
    sfnt_len: usize,
    code_points: *const u32,
    code_points_len: usize,
) -> usize {
    // SAFETY: the caller's contract above.
    let (sfnt, code_points) = unsafe {
        (
            std::slice::from_raw_parts(sfnt, sfnt_len),
            std::slice::from_raw_parts(code_points, code_points_len),
        )
    };
    run(sfnt, code_points)
}
