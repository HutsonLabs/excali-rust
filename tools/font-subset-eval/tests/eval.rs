//! ex-408: the font subsetting evaluation and the ADR-010 decision it
//! backs.
//!
//! The candidates are called as upstream calls HarfBuzz and the fidelity
//! measure is pinned on known fonts; the committed report.json must be what
//! a fresh run over upstream-subsets.json gives; browser.json and wasm.json
//! must hold what the ADR's rule needs; and ADR-010 must quote the tables
//! and the decision the rule gives.

use std::path::{Path, PathBuf};

use font_subset_eval::candidates::{decode, encode, glyph_ids, subset, Encoder, Subsetter};
use font_subset_eval::decision::{
    builds_for_wasm, candidate_table, decide, eligible, faithful_in_browser, load_browser,
    load_wasm, scene_table, BrowserTotals, WasmBuild,
};
use font_subset_eval::fidelity::{compare, runs, Fidelity};
use font_subset_eval::report::{
    data_url_len, evaluate, load_manifest, load_scenes, percent, report_text, text_families,
    Candidate, Totals,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

const EXCALIFONT_LATIN: &str =
    "Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2";
const NUNITO_LATIN: &str =
    "Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTQ3j6zbXWjgeg.woff2";

fn font(file: &str) -> Vec<u8> {
    let bytes =
        std::fs::read(root().join("crates/excali-text/assets/fonts").join(file)).expect("font");
    decode(&bytes).expect("decodes")
}

fn cps(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}

// ---------------------------------------------------------------------------
// Candidates

#[test]
fn decode_reads_woff2_and_passes_sfnt_through() {
    let sfnt = font(EXCALIFONT_LATIN);
    assert_eq!(&sfnt[..4], &[0, 1, 0, 0]);
    assert_eq!(decode(&sfnt).unwrap(), sfnt);
    let ttf = std::fs::read(
        root().join("crates/excali-text/assets/fonts/Liberation/LiberationSans-Regular.ttf"),
    )
    .unwrap();
    assert_eq!(decode(&ttf).unwrap(), ttf);
}

#[test]
fn glyph_ids_start_with_notdef_sorted_and_unique() {
    let sfnt = font(EXCALIFONT_LATIN);
    let ids = glyph_ids(&sfnt, &cps("baab\u{10FFFF}")).unwrap();
    assert_eq!(ids[0], 0);
    assert_eq!(ids.len(), 3, "{ids:?}");
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn hb_subset_and_skera_keep_exactly_what_the_original_draws() {
    let sfnt = font(NUNITO_LATIN);
    let text = "Web app, API Gateway: Wave AV To";
    for s in [Subsetter::HbSubset, Subsetter::Skera] {
        let out = subset(s, &sfnt, &cps(text)).unwrap();
        let f = compare(&sfnt, &out, &cps(text), &[text]).unwrap();
        assert!(f.full(), "{}: {f:?}", s.name());
        assert_eq!(f.code_points, 19, "{}", s.name());
        // Smaller than the face, larger than nothing.
        assert!(
            out.len() < sfnt.len() / 4,
            "{}: {} of {}",
            s.name(),
            out.len(),
            sfnt.len()
        );
        let face = ttf_parser::Face::parse(&out, 0).unwrap();
        assert!(
            face.glyph_index('Z').is_none(),
            "{}: kept a glyph not asked for",
            s.name()
        );
    }
}

#[test]
fn allsorts_drops_the_layout_tables_so_kerned_text_moves() {
    // allsorts 0.17.0 writes no GSUB, GPOS or GDEF (subset_ttf builds cmap,
    // cvt, fpgm, hhea, hmtx, maxp, name, post, prep, OS/2, head, glyf and
    // loca only): every glyph is kept, the kerning is not.
    let sfnt = font(NUNITO_LATIN);
    let text = "AVATAR To Wa";
    let out = subset(Subsetter::Allsorts, &sfnt, &cps(text)).unwrap();
    let face = ttf_parser::Face::parse(&out, 0).unwrap();
    for tag in [b"GSUB", b"GPOS", b"GDEF"] {
        assert!(face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(tag))
            .is_none());
    }
    let f = compare(&sfnt, &out, &cps(text), &[text]).unwrap();
    assert_eq!(f.kept, f.code_points);
    assert_eq!(f.glyphs_equal, f.code_points);
    assert_eq!((f.runs, f.runs_equal), (1, 0), "{f:?}");
}

#[test]
fn typst_subsetter_removes_the_cmap() {
    // "we remove the cmap table from the font, so you must provide your own
    // cmap table in the PDF" (subsetter 0.2.6 src/lib.rs).
    let sfnt = font(EXCALIFONT_LATIN);
    let out = subset(Subsetter::Subsetter, &sfnt, &cps("abc")).unwrap();
    let face = ttf_parser::Face::parse(&out, 0).unwrap();
    assert!(face.tables().cmap.is_none());
    let f = compare(&sfnt, &out, &cps("abc"), &["abc"]).unwrap();
    assert_eq!((f.code_points, f.kept), (3, 0));
}

#[test]
fn both_encoders_write_woff2_that_decodes_to_the_same_font() {
    let sfnt = font(EXCALIFONT_LATIN);
    let sub = subset(Subsetter::Skera, &sfnt, &cps("Hello world")).unwrap();
    for e in Encoder::ALL {
        let woff2 = encode(e, &sub).unwrap();
        assert_eq!(&woff2[..4], b"wOF2", "{}", e.name());
        assert!(woff2.len() < sub.len(), "{}", e.name());
        let back = decode(&woff2).unwrap();
        let f = compare(&sub, &back, &cps("Hello world"), &["Hello world"]).unwrap();
        assert!(f.full(), "{}: {f:?}", e.name());
    }
}

// ---------------------------------------------------------------------------
// Fidelity

#[test]
fn a_font_is_faithful_to_itself() {
    let sfnt = font(EXCALIFONT_LATIN);
    let f = compare(
        &sfnt,
        &sfnt,
        &cps("Hello, world!"),
        &["Hello, world!", "old lore"],
    )
    .unwrap();
    assert_eq!(
        f,
        Fidelity {
            code_points: 10,
            kept: 10,
            glyphs_equal: 10,
            metrics_equal: true,
            runs: 2,
            runs_equal: 2,
        }
    );
    assert!(f.full());
}

#[test]
fn another_face_is_not() {
    let a = font(EXCALIFONT_LATIN);
    let b = font(NUNITO_LATIN);
    let f = compare(&a, &b, &cps("abc"), &["abc"]).unwrap();
    assert_eq!((f.code_points, f.kept, f.glyphs_equal), (3, 3, 0));
    assert!(!f.metrics_equal);
    assert_eq!(f.runs_equal, 0);
    assert!(compare(&a, b"not a font", &cps("abc"), &[]).is_none());
}

#[test]
fn runs_are_the_mapped_characters_asked_for() {
    let sfnt = font(EXCALIFONT_LATIN);
    let face = ttf_parser::Face::parse(&sfnt, 0).unwrap();
    // 你 is not in the Latin face; z was not asked for; \n is not mapped.
    assert_eq!(
        runs(&face, &cps("ab cd"), "ab 你cd\nz a"),
        ["ab ", "cd", " a"]
    );
}

#[test]
fn a_soft_hyphen_without_a_space_glyph_draws_nothing_either_way() {
    // The one run of the scenes where a subset's shaping differed from its
    // original before invisible glyphs were set aside: U+00AD in a face
    // subset without U+0020 (latin1-excalifont), upstream's subset included.
    let sfnt = font(EXCALIFONT_LATIN);
    let text = "\u{ad}®¯°±";
    for s in [Subsetter::HbSubset, Subsetter::Skera] {
        let out = subset(s, &sfnt, &cps(text)).unwrap();
        assert!(ttf_parser::Face::parse(&out, 0)
            .unwrap()
            .glyph_index(' ')
            .is_none());
        let f = compare(&sfnt, &out, &cps(text), &[text]).unwrap();
        assert_eq!((f.runs, f.runs_equal), (1, 1), "{}", s.name());
    }
}

// ---------------------------------------------------------------------------
// Report arithmetic

#[test]
fn data_url_length_is_prefix_plus_padded_base64() {
    assert_eq!(
        data_url_len("font/woff2", 0),
        "data:font/woff2;base64,".len()
    );
    assert_eq!(data_url_len("font/woff2", 1), 23 + 4);
    assert_eq!(data_url_len("font/woff2", 3), 23 + 4);
    assert_eq!(data_url_len("font/woff2", 4), 23 + 8);
    assert_eq!(data_url_len("font/ttf", 410_712), 21 + 547_616);
}

#[test]
fn percent_rounds_half_up_to_one_decimal() {
    assert_eq!(percent(1, 3), "33.3");
    assert_eq!(percent(2, 3), "66.7");
    assert_eq!(percent(1, 8), "12.5");
    assert_eq!(percent(1, 16), "6.3");
    assert_eq!(percent(0, 5), "0.0");
    assert_eq!(percent(20, 1), "2000.0");
    assert_eq!(percent(1, 0), "n/a");
}

#[test]
fn xiaolai_faces_draw_excalifonts_text() {
    // Fonts.ts:196-205: Xiaolai is inlined with Excalifont's characters.
    assert_eq!(text_families(100), [100, 5]);
    assert_eq!(text_families(6), [6]);
    let m = load_manifest(&root());
    assert_eq!(m.families["Xiaolai"], 100);
    assert_eq!(m.families["Excalifont"], 5);
    assert_eq!(
        m.files["Liberation/LiberationSans-Regular.woff2"],
        "Liberation/LiberationSans-Regular.ttf"
    );
}

#[test]
fn the_fixture_is_upstreams_and_covers_every_inlined_family() {
    let (commit, scenes) = load_scenes(&root());
    assert_eq!(commit.len(), 40);
    assert!(
        read("site/config.toml").contains(&commit),
        "not the pinned commit"
    );
    let families: std::collections::BTreeSet<&str> = scenes
        .iter()
        .flat_map(|s| s.declarations.iter().map(|d| d.family.as_str()))
        .collect();
    assert_eq!(
        families.into_iter().collect::<Vec<_>>(),
        [
            "Cascadia",
            "Comic Shanns",
            "Excalifont",
            "Liberation Sans",
            "Lilita One",
            "Nunito",
            "Virgil",
            "Xiaolai"
        ]
    );
    assert!(scenes.iter().any(|s| s.font_face == "vitest"));
}

// ---------------------------------------------------------------------------
// Decision rule

fn totals(bytes: usize, full: bool) -> Totals {
    Totals {
        declarations: 2,
        full: if full { 2 } else { 1 },
        bytes,
        ..Totals::default()
    }
}

fn wasm(builds: &[(&str, bool)]) -> Vec<WasmBuild> {
    builds
        .iter()
        .map(|(f, ok)| WasmBuild {
            features: f.to_string(),
            builds: *ok,
            bytes: ok.then_some(100),
            gzip: ok.then_some(50),
        })
        .collect()
}

fn browser(entries: &[(&str, usize, usize, usize)]) -> Vec<(String, BrowserTotals)> {
    entries
        .iter()
        .map(|(n, loaded, runs, equal)| {
            (
                n.to_string(),
                BrowserTotals {
                    declarations: 3,
                    failed: 0,
                    loaded: *loaded,
                    runs: *runs,
                    runs_equal: *equal,
                },
            )
        })
        .collect()
}

#[test]
fn the_rule_needs_all_three_conditions() {
    let skera = Candidate::Subset(Subsetter::Skera, Encoder::Ttf2woff2);
    let hb = Candidate::Subset(Subsetter::HbSubset, Encoder::Ttf2woff2);
    let w = wasm(&[
        ("skera", true),
        ("hb-subset", false),
        ("ttf2woff2", true),
        ("skera,ttf2woff2", true),
    ]);
    let b = browser(&[
        ("upstream", 2, 5, 5),
        ("skera+ttf2woff2", 2, 5, 5),
        ("hb-subset+ttf2woff2", 2, 5, 5),
    ]);
    assert!(builds_for_wasm(&w, skera));
    assert!(!builds_for_wasm(&w, hb));
    assert!(builds_for_wasm(&w, Candidate::Whole));
    assert!(faithful_in_browser(&b, skera));
    assert!(eligible(&totals(10, true), &w, &b, skera));
    assert!(
        !eligible(&totals(10, false), &w, &b, skera),
        "a face not faithful"
    );
    assert!(!eligible(&totals(10, true), &w, &b, hb), "no wasm32 build");
    let fewer_loaded = browser(&[("upstream", 2, 5, 5), ("skera+ttf2woff2", 1, 5, 5)]);
    assert!(
        !eligible(&totals(10, true), &w, &fewer_loaded, skera),
        "rejected by the browser"
    );
    let pixels = browser(&[("upstream", 2, 5, 5), ("skera+ttf2woff2", 2, 5, 4)]);
    assert!(
        !eligible(&totals(10, true), &w, &pixels, skera),
        "a run drawn differently"
    );
    assert!(
        !eligible(
            &totals(10, true),
            &w,
            &browser(&[("upstream", 2, 5, 5)]),
            skera
        ),
        "not run in the browser"
    );
    let pair_fails = wasm(&[
        ("skera", true),
        ("ttf2woff2", true),
        ("skera,ttf2woff2", false),
    ]);
    assert!(!builds_for_wasm(&pair_fails, skera));
}

// ---------------------------------------------------------------------------
// The committed files and ADR-010

fn adr() -> String {
    read("site/content/decisions/adr-010-svg-font-subsetting.md")
}

#[test]
fn report_json_is_a_fresh_run() {
    let report = evaluate(&root());
    assert_eq!(
        report_text(&report),
        read("tools/font-subset-eval/report.json"),
        "stale: run cargo run --release --manifest-path tools/font-subset-eval/Cargo.toml -- --write"
    );
}

#[test]
fn upstreams_own_subsets_are_faithful_by_the_same_measure() {
    let report = evaluate(&root());
    let up = &report.upstream_totals;
    assert!(up.faithful(), "{up:?}");
    assert_eq!(
        up.declarations,
        report
            .scenes
            .iter()
            .map(|s| s.declarations.len())
            .sum::<usize>()
    );
    // The whole files are 20 times upstream's bytes.
    assert!(report.totals("whole").bytes > 20 * up.bytes);
}

#[test]
fn browser_and_wasm_records_hold_what_the_rule_reads() {
    let b = load_browser(&root());
    let names: Vec<&str> = b.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "upstream",
            "hb-subset+ttf2woff2",
            "skera+ttf2woff2",
            "allsorts+ttf2woff2",
            "subsetter+ttf2woff2"
        ]
    );
    let up = &b[0].1;
    assert!(up.runs > 250 && up.runs_equal == up.runs, "{up:?}");
    let w = load_wasm(&root());
    let builds = |f: &str| w.iter().find(|x| x.features == f).unwrap().builds;
    // C++ has no standard library on wasm32-unknown-unknown.
    assert!(!builds("hb-subset"));
    assert!(!builds("woofwoof"));
    for f in [
        "none",
        "skera",
        "allsorts",
        "subsetter",
        "ttf2woff2",
        "skera,ttf2woff2",
        "allsorts,ttf2woff2",
        "subsetter,ttf2woff2",
    ] {
        assert!(builds(f), "{f}");
    }
}

#[test]
fn the_rule_decides_skera_with_ttf2woff2() {
    let report = evaluate(&root());
    let decided = decide(&report, &load_wasm(&root()), &load_browser(&root()));
    assert_eq!(
        decided,
        Candidate::Subset(Subsetter::Skera, Encoder::Ttf2woff2)
    );
    assert!(adr().contains(&format!("**Decision: `{}`**", decided.name())));
}

#[test]
fn adr_010_quotes_the_tables() {
    let root = root();
    let report = evaluate(&root);
    let (w, b) = (load_wasm(&root), load_browser(&root));
    let adr = adr();
    for line in candidate_table(&report, &w, &b) {
        assert!(adr.contains(&line), "ADR-010 does not quote:\n{line}");
    }
    for line in scene_table(&report, decide(&report, &w, &b)) {
        assert!(adr.contains(&line), "ADR-010 does not quote:\n{line}");
    }
}
