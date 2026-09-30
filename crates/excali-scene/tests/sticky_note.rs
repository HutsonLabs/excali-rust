//! A sticky note's date footer (`getStickyNoteDateLabel` and
//! `getStickyNoteFooter`, `packages/element/src/stickyNote.ts:425-500`) in
//! time zones other than the goldens' UTC, and its outline's PRNG
//! (`seededRandom`, `common/src/random.ts:24-33`).
//!
//! The expected labels are upstream's function run under Node 24 with
//! `TZ=America/New_York` and `TZ=UTC` (2026-09-29): `new Date(created)`'s
//! local date and year.

use excali_core::element::Element;
use excali_scene::sticky_note::{
    get_sticky_note_date_label, get_sticky_note_footer, seeded_random, Clock,
};
use serde_json::json;

/// America/New_York in 2026: UTC−4 from 8 March 07:00 UTC to 1 November
/// 06:00 UTC, UTC−5 otherwise.
fn new_york(time: f64) -> f64 {
    let dst_start = 1_772_953_200_000.0;
    let dst_end = 1_793_512_800_000.0;
    if (dst_start..dst_end).contains(&time) {
        -240.0
    } else {
        -300.0
    }
}

/// 2026-01-01 03:00 UTC: still 2025 in New York.
const NOW: f64 = 1_767_236_400_000.0;

fn labels(clock: &Clock, created: f64) -> (Option<String>, Option<String>) {
    (
        get_sticky_note_date_label(Some(created), false, clock),
        get_sticky_note_date_label(Some(created), true, clock),
    )
}

fn some(long: &str, short: &str) -> (Option<String>, Option<String>) {
    (Some(long.to_owned()), Some(short.to_owned()))
}

#[test]
fn dates_are_local() {
    let ny = Clock {
        now: NOW,
        utc_offset_minutes: new_york,
    };
    let utc = Clock::utc(NOW);
    // 2025-12-31 23:30 UTC
    assert_eq!(labels(&ny, 1_767_223_800_000.0), some("31 Dec", "31 Dec"));
    assert_eq!(
        labels(&utc, 1_767_223_800_000.0),
        some("31 Dec 2025", "31 Dec")
    );
    // 2026-07-04 02:00 UTC, in summer time
    assert_eq!(
        labels(&ny, 1_783_130_400_000.0),
        some("3 Jul 2026", "3 Jul")
    );
    assert_eq!(labels(&utc, 1_783_130_400_000.0), some("4 Jul", "4 Jul"));
    // a minute before the change to summer time
    assert_eq!(
        labels(&ny, 1_772_953_140_000.0),
        some("8 Mar 2026", "8 Mar")
    );
    // just before the epoch, and a fraction after it
    assert_eq!(labels(&ny, -1.0), some("31 Dec 1969", "31 Dec"));
    assert_eq!(labels(&ny, 1.5), some("31 Dec 1969", "31 Dec"));
    assert_eq!(labels(&utc, 1.5), some("1 Jan 1970", "1 Jan"));
}

#[test]
fn years_far_out_and_invalid_dates() {
    let utc = Clock::utc(NOW);
    // year 0 is 1 BC: JavaScript prints it as -1 two years earlier
    assert_eq!(
        labels(&utc, -62_198_755_200_000.0),
        some("1 Jan -1", "1 Jan")
    );
    // the last valid time value
    assert_eq!(labels(&utc, 8.64e15), some("13 Sep 275760", "13 Sep"));
    // past it: Invalid Date
    assert_eq!(labels(&utc, 8.64e15 + 1.0), (None, None));
    for created in [f64::NAN, f64::INFINITY] {
        assert_eq!(labels(&utc, created), (None, None));
    }
    assert_eq!(get_sticky_note_date_label(None, false, &utc), None);
}

fn note(width: f64, height: f64, created: Option<f64>) -> Element {
    let mut raw = json!({
        "id": "n", "type": "stickynote", "x": 0, "y": 0, "width": width, "height": height,
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "#ffdf6b",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
        "opacity": 100, "groupIds": [], "frameId": null, "index": null, "roundness": null,
        "seed": 1, "version": 1, "versionNonce": 0, "isDeleted": false, "boundElements": null,
        "updated": 1, "link": null, "locked": false, "baseHeight": height
    });
    if let Some(created) = created {
        raw["created"] = json!(created);
    }
    Element::from_map(raw.as_object().unwrap().clone()).unwrap()
}

#[test]
fn the_footer_sits_bottom_right_and_drops_the_year_when_narrow() {
    let utc = Clock::utc(NOW);
    // 2025-12-31 23:30 UTC, the year before NOW
    let created = Some(1_767_223_800_000.0);
    let wide = get_sticky_note_footer(&note(112.0, 75.0, created), &utc).unwrap();
    assert_eq!(wide.text, "31 Dec 2025");
    assert_eq!((wide.x, wide.y), (96.0, 61.0));
    // a body under 80 px: the short date
    let narrow = get_sticky_note_footer(&note(111.0, 75.0, created), &utc).unwrap();
    assert_eq!(narrow.text, "31 Dec");
    // under STICKY_NOTE_MIN_SIZE either way, or undated: none
    assert_eq!(
        get_sticky_note_footer(&note(74.0, 200.0, created), &utc),
        None
    );
    assert_eq!(
        get_sticky_note_footer(&note(200.0, 74.0, created), &utc),
        None
    );
    assert_eq!(
        get_sticky_note_footer(&note(200.0, 200.0, None), &utc),
        None
    );
}

#[test]
fn seeded_random_is_mulberry32() {
    // seededRandom(1), seededRandom(0) and seededRandom(-1) under Node
    let take = |seed: f64| {
        let mut r = seeded_random(seed);
        [r(), r(), r()]
    };
    assert_eq!(
        take(1.0),
        [0.6270739405881613, 0.002735721180215478, 0.5274470399599522]
    );
    assert_eq!(
        take(0.0),
        [
            0.26642920868471265,
            0.0003297457005828619,
            0.2232720274478197
        ]
    );
    assert_eq!(
        take(-1.0),
        [0.8964226141106337, 0.189478256739676, 0.7156526781618595]
    );
}
