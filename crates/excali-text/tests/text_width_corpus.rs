//! ex-308: stored against measured text widths across the whole ex-003
//! corpus, the 232 catalogue libraries of `fixtures/libraries` and the
//! scene-bearing files of upstream's test fixtures (`fixtures/upstream`),
//! both as `fixtures/manifest.json` lists them.
//!
//! Upstream stores each text element's `width` and `height` as the writing
//! browser measured them: `measureText(text, getFontString(element),
//! lineHeight)` (`packages/element/src/textMeasurements.ts:12-27`), called
//! through `refreshTextDimensions` and `getAdjustedDimensions`
//! (`packages/element/src/newElement.ts:533-580`). The port measures every
//! text of a vendored family the same way with [`FontStore`] and compares.
//!
//! Every text of a vendored family falls in one class, decided without
//! looking at its width:
//!
//! - `measured`: has `lineHeight` and `autoResize` is not false, and its
//!   stored `height` is what `measureText` gives, `fontSize * lineHeight *
//!   lines` (`getTextHeight`, `textMeasurements.ts:170-177`), within the
//!   tolerance. Its `width` is a browser measurement of the same text.
//! - `height_mismatch`: has `lineHeight` and `autoResize`, but a `height`
//!   `measureText` cannot give, so the element was not written by
//!   upstream's measurement (a generator wrote it); its width is reported,
//!   not gated.
//! - `legacy`: no `lineHeight`, written before upstream measured on a
//!   canvas with a unitless line height; restore detects one from the
//!   height (`packages/excalidraw/data/restore.ts:549-560`). Reported, not
//!   gated.
//! - `fixed_width`: `autoResize: false`, whose `width` is the width the
//!   user wrapped to, not a measurement. Counted.
//!
//! The report, `tests/fixtures/text-width-corpus-report.json`, gives the
//! count, the number within 0.5 px, and the max and mean |measured -
//! stored| per family and class, and this test rebuilds and compares it
//! (`EXCALI_BLESS=1` rewrites it after a deliberate change; the diff is the
//! review). For Virgil, Excalifont, Nunito and Comic Shanns (ADR-007)
//! every `measured` text must be within 0.5 px, and so must the family's
//! mean; the only exceptions are the texts of [`KNOWN_DEVIATIONS`], each
//! pinned to its stored and measured width and to the [`Cause`] that wrote
//! it, which the test recomputes.

mod common;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use excali_core::element::FontFamily;
use excali_core::png::decode_png_metadata;
use excali_core::svg_payload::decode_svg_base64_payload;
use excali_text::font_metadata::get_font_string;
use excali_text::font_store::decode_font_file;
use excali_text::text_measurements::measure_text;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const TOLERANCE: f64 = 0.5;
const REPORT: &str = "tests/fixtures/text-width-corpus-report.json";

/// Virgil, Excalifont, Nunito, Comic Shanns: the families whose `measured`
/// texts are held to [`TOLERANCE`] (ADR-007).
const GATED_FAMILIES: [u32; 4] = [1, 5, 6, 8];

/// The texts of the gated families that are `measured` by class and still
/// deviate by more than 0.5 px: `(file under fixtures/, element id, stored
/// width, measured width, cause)`.
///
/// Every stored width is wider than the text measures today, and every
/// cause but [`Cause::KeptWidth`] is an earlier upstream measurement that
/// `known_deviations_are_what_their_cause_writes` recomputes from the
/// vendored fonts to within 0.001 px (the DOM ones exactly): the
/// `offsetWidth` measurement upstream used until `9659254f` (2023-02-23),
/// in its two variants and after a resize, a bound text laid out at its
/// container's width before `4cb6f095` (2022-09-22), and the ink-box
/// `getLineWidth` of `62228e0b` (2024-07-25) to `e3060dfb` (2025-02-11)
/// with the ink box reported three ways. Restore keeps a stored width
/// unless asked to refresh dimensions (`restore.ts:1033-1046`), and so does
/// the port on load (ADR-007); on edit both measure today's advance width.
///
/// All 94 Virgil texts have such a cause. Five Excalifont and Comic Shanns
/// texts have none ([`Cause::KeptWidth`]): each was measured again
/// (2026-09-28) in every vendored family at its font size, in its own
/// family without kerning, with every [`Cause`] model, and with the
/// Excalifont and Comic Shanns builds upstream shipped before
/// `61623bbeba` (2024-10-20; the single-file
/// `packages/excalidraw/fonts/assets/*-Regular.woff2` at `a80cb5896a`),
/// and nothing gives their widths (`A` is stored as a whole 25 px, `Alarms`
/// and `Oracle\nAutonomous\nDatabase` exactly 1 px wider than the per-glyph
/// ink box). That fits a width kept from an earlier state of the element
/// better than a measuring error, but it is not shown.
///
/// A text that starts to measure within 0.5 px must be removed from the
/// list, and a new deviation fails the gate.
const KNOWN_DEVIATIONS: &[(&str, &str, f64, f64, Cause)] = &[
    // Virgil (1)
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "0JRcBSO_gKMJBazHfxAl5",
        103.8600082397461,
        102.86,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "KGpnaWujf-puo-oaNV0qu",
        143.5,
        142.5,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "rGURyQjUp70vj7So_WBL0",
        107.14000701904297,
        106.14,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "m0uvWqSaJRPECfyLSUvGH",
        136.25999450683594,
        135.26,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "PG9muBDxU8Hh77wIRcGtu",
        96.07999420166016,
        95.08,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "_LJgifPgEA3C-zj1VhkgA",
        58.78000259399414,
        55.78,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "dtHwYAIJu-13X35LNNi6Y",
        109.50000762939453,
        108.5,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "IjWOpBn7bNvJrkpCPuYNJ",
        131.86000061035156,
        130.86,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "7Pp35atG3G5vHW-3XRqpM",
        156.63999938964844,
        155.64,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "fWykqsSesmp54v7rW9O7p",
        70.34000396728516,
        68.34,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "S7kh-UnJCL1TuSUY2hReC",
        98.63999938964844,
        97.64,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "j7xALRrIexOFUYEiJoTd3",
        158.3199920654297,
        157.32,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "n_JDfl_g6uQDrqVKGbUcw",
        80.04000091552734,
        79.04,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "u_uhh3cfmkMM66sXwtBTU",
        58.86000061035156,
        57.86,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "xhQ10Dk-SS6b7H8WKRJTy",
        92.20000457763672,
        91.2,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "kV40coNBHgOxUNfFtSXZH",
        52.019996643066406,
        50.02,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "6l6iMXkjj9hfubtTZAtY1",
        68.52000427246094,
        67.52,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "UsC36xWMGRdXNDrxvt7CI",
        133.27999877929688,
        132.28,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "duplSvMz1eOw6FdTpM05f",
        42.119998931884766,
        41.12,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "jo3rSyijSdJ6n371iB9IP",
        91.56000518798828,
        90.56,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "itFmV-sk7NxvCvHyMAdsz",
        110.73999786376953,
        109.74,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "026Kun7u_LYI6n-CyHSm2",
        110.41999053955078,
        108.42,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "3SOkju7gYyVeG4PTWWgVp",
        92.33999633789062,
        91.34,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "UEwP82gJFZqYzzZato7Nk",
        160.3800048828125,
        159.38,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "1uR8aa9lbwX2WUvcu4cpL",
        168.02001953125,
        167.02,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "ScekL0QomoQCfwlJ-OEjF",
        169.52000427246094,
        168.52,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "VY2cvZbn-GYu1zKtdrx4E",
        161.52000427246094,
        160.52,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "4UrgpAQ0P4tskB24urtBJ",
        41.84000015258789,
        40.84,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "hI_kvnC-eTmApf1FEkcVx",
        129.25999450683594,
        128.26,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "XJfpCe3R8fKC6hsJTEyIK",
        191.05999755859375,
        190.06,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "THWNHGv_rtwEZuKeO2933",
        82.70000457763672,
        81.7,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "vg_7jzFraD46NANjmsIAx",
        120.0,
        119.24,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "yKx_tNXOKttAdgFX2aiRs",
        43.0,
        40.679,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "b0HjIfccJDbGzOD0oJTqW",
        43.0,
        40.679,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "v3JV0jo1y8ri7u-ezyS3J",
        43.0,
        40.679,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "crYgsUPUOuoGa3B3yWSmU",
        38.0,
        37.26,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "NGYrle_mTo26iQ1bDz4M6",
        43.29999923706055,
        42.3,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "mk0-Mdu4ulTVe7X9VAi1N",
        60.0,
        59.32,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "Hwjeq64YjpS2TC6hOJWkh",
        125.7400131225586,
        124.74,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "uZDG6Xf8faPMwtOC9NHGI",
        128.0,
        126.7,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/erlina/data-processing.excalidrawlib.gz",
        "NPzVqC8DV7s17WxsuEJk_",
        43.44949194141259,
        40.86,
        Cause::ScaledDomOffsetWidth {
            px: 39,
            plus_one: true,
        },
    ),
    (
        "libraries/gabrielamacakova/halloween-elements.excalidrawlib.gz",
        "EzBbdxqivZ8QhmV67VrnF",
        26.082033226856986,
        25.382,
        Cause::ScaledDomOffsetWidth {
            px: 48,
            plus_one: false,
        },
    ),
    (
        "libraries/gabrielamacakova/halloween-elements.excalidrawlib.gz",
        "rx9xT52592PqRrVqDy5kq",
        31.515790149118853,
        30.784,
        Cause::ScaledDomOffsetWidth {
            px: 58,
            plus_one: false,
        },
    ),
    (
        "libraries/gabrielamacakova/halloween-elements.excalidrawlib.gz",
        "Yq_9tkFIFmnNSEQCtu_gl",
        19.682760747889198,
        19.154,
        Cause::ScaledDomOffsetWidth {
            px: 48,
            plus_one: false,
        },
    ),
    (
        "libraries/gabrielamacakova/halloween-elements.excalidrawlib.gz",
        "v5hy6FLFxs3QvXlyPRpK0",
        23.78333590369944,
        23.231,
        Cause::ScaledDomOffsetWidth {
            px: 58,
            plus_one: false,
        },
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "Ai0jv2vBdOIZUkYOjDZdz",
        313.759730219841,
        313.24,
        Cause::InkBox(Ink::Exact),
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "7ExgwdP8Ry41GVZAiXzen",
        245.84391552209854,
        244.62,
        Cause::InkBox(Ink::Exact),
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "dDlDHKYYP2k4hMDK05QC2",
        155.17593908309937,
        153.048,
        Cause::InkBox(Ink::Exact),
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "XBDtnVgCPkwD_lLW0pbG0",
        98.63993012905121,
        97.96,
        Cause::InkBox(Ink::Exact),
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "kDGfu_d9JBFZC-eMtRrQn",
        158.35986077785492,
        157.6,
        Cause::InkBox(Ink::Exact),
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "cleW_j4SllhjLikgaQ7Y-",
        107.9799053966999,
        107.28,
        Cause::InkBox(Ink::Exact),
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "bZRkonncf_xINk1YXivYs",
        74.91195410490036,
        74.0,
        Cause::InkBox(Ink::Exact),
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "cNJNEAfu-mAf3iuWRs7Hl",
        129.43988341093063,
        128.6,
        Cause::InkBox(Ink::Exact),
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "zPt1vdWcfV0U6qTeFwvqQ",
        73.0,
        71.44,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "4F4iRcCo-63GtJr5xmT4d",
        39.0,
        37.232,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "KUvdjejdRDVTCqcI7bE4y",
        160.0,
        158.44,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "bKuB0x46YysJPYBtvVFDw",
        38.078209181986566,
        37.232,
        Cause::ScaledDomOffsetWidth {
            px: 92,
            plus_one: true,
        },
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "sD-TUGTrTnjM5PCRcDr_m",
        64.0,
        61.7,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "_ZrRanQkZaMGg_98NfYcw",
        39.0,
        37.232,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "_S5sXU3j0JBXsptKfzJa3",
        92.0,
        89.956,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "Y8SrMyXIof1WKbEfwq0Me",
        74.0,
        72.24,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "yhsOGotiHG9vbMrS23eiC",
        56.0,
        54.42,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "w4K_7dkA_4tcZpqraCAWS",
        38.0,
        35.78,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "Lf9ubXISAq7xb7rW5qXjN",
        46.0,
        43.74,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "w3yVI0yBdVFOd4FFO6X7j",
        87.0,
        84.726,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "k0DHmaQpBmdgY-c6KH68e",
        49.0,
        47.216,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/infamousjoeg/cyberark.excalidrawlib.gz",
        "gfMbCz4eVWHzzjSebdG9o",
        28.0,
        26.1,
        Cause::DomOffsetWidthPlusOne,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "JA4CWhChhKQuYXuq8R3y-",
        203.0,
        86.26,
        Cause::ContainerWidth,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "0txHLBnPaLdbC-6oBR5Nc",
        78.0,
        77.008,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "GHgonuJ-4BpYWc45yBXyV",
        58.0,
        57.08,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "Rux2x4t2eZD_IGJdiVVLi",
        58.0,
        57.08,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "P7W_lCMx6UrdVeWVX6mPg",
        78.0,
        77.008,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "j-7bmxUrNpmryK2lf0Tye",
        55.0,
        53.904,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "uXu53U7-3eUe4fieMfYJj",
        55.0,
        53.904,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "esmwWoflQ6lHYdt6r2lJ9",
        100.0,
        98.88,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "M1CFTYVU0kZhawVm1DnJZ",
        107.0,
        105.856,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "0XgLS_x0P0NFlolAyltTu",
        78.0,
        77.008,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "CrTBdkdD6bD_uOC81RfJz",
        55.0,
        53.904,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "R50TfCqfoj-BTvf2yrEi7",
        55.0,
        53.904,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/pratheeshpm/basic-system-design.excalidrawlib.gz",
        "mzKXC_i4q_CKOUKxaNHCf",
        100.0,
        98.88,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "ZS_5Ir5d4kbWmXs-m8L1_",
        99.0,
        98.448,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "NbIy6eIDsKO1igUZaMSB6",
        100.0,
        99.168,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "gqJ1RHbaIkEhYxrR4Ozuz",
        108.0,
        106.8,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "rVl-wrsBP-LVOCQEBgtmn",
        82.0,
        81.232,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "oXtk3I7XPc6BTLgTSHjMy",
        93.0,
        92.304,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "y3ysQIwsKjYzw9HfoyoHk",
        96.0,
        94.544,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "0YXrCqAaxqzIeHee0kVTy",
        86.0,
        85.408,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "ZrvzmoYKDdyp_9vcshcqg",
        84.0,
        83.008,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "6YpWOhFlj6o9250kO4Zfz",
        96.0,
        95.024,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "syd-pcs8a7hGFqOmxXH6Q",
        90.0,
        89.152,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "kgxsBAsfAvP9OaUEfDKPG",
        116.0,
        114.752,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "6Qn7igOMwRE2ekgcmcUWb",
        93.0,
        91.664,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "jAnR8WvZYPDLMbUl3PBkT",
        116.0,
        115.2,
        Cause::DomOffsetWidth,
    ),
    (
        "libraries/stojanovic/aws-serverless-icons-v2.excalidrawlib.gz",
        "KVNCfIQjm95Hg6bcTSOxn",
        95.0,
        94.48,
        Cause::DomOffsetWidth,
    ),
    // Excalifont (5)
    (
        "libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz",
        "tSfxrqkItq3SZLhIbkX8I",
        10.807999610900879,
        9.808,
        Cause::InkBox(Ink::WholePixels),
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "Vw9cbJ_YyXodALTOaKDJl",
        167.9038791656494,
        166.96,
        Cause::InkBox(Ink::Exact),
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "ZtJ-xO5KFQFAKmKZw52Sk",
        101.34393501281738,
        100.32,
        Cause::InkBox(Ink::Exact),
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "3hvYeGmB_6MXefzOj3ju5",
        167.9038791656494,
        166.96,
        Cause::InkBox(Ink::Exact),
    ),
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "g3eA4wT2HZVZ3tORzoFoa",
        130.2078845500946,
        129.52,
        Cause::InkBox(Ink::Exact),
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "b13s8Yg3H6MAv5DP57EQu",
        112.87196350097656,
        111.42,
        Cause::KeptWidth,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "4hdzG83Rxx1fldGChWrTe",
        123.52796936035156,
        122.076,
        Cause::InkBox(Ink::GlyphPixels),
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "bcj9TbntkjxAbnJT_A8Kq",
        153.6599578857422,
        151.74,
        Cause::InkBox(Ink::GlyphPixels),
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "MGLV7_xBOL0gssIKcdHJa",
        153.6599578857422,
        151.74,
        Cause::InkBox(Ink::GlyphPixels),
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "dmlv3N_9yOdMLcbvn0XXr",
        153.6599578857422,
        151.74,
        Cause::InkBox(Ink::GlyphPixels),
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "iZXwggIULIN6zn5T-JunY",
        329.6038818359375,
        329.076,
        Cause::KeptWidth,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "ycFqvLAQT_EpoP5IbUbMZ",
        218.53192138671875,
        216.864,
        Cause::InkBox(Ink::GlyphPixels),
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "oLC62R0OYAZYsSLIqv04a",
        74.31198120117188,
        72.108,
        Cause::InkBox(Ink::GlyphPixels),
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "nANmHqm_Fgj1adE7oRI0H",
        212.419921875,
        211.752,
        Cause::InkBox(Ink::GlyphPixels),
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "bMz65OYUyPcq4ZMyzw3nX",
        209.21194458007812,
        208.044,
        Cause::InkBox(Ink::GlyphPixels),
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "ZNbPoQfsdlJcHOS4dgTd9",
        212.30393981933594,
        210.852,
        Cause::KeptWidth,
    ),
    (
        "libraries/martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib.gz",
        "HcRiKbkgPSdROBEureNmK",
        25.0,
        24.336,
        Cause::KeptWidth,
    ),
    // Comic Shanns (8)
    (
        "libraries/hartmut-co-uk/kafka-streams-topology-design.excalidrawlib.gz",
        "19KTHBlJtvmGlR4K0BJ3U",
        11.519999623298645,
        11.0,
        Cause::KeptWidth,
    ),
];

/// The libraries with `height_mismatch` texts and how many: Nunito texts
/// of a generated library, whose heights are
/// `fontSize * 1.4` under a stored `lineHeight` of 1.25 and whose widths
/// are `0.6 * fontSize` per character.
const HEIGHT_MISMATCH: &[(&str, u32, usize)] = &[(
    "libraries/datavizfairy/dashboard-charts.excalidrawlib.gz",
    6,
    328,
)];

/// `BOUND_TEXT_PADDING`, `packages/common/src/constants.ts:421` at the pin,
/// 5 since `5c67329b` (2022-01-03).
const BOUND_TEXT_PADDING: f64 = 5.0;

/// What wrote the stored width of a [`KNOWN_DEVIATIONS`] text. Every cause
/// but [`Cause::KeptWidth`] is a width an earlier upstream `measureText` or
/// bound-text layout computes; [`Cause::width`] recomputes it from the
/// vendored fonts and `known_deviations_are_what_their_cause_writes`
/// requires it to equal the stored width.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Cause {
    /// No earlier measurement found: a width kept from an earlier state of
    /// the element (see [`KNOWN_DEVIATIONS`]); only the stored and measured
    /// widths are pinned.
    KeptWidth,
    /// `measureText` of `3a141ca7^` (2023-01-30) and before: the
    /// `offsetWidth` of an absolutely positioned `white-space: pre` div
    /// holding the text and, after its last line, a 1 px inline-block span
    /// (`src/element/textElement.ts:258-296` at `3a141ca7^`). The widest
    /// line (the last one 1 px wider), ceiled to a 1/64 px layout unit and
    /// rounded to a whole pixel.
    DomOffsetWidth,
    /// `measureText` of `3a141ca7` (2023-01-30) to `9659254f^` (2023-02-23):
    /// the same `offsetWidth` plus 1 ("since we are adding a span of width
    /// 1px").
    DomOffsetWidthPlusOne,
    /// A [`Cause::DomOffsetWidth`] (`plus_one`: [`Cause::DomOffsetWidthPlusOne`])
    /// width of `px` whole pixels at an earlier font size, then resized:
    /// text resizing scales `width` and `fontSize` by the same factor and
    /// does not measure (`resizeSingleTextElement`). The earlier font size
    /// is `fontSize * px / width`; there the text measures `px` and the
    /// stored `height` scales back to whole pixels, `offsetHeight`.
    ScaledDomOffsetWidth { px: u32, plus_one: bool },
    /// `getLineWidth` of `62228e0b` (2024-07-25) to `e3060dfb^`
    /// (2025-02-11): `max(|actualBoundingBoxLeft| + |actualBoundingBoxRight|,
    /// width)`, the ink box where it is wider than the advance
    /// (`packages/excalidraw/element/textElement.ts` at `62228e0b`), with
    /// the ink box as the writing browser reported it ([`Ink`]). Which
    /// browser reported which ink box is inferred, not shown; the formula
    /// is the measurement.
    InkBox(Ink),
    /// A bound text of `5c67329b` (2022-01-03) to `4cb6f095^` (2022-09-22),
    /// laid out at the container's width: `measureText` with `maxWidth` set
    /// the div's `style.width` to it, and `handleBindTextResize` wrote
    /// `width: container.width - BOUND_TEXT_PADDING * 2`, `x: container.x +
    /// BOUND_TEXT_PADDING` (`src/element/textElement.ts` at `4cb6f095^`).
    ContainerWidth,
}

impl Cause {
    fn key(self) -> &'static str {
        match self {
            Cause::KeptWidth => "kept_width",
            Cause::DomOffsetWidth => "dom_offset_width",
            Cause::DomOffsetWidthPlusOne => "dom_offset_width_plus_one",
            Cause::ScaledDomOffsetWidth { .. } => "scaled_dom_offset_width",
            Cause::InkBox(Ink::Exact) => "ink_box",
            Cause::InkBox(Ink::WholePixels) => "ink_box_whole_pixels",
            Cause::InkBox(Ink::GlyphPixels) => "ink_box_glyph_pixels",
            Cause::ContainerWidth => "container_width",
        }
    }

    /// The width this cause writes for `t`, `None` for
    /// [`Cause::KeptWidth`] and for a bound text whose container is not in
    /// its item. Panics if a scaled width's earlier state fails its own
    /// check (whole-pixel height, `px` measured there).
    fn width(self, t: &Text) -> Option<f64> {
        let lines: Vec<&str> = t.text.split('\n').collect();
        match self {
            Cause::KeptWidth => None,
            Cause::DomOffsetWidth => Some(dom_offset_width(&lines, t.family, t.font_size)),
            Cause::DomOffsetWidthPlusOne => {
                Some(dom_offset_width(&lines, t.family, t.font_size) + 1.0)
            }
            Cause::ScaledDomOffsetWidth { px, plus_one } => {
                let px = f64::from(px);
                let scale = t.stored / px;
                let earlier = t.font_size / scale;
                let height = t.height / scale;
                assert!(
                    (height - height.round()).abs() < 1e-6,
                    "{}: height {} at font size {earlier} is not whole pixels",
                    t.id,
                    height
                );
                let at_earlier =
                    dom_offset_width(&lines, t.family, earlier) + if plus_one { 1.0 } else { 0.0 };
                Some(at_earlier * scale)
            }
            Cause::InkBox(ink) => Some(
                lines
                    .iter()
                    .map(|l| ink_box_width(l, t.family, t.font_size, ink))
                    .fold(0.0, f64::max),
            ),
            Cause::ContainerWidth => {
                let (x, width) = t.container?;
                assert!(
                    (t.x - (x + BOUND_TEXT_PADDING)).abs() < 1e-9,
                    "{}: x is not the container's x + padding",
                    t.id
                );
                Some(width - BOUND_TEXT_PADDING * 2.0)
            }
        }
    }
}

/// Chrome's `offsetWidth` of a shrink-to-fit box whose content is `width`
/// px wide at x = 0: the preferred width ceiled to a `LayoutUnit` (1/64
/// px), snapped to a whole pixel by rounding.
fn offset_width(width: f64) -> f64 {
    ((width * 64.0 - 1e-9).ceil() / 64.0).round()
}

/// [`Cause::DomOffsetWidth`] for `lines` of `family` at `font_size`.
fn dom_offset_width(lines: &[&str], family: u32, font_size: f64) -> f64 {
    let font = get_font_string(font_size, FontFamily(family));
    let store = common::store();
    let last = lines.len() - 1;
    let widest = lines
        .iter()
        .enumerate()
        .map(|(i, line)| store.line_width(line, &font) + if i == last { 1.0 } else { 0.0 })
        .fold(0.0, f64::max);
    offset_width(widest)
}

/// The sfnt bytes of every vendored face of `family`, in the font assets
/// manifest's order.
fn family_faces(family: u32) -> &'static [Vec<u8>] {
    static FACES: OnceLock<BTreeMap<u32, Vec<Vec<u8>>>> = OnceLock::new();
    let faces = FACES.get_or_init(|| {
        let manifest: Value = serde_json::from_slice(&common::font_file("manifest.json"))
            .expect("font manifest parses");
        let mut out = BTreeMap::new();
        for f in manifest["families"].as_array().expect("families") {
            let id = u32::try_from(f["id"].as_u64().expect("id")).expect("id fits u32");
            let files = f["faces"]
                .as_array()
                .expect("faces")
                .iter()
                .map(|face| {
                    let file = face["file"].as_str().expect("file");
                    decode_font_file(&common::font_file(file)).expect(file)
                })
                .collect();
            out.insert(id, files);
        }
        out
    });
    faces.get(&family).map_or(&[], Vec::as_slice)
}

/// How a browser reports a line's ink box to `getLineWidth`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ink {
    /// The union of the glyph outlines' bounds as they are.
    Exact,
    /// The advance box grown by the ink's overhang on each side, each
    /// overhang rounded out to a whole pixel.
    WholePixels,
    /// The union of each glyph's bounds rounded out to whole pixels about
    /// the glyph's own origin, at the glyph's fractional pen position:
    /// Blink's glyph bounds when the font is not subpixel-positioned
    /// (`SkRect::roundOut` per glyph in `SkFontGetBoundsForGlyphs`).
    GlyphPixels,
}

/// A line laid out as `rustybuzz` shapes it in one face of its family.
struct InkLayout {
    /// The advance width, `measureText(line).width`.
    advance: f64,
    /// The ink extent `(left, right)` of the outlines, from the line's
    /// origin; `None` when no glyph has an outline.
    exact: Option<(f64, f64)>,
    /// The same with each glyph's bounds rounded out to whole pixels.
    glyph_pixels: Option<(f64, f64)>,
}

/// `line` in the first face of `family` (font assets manifest order) that
/// maps every character, at `font_size`.
fn ink_layout(line: &str, family: u32, font_size: f64) -> InkLayout {
    let data = family_faces(family)
        .iter()
        .find(|data| {
            let face = rustybuzz::ttf_parser::Face::parse(data, 0).expect("face parses");
            line.chars().all(|c| face.glyph_index(c).is_some())
        })
        .unwrap_or_else(|| panic!("no face of fontFamily {family} maps {line:?}"));
    let face = rustybuzz::Face::from_slice(data, 0).expect("face parses");
    let scale = font_size / f64::from(face.units_per_em());
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(line);
    let shaped = rustybuzz::shape(&face, &[], buffer);
    let unite = |ink: Option<(f64, f64)>, left: f64, right: f64| {
        Some(ink.map_or((left, right), |(l, r): (f64, f64)| {
            (l.min(left), r.max(right))
        }))
    };
    let mut layout = InkLayout {
        advance: 0.0,
        exact: None,
        glyph_pixels: None,
    };
    for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        let glyph = rustybuzz::ttf_parser::GlyphId(u16::try_from(info.glyph_id).expect("glyph"));
        if let Some(rect) = face.glyph_bounding_box(glyph) {
            let origin = layout.advance + f64::from(pos.x_offset) * scale;
            let (x_min, x_max) = (f64::from(rect.x_min) * scale, f64::from(rect.x_max) * scale);
            layout.exact = unite(layout.exact, origin + x_min, origin + x_max);
            layout.glyph_pixels = unite(
                layout.glyph_pixels,
                origin + x_min.floor(),
                origin + x_max.ceil(),
            );
        }
        layout.advance += f64::from(pos.x_advance) * scale;
    }
    layout
}

/// `getLineWidth` of `62228e0b` to `e3060dfb^` for one line:
/// `max(|actualBoundingBoxLeft| + |actualBoundingBoxRight|, width)`, with
/// the ink box as `ink` reports it.
fn ink_box_width(line: &str, family: u32, font_size: f64, ink: Ink) -> f64 {
    let layout = ink_layout(line, family, font_size);
    let advance = layout.advance;
    let bounds = match ink {
        Ink::Exact | Ink::WholePixels => layout.exact,
        Ink::GlyphPixels => layout.glyph_pixels,
    };
    let Some((left, right)) = bounds else {
        return advance;
    };
    match ink {
        Ink::WholePixels => {
            let over_left = (-left).max(0.0);
            let over_right = (right - advance).max(0.0);
            advance + (over_left - 1e-9).ceil().max(0.0) + (over_right - 1e-9).ceil().max(0.0)
        }
        Ink::Exact | Ink::GlyphPixels => advance.max(left.abs() + right.abs()),
    }
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Measured,
    HeightMismatch,
    Legacy,
    FixedWidth,
}

impl Class {
    fn key(self) -> &'static str {
        match self {
            Class::Measured => "measured",
            Class::HeightMismatch => "height_mismatch",
            Class::Legacy => "legacy",
            Class::FixedWidth => "fixed_width",
        }
    }
}

/// Which class a text of a vendored family is in, from everything but its
/// width. `measured_height` is `measureText`'s height for the text.
fn classify(element: &Value, measured_height: f64) -> Class {
    let line_height = element
        .get("lineHeight")
        .and_then(Value::as_f64)
        .filter(|lh| *lh > 0.0);
    if element.get("autoResize") == Some(&Value::Bool(false)) {
        return Class::FixedWidth;
    }
    if line_height.is_none() {
        return Class::Legacy;
    }
    let stored_height = element.get("height").and_then(Value::as_f64);
    match stored_height {
        Some(h) if (h - measured_height).abs() <= TOLERANCE => Class::Measured,
        _ => Class::HeightMismatch,
    }
}

/// One text element of the corpus in a vendored family.
#[derive(Debug)]
struct Text {
    /// The file under `fixtures/`.
    file: String,
    /// `upstream` or `libraries`.
    source: &'static str,
    item: usize,
    id: String,
    family: u32,
    font_size: f64,
    text: String,
    class: Class,
    stored: f64,
    measured: f64,
    /// The stored `x` and `height`.
    x: f64,
    height: f64,
    /// The `x` and `width` of the container a bound text names in
    /// `containerId`, when the same item holds it.
    container: Option<(f64, f64)>,
}

impl Text {
    fn deviation(&self) -> f64 {
        (self.measured - self.stored).abs()
    }
}

/// Corpus-wide counts of text elements the port does not measure.
#[derive(Debug, Default)]
struct Skipped {
    /// fontFamily -> texts, for families with no vendored faces.
    not_vendored: BTreeMap<String, usize>,
    /// Texts without a numeric `fontFamily`, `fontSize`, `width` or `text`.
    malformed: usize,
}

struct Corpus {
    texts: Vec<Text>,
    skipped: Skipped,
    library_files: usize,
    upstream_files: Vec<String>,
}

/// The elements of a library (`libraryItems` of v2 or `library` of v1) or
/// scene, with the index of the item they belong to (0 for a scene).
fn elements(doc: &Value) -> Vec<(usize, &Value)> {
    let mut out = Vec::new();
    if let Some(els) = doc.get("elements").and_then(Value::as_array) {
        out.extend(els.iter().map(|e| (0, e)));
    }
    let items = doc
        .get("libraryItems")
        .or_else(|| doc.get("library"))
        .and_then(Value::as_array);
    for (index, item) in items.into_iter().flatten().enumerate() {
        let els = match item {
            Value::Array(els) => Some(els),
            Value::Object(o) => o.get("elements").and_then(Value::as_array),
            _ => None,
        };
        out.extend(els.into_iter().flatten().map(|e| (index, e)));
    }
    out
}

/// The scene text an upstream fixture carries, `None` for files that carry
/// none: a library as is, the payload embedded in a PNG or SVG export.
fn upstream_scene(name: &str, bytes: &[u8]) -> Option<String> {
    if name.ends_with(".excalidrawlib") {
        Some(String::from_utf8(bytes.to_vec()).expect("library is UTF-8"))
    } else if name.ends_with(".png") {
        decode_png_metadata(bytes).ok().flatten()
    } else if name.ends_with(".svg") {
        decode_svg_base64_payload(std::str::from_utf8(bytes).ok()?).ok()
    } else {
        None
    }
}

fn corpus() -> &'static Corpus {
    static CORPUS: OnceLock<Corpus> = OnceLock::new();
    CORPUS.get_or_init(build_corpus)
}

fn build_corpus() -> Corpus {
    let store = common::store();
    let vendored: Vec<String> = store.families().map(str::to_owned).collect();
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("fixtures/manifest.json")).expect("manifest"),
    )
    .expect("manifest parses");
    let mut corpus = Corpus {
        texts: Vec::new(),
        skipped: Skipped::default(),
        library_files: 0,
        upstream_files: Vec::new(),
    };
    for entry in manifest["files"].as_array().expect("files") {
        let path = entry["path"].as_str().expect("path");
        let bytes = std::fs::read(repo().join("fixtures").join(path)).expect(path);
        assert_eq!(sha256(&bytes), entry["sha256"], "{path}");
        let (source, scene) = match entry["source"].as_str() {
            Some("libraries") if path.ends_with(".excalidrawlib.gz") => {
                let mut text = Vec::new();
                flate2::read::GzDecoder::new(&bytes[..])
                    .read_to_end(&mut text)
                    .expect("gzip");
                assert_eq!(sha256(&text), entry["content_sha256"], "{path}");
                corpus.library_files += 1;
                ("libraries", String::from_utf8(text).expect("UTF-8"))
            }
            Some("upstream") => match upstream_scene(path, &bytes) {
                Some(scene) => {
                    corpus.upstream_files.push(path.to_owned());
                    ("upstream", scene)
                }
                None => continue,
            },
            _ => continue,
        };
        let doc: Value = serde_json::from_str(&scene).expect(path);
        let all = elements(&doc);
        for &(item, element) in &all {
            if element.get("type").and_then(Value::as_str) != Some("text") {
                continue;
            }
            let fields = (
                element.get("fontFamily").and_then(Value::as_u64),
                element.get("fontSize").and_then(Value::as_f64),
                element.get("width").and_then(Value::as_f64),
                element.get("text").and_then(Value::as_str),
            );
            let (Some(family), Some(font_size), Some(stored), Some(text)) = fields else {
                corpus.skipped.malformed += 1;
                continue;
            };
            let family = u32::try_from(family).expect("fontFamily fits u32");
            let name = FontFamily(family).name();
            if !name.is_some_and(|n| vendored.iter().any(|v| v == n)) {
                *corpus
                    .skipped
                    .not_vendored
                    .entry(family.to_string())
                    .or_default() += 1;
                continue;
            }
            let font = get_font_string(font_size, FontFamily(family));
            let line_height = element
                .get("lineHeight")
                .and_then(Value::as_f64)
                .filter(|lh| *lh > 0.0)
                .unwrap_or(1.25);
            let dims = measure_text(text, &font, line_height, store);
            let container = element
                .get("containerId")
                .and_then(Value::as_str)
                .and_then(|cid| {
                    all.iter().find(|(i, e)| {
                        *i == item && e.get("id").and_then(Value::as_str) == Some(cid)
                    })
                })
                .and_then(|(_, c)| Some((c.get("x")?.as_f64()?, c.get("width")?.as_f64()?)));
            corpus.texts.push(Text {
                file: path.to_owned(),
                source,
                item,
                id: element["id"].as_str().unwrap_or_default().to_owned(),
                family,
                font_size,
                text: text.to_owned(),
                class: classify(element, dims.height),
                stored,
                measured: dims.width,
                x: element.get("x").and_then(Value::as_f64).unwrap_or_default(),
                height: element
                    .get("height")
                    .and_then(Value::as_f64)
                    .unwrap_or_default(),
                container,
            });
        }
    }
    corpus
}

fn round(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

#[derive(Default)]
struct Stats {
    texts: usize,
    within: usize,
    max: f64,
    sum: f64,
}

impl Stats {
    fn add(&mut self, deviation: f64) {
        self.texts += 1;
        if deviation <= TOLERANCE {
            self.within += 1;
        }
        self.max = self.max.max(deviation);
        self.sum += deviation;
    }

    fn mean(&self) -> f64 {
        if self.texts == 0 {
            0.0
        } else {
            self.sum / self.texts as f64
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "texts": self.texts,
            "within_tolerance": self.within,
            "max_deviation_px": round(self.max),
            "mean_deviation_px": round(self.mean()),
        })
    }
}

/// `(fontFamily, class) -> stats` over `texts`.
fn stats<'a>(texts: impl Iterator<Item = &'a Text>) -> BTreeMap<(u32, Class), Stats> {
    let mut out: BTreeMap<(u32, Class), Stats> = BTreeMap::new();
    for t in texts {
        out.entry((t.family, t.class))
            .or_default()
            .add(t.deviation());
    }
    out
}

fn family_table(stats: &BTreeMap<(u32, Class), Stats>) -> Value {
    let mut families: BTreeMap<u32, Map<String, Value>> = BTreeMap::new();
    for ((family, class), s) in stats {
        let entry = families.entry(*family).or_insert_with(|| {
            let mut m = Map::new();
            m.insert("fontFamily".into(), json!(family));
            m.insert("name".into(), json!(FontFamily(*family).name()));
            m.insert("gated".into(), json!(GATED_FAMILIES.contains(family)));
            m
        });
        entry.insert(class.key().into(), s.to_json());
    }
    Value::Array(families.into_values().map(Value::Object).collect())
}

/// JSON text with every non-ASCII character escaped, so the report holds
/// no emoji variation selectors (the authorship gate rejects them).
fn ascii_json(value: &Value) -> String {
    let text = serde_json::to_string_pretty(value).expect("report") + "\n";
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii() {
            out.push(ch);
        } else {
            let mut units = [0u16; 2];
            for unit in ch.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

fn report() -> Value {
    let corpus = corpus();
    let all = stats(corpus.texts.iter());
    let upstream = stats(corpus.texts.iter().filter(|t| t.source == "upstream"));
    let upstream_texts: Vec<Value> = corpus
        .texts
        .iter()
        .filter(|t| t.source == "upstream")
        .map(|t| {
            json!({
                "file": t.file,
                "id": t.id,
                "fontFamily": t.family,
                "fontSize": t.font_size,
                "text": t.text,
                "class": t.class.key(),
                "stored_width": t.stored,
                "measured_width": round(t.measured),
            })
        })
        .collect();
    let known: Vec<Value> = KNOWN_DEVIATIONS
        .iter()
        .map(|(file, id, stored, measured, cause)| {
            let t = find(file, id);
            json!({
                "file": file,
                "item": t.item,
                "id": id,
                "fontFamily": t.family,
                "fontSize": t.font_size,
                "text": t.text,
                "stored_width": stored,
                "measured_width": measured,
                "deviation_px": round(t.measured - t.stored),
                "cause": cause.key(),
            })
        })
        .collect();
    json!({
        "generator": "crates/excali-text/tests/text_width_corpus.rs",
        "description": "Stored against measured widths of every text element in a vendored \
    family across the ex-003 corpus (fixtures/manifest.json): the catalogue libraries and the \
    scene-bearing upstream test fixtures. deviation = |measured - stored| in px, measured with \
    excali_text's FontStore as measureText(text, getFontString(element), lineHeight). Classes \
    are decided without the width; see classes. Regenerate with EXCALI_BLESS=1 cargo test -p \
    excali-text --test text_width_corpus.",
        "tolerance_px": TOLERANCE,
        "gated_families": GATED_FAMILIES,
        "classes": {
            "measured": "lineHeight set, autoResize not false, stored height = fontSize * \
    lineHeight * lines (getTextHeight): a width measureText wrote; gated for the gated families",
            "height_mismatch": "lineHeight set and autoResize not false, but a stored height \
    measureText cannot give: not written by upstream's measurement; reported",
            "legacy": "no lineHeight: written before measureText used a unitless line \
    height; reported",
            "fixed_width": "autoResize false: width is the wrap width the user set, not a \
    measurement; counted",
        },
        "sources": {
            "library_files": corpus.library_files,
            "upstream_files": corpus.upstream_files,
        },
        "families": family_table(&all),
        "upstream_fixtures": {
            "families": family_table(&upstream),
            "texts": upstream_texts,
        },
        "skipped": {
            "not_vendored_by_fontFamily": corpus.skipped.not_vendored,
            "malformed": corpus.skipped.malformed,
        },
        "known_deviation_causes": {
            "dom_offset_width": "measureText up to 3a141ca7^ (2023-01-30): offsetWidth of a \
    white-space: pre div holding the text and a 1 px inline-block span after its last line; the \
    widest line, ceiled to 1/64 px and rounded",
            "dom_offset_width_plus_one": "measureText of 3a141ca7 to 9659254f^ (2023-02-23): \
    the same offsetWidth + 1",
            "scaled_dom_offset_width": "a dom_offset_width (or plus one) at an earlier font \
    size, then resized: width and fontSize scaled together, the height scaling back to whole \
    pixels",
            "container_width": "bound text of 5c67329b (2022-01-03) to 4cb6f095^ (2022-09-22): \
    width = container width - 2 * BOUND_TEXT_PADDING",
            "ink_box": "getLineWidth of 62228e0b (2024-07-25) to e3060dfb^ (2025-02-11): \
    max(|actualBoundingBoxLeft| + |actualBoundingBoxRight|, width) from the glyph outlines",
            "ink_box_whole_pixels": "the same, with the ink box the advance box grown by the \
    ink overhang on each side rounded out to whole pixels",
            "ink_box_glyph_pixels": "the same, with each glyph's bounds rounded out to whole \
    pixels about its origin",
            "kept_width": "no measurement found; a width kept from an earlier state of the \
    element",
        },
        "known_deviations": known,
    })
}

fn find(file: &str, id: &str) -> &'static Text {
    let matches: Vec<&Text> = corpus()
        .texts
        .iter()
        .filter(|t| t.file == file && t.id == id)
        .collect();
    assert_eq!(matches.len(), 1, "{file} {id}: expected exactly one text");
    matches[0]
}

#[test]
fn report_matches_the_committed_report() {
    let report = report();
    // The per-family table, for CI logs (`--nocapture`).
    for row in report["families"].as_array().expect("families") {
        println!("{row}");
    }
    let text = ascii_json(&report);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(REPORT);
    if std::env::var_os("EXCALI_BLESS").is_some() {
        std::fs::write(&path, &text).expect("write report");
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{REPORT}: {e} (run with EXCALI_BLESS=1 to write it)"));
    assert!(
        committed == text,
        "{REPORT} is out of date: run with EXCALI_BLESS=1 and review the diff"
    );
}

#[test]
fn corpus_is_every_library_and_every_scene_bearing_upstream_fixture() {
    let corpus = corpus();
    assert_eq!(corpus.library_files, 232);
    assert_eq!(
        corpus
            .upstream_files
            .iter()
            .map(|p| p.rsplit('/').next().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "fixture_library.excalidrawlib",
            "smiley_embedded_v2.png",
            "smiley_embedded_v2.svg",
            "test_embedded_v1.png",
            "test_embedded_v1.svg",
        ]
    );
    let mut per_class: BTreeMap<&str, usize> = BTreeMap::new();
    for t in &corpus.texts {
        *per_class.entry(t.class.key()).or_default() += 1;
    }
    // ex-302's baseline: 1866 texts with lineHeight and autoResize in the
    // vendored families, of which 328 have a height measureText cannot give.
    assert_eq!(
        per_class,
        BTreeMap::from([
            ("measured", 1538),
            ("height_mismatch", 328),
            ("legacy", 4403),
            ("fixed_width", 585),
        ])
    );
}

#[test]
fn gated_families_measure_within_half_a_pixel() {
    let corpus = corpus();
    let mut failures = Vec::new();
    for family in GATED_FAMILIES {
        let measured: Vec<&Text> = corpus
            .texts
            .iter()
            .filter(|t| t.family == family && t.class == Class::Measured)
            .collect();
        assert!(
            !measured.is_empty(),
            "fontFamily {family}: no measured texts"
        );
        let mean = measured.iter().map(|t| t.deviation()).sum::<f64>() / measured.len() as f64;
        if mean > TOLERANCE {
            failures.push(format!(
                "fontFamily {family}: mean deviation {mean} px over {} texts",
                measured.len()
            ));
        }
        for t in measured {
            let known = KNOWN_DEVIATIONS
                .iter()
                .any(|(file, id, _, _, _)| *file == t.file && *id == t.id);
            if t.deviation() > TOLERANCE && !known {
                failures.push(format!(
                    "{} item {} id {}: {:?} at {} px in fontFamily {family}: measured {} stored {}",
                    t.file, t.item, t.id, t.text, t.font_size, t.measured, t.stored
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} deviations beyond {TOLERANCE} px:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn known_deviations_still_deviate_as_recorded() {
    for (file, id, stored, measured, _) in KNOWN_DEVIATIONS {
        let t = find(file, id);
        assert!(
            GATED_FAMILIES.contains(&t.family),
            "{id}: not a gated family"
        );
        assert_eq!(t.class, Class::Measured, "{id}: not a measured text");
        assert_eq!(t.stored, *stored, "{id}: stored width");
        assert!(
            (t.measured - measured).abs() < 0.001,
            "{id}: measures {} now, recorded {measured}",
            t.measured
        );
        assert!(
            t.deviation() > TOLERANCE,
            "{id}: now within {TOLERANCE} px; remove it from KNOWN_DEVIATIONS"
        );
        // Every one is a stored width wider than the text measures.
        assert!(t.stored > t.measured, "{id}");
    }
    let mut ids: Vec<&str> = KNOWN_DEVIATIONS.iter().map(|k| k.1).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), KNOWN_DEVIATIONS.len(), "an id is listed twice");
}

#[test]
fn only_the_generated_library_has_heights_measure_text_cannot_give() {
    let mut found: BTreeMap<(&str, u32), usize> = BTreeMap::new();
    for t in corpus()
        .texts
        .iter()
        .filter(|t| t.class == Class::HeightMismatch)
    {
        *found.entry((t.file.as_str(), t.family)).or_default() += 1;
    }
    let expected: BTreeMap<(&str, u32), usize> = HEIGHT_MISMATCH
        .iter()
        .map(|(file, family, n)| ((*file, *family), *n))
        .collect();
    assert_eq!(found, expected);
}

#[test]
fn classes_follow_the_stored_fields_not_the_width() {
    let base = json!({
        "type": "text", "fontSize": 20, "lineHeight": 1.25, "autoResize": true,
        "height": 50, "width": 1000,
    });
    // Two lines at 20 px and 1.25: measureText's height is 50.
    assert_eq!(classify(&base, 50.0), Class::Measured);
    assert_eq!(classify(&base, 50.4), Class::Measured);
    assert_eq!(classify(&base, 25.0), Class::HeightMismatch);
    let mut no_resize = base.clone();
    no_resize["autoResize"] = json!(false);
    assert_eq!(classify(&no_resize, 50.0), Class::FixedWidth);
    let mut missing = base.clone();
    missing.as_object_mut().unwrap().remove("autoResize");
    assert_eq!(classify(&missing, 50.0), Class::Measured);
    let mut legacy = base.clone();
    legacy.as_object_mut().unwrap().remove("lineHeight");
    assert_eq!(classify(&legacy, 50.0), Class::Legacy);
    let mut legacy_fixed = legacy.clone();
    legacy_fixed["autoResize"] = json!(false);
    assert_eq!(classify(&legacy_fixed, 50.0), Class::FixedWidth);
    let mut no_height = base;
    no_height.as_object_mut().unwrap().remove("height");
    assert_eq!(classify(&no_height, 50.0), Class::HeightMismatch);
}

#[test]
fn known_deviations_are_what_their_cause_writes() {
    let mut failures = Vec::new();
    for (file, id, stored, _, cause) in KNOWN_DEVIATIONS {
        let t = find(file, id);
        let Some(width) = cause.width(t) else {
            assert_eq!(*cause, Cause::KeptWidth, "{id}: no container for {cause:?}");
            continue;
        };
        if (width - stored).abs() > 0.001 {
            failures.push(format!(
                "{file} {id} {:?}: {cause:?} writes {width}, stored {stored}",
                t.text
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_virgil_deviation_has_a_measured_cause() {
    for (file, id, _, _, cause) in KNOWN_DEVIATIONS {
        if find(file, id).family == 1 {
            assert_ne!(
                *cause,
                Cause::KeptWidth,
                "{id}: Virgil text without a cause"
            );
        }
    }
}

#[test]
fn offset_width_ceils_to_a_layout_unit_then_rounds() {
    // 77.008 + 1 (the span) is 78.008: one layout unit up is 78.015625.
    assert_eq!(offset_width(78.008), 78.0);
    assert_eq!(offset_width(95.544), 96.0);
    // On a layout unit it stays; 60.49 ceils to 60.5 and rounds up.
    assert_eq!(offset_width(60.25), 60.0);
    assert_eq!(offset_width(60.49), 61.0);
    assert_eq!(offset_width(38.0), 38.0);
}

#[test]
fn ink_layout_advances_are_the_measured_width() {
    let store = common::store();
    let texts = [
        (1, "Data lake"),
        (1, "Kafka Streams Topology Design"),
        (5, "OCI Region"),
        (8, "7"),
    ];
    for (family, text) in texts {
        for size in [16.0, 20.0, 36.0] {
            let font = get_font_string(size, FontFamily(family));
            let advance = ink_layout(text, family, size).advance;
            assert!(
                (advance - store.line_width(text, &font)).abs() < 1e-9,
                "{family} {text} {size}"
            );
        }
    }
}

#[test]
fn ink_box_is_the_advance_without_ink_and_never_narrower() {
    let blank = ink_layout("  ", 1, 20.0);
    assert_eq!(blank.exact, None);
    assert_eq!(blank.glyph_pixels, None);
    for ink in [Ink::Exact, Ink::WholePixels, Ink::GlyphPixels] {
        assert_eq!(ink_box_width("  ", 1, 20.0, ink), blank.advance);
    }
    for text in ["Table", "Glacier", "mapValues", "I", "P"] {
        let layout = ink_layout(text, 1, 20.0);
        let (left, right) = layout.exact.expect("ink");
        let (pixel_left, pixel_right) = layout.glyph_pixels.expect("ink");
        // Rounding each glyph out only grows the box.
        assert!(pixel_left <= left && pixel_right >= right, "{text}");
        let advance = layout.advance;
        let exact = ink_box_width(text, 1, 20.0, Ink::Exact);
        assert_eq!(exact, advance.max(left.abs() + right.abs()), "{text}");
        let glyphs = ink_box_width(text, 1, 20.0, Ink::GlyphPixels);
        assert_eq!(
            glyphs,
            advance.max(pixel_left.abs() + pixel_right.abs()),
            "{text}"
        );
        // Whole pixels over the advance, however far the ink overhangs.
        let whole = ink_box_width(text, 1, 20.0, Ink::WholePixels);
        assert!(whole >= advance, "{text}");
        assert!((whole - advance).fract().abs() < 1e-9, "{text}");
    }
    // "Table" at 20 px: the T's ink starts 1.04 px left of the origin and
    // the e's ends 0.42 px past the advance, so whole pixels add 2 + 1.
    let table = ink_layout("Table", 1, 20.0);
    assert_eq!(
        ink_box_width("Table", 1, 20.0, Ink::WholePixels) - table.advance,
        3.0
    );
}
