//! The pixel diff harness (ex-401): what `excali_raster::diff` counts as a
//! difference, how a tolerance reads, and the report a failure prints.

use excali_raster::diff::{compare, diff_image, Pixel, SizeMismatch, Tolerance};
use excali_raster::tiny_skia::{Pixmap, PremultipliedColorU8};

fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Pixmap {
    let mut p = Pixmap::new(w, h).unwrap();
    let c = PremultipliedColorU8::from_rgba(rgba[0], rgba[1], rgba[2], rgba[3]).unwrap();
    p.pixels_mut().fill(c);
    p
}

fn set(p: &mut Pixmap, x: u32, y: u32, rgba: [u8; 4]) {
    let w = p.width();
    p.pixels_mut()[(y * w + x) as usize] =
        PremultipliedColorU8::from_rgba(rgba[0], rgba[1], rgba[2], rgba[3]).unwrap();
}

#[test]
fn identical_pixmaps_differ_nowhere() {
    let a = solid(4, 3, [10, 20, 30, 255]);
    let d = compare(a.as_ref(), a.as_ref()).unwrap();
    assert_eq!((d.width, d.height), (4, 3));
    assert_eq!(d.max_channel, 0);
    assert_eq!(d.pixels_over(0), 0);
    assert_eq!(d.worst, None);
    assert!(d.within(&Tolerance::EXACT));
}

#[test]
fn a_pixel_differs_by_its_largest_channel_difference() {
    let a = solid(4, 4, [100, 100, 100, 255]);
    let mut b = a.clone();
    // Channels off by 1, 3 and 2: the pixel counts as 3.
    set(&mut b, 2, 1, [101, 97, 102, 255]);
    let d = compare(a.as_ref(), b.as_ref()).unwrap();
    assert_eq!(d.max_channel, 3);
    assert_eq!(d.pixels_over(2), 1);
    assert_eq!(d.pixels_over(3), 0);
    assert_eq!(
        d.worst,
        Some(Pixel {
            x: 2,
            y: 1,
            actual: [100, 100, 100, 255],
            expected: [101, 97, 102, 255],
        })
    );
}

#[test]
fn a_tolerance_is_a_channel_bound_and_a_pixel_budget() {
    let a = solid(8, 8, [0, 0, 0, 0]);
    let mut b = a.clone();
    set(&mut b, 0, 0, [0, 0, 0, 4]);
    set(&mut b, 1, 0, [0, 0, 0, 9]);
    set(&mut b, 2, 0, [0, 0, 0, 30]);
    let d = compare(a.as_ref(), b.as_ref()).unwrap();
    assert_eq!(d.max_channel, 30);
    // Every pixel within 30.
    assert!(d.within(&Tolerance {
        channel: 30,
        pixels: 0
    }));
    // Two pixels over 8: a budget of two passes, one fails.
    assert!(d.within(&Tolerance {
        channel: 8,
        pixels: 2
    }));
    assert!(!d.within(&Tolerance {
        channel: 8,
        pixels: 1
    }));
    // Three pixels over 3.
    assert_eq!(d.pixels_over(3), 3);
    assert!(!d.within(&Tolerance::EXACT));
}

#[test]
fn alpha_and_premultiplied_colour_both_count() {
    // Premultiplied comparison: the same colour at another alpha differs
    // in every channel, and a transparent pixel equals only transparent.
    let a = solid(1, 1, [128, 0, 0, 128]);
    let b = solid(1, 1, [64, 0, 0, 64]);
    let d = compare(a.as_ref(), b.as_ref()).unwrap();
    assert_eq!(d.max_channel, 64);
}

#[test]
fn the_worst_pixel_is_the_first_with_the_largest_difference() {
    let a = solid(3, 3, [0, 0, 0, 255]);
    let mut b = a.clone();
    set(&mut b, 2, 0, [5, 0, 0, 255]);
    set(&mut b, 1, 1, [9, 0, 0, 255]);
    set(&mut b, 0, 2, [9, 0, 0, 255]);
    let d = compare(a.as_ref(), b.as_ref()).unwrap();
    assert_eq!(d.worst.map(|p| (p.x, p.y)), Some((1, 1)));
}

#[test]
fn sizes_must_match() {
    let a = solid(4, 3, [0, 0, 0, 255]);
    let b = solid(3, 4, [0, 0, 0, 255]);
    assert_eq!(
        compare(a.as_ref(), b.as_ref()),
        Err(SizeMismatch {
            actual: (4, 3),
            expected: (3, 4),
        })
    );
}

#[test]
fn the_report_names_the_counts_and_the_worst_pixel() {
    let a = solid(2, 2, [0, 0, 0, 255]);
    let mut b = a.clone();
    set(&mut b, 1, 0, [0, 40, 0, 255]);
    let d = compare(a.as_ref(), b.as_ref()).unwrap();
    let text = d.report(&Tolerance {
        channel: 10,
        pixels: 0,
    });
    assert!(
        text.contains("1 of 4 pixels differ by more than 10"),
        "{text}"
    );
    assert!(text.contains("allowed 0"), "{text}");
    assert!(text.contains("max channel difference 40"), "{text}");
    assert!(text.contains("(1, 0)"), "{text}");
    assert!(text.contains("[0, 0, 0, 255]"), "{text}");
    assert!(text.contains("[0, 40, 0, 255]"), "{text}");
}

#[test]
fn the_diff_image_marks_pixels_over_the_bound() {
    let a = solid(3, 1, [0, 0, 255, 255]);
    let mut b = a.clone();
    set(&mut b, 1, 0, [0, 2, 255, 255]);
    set(&mut b, 2, 0, [0, 90, 255, 255]);
    let img = diff_image(
        a.as_ref(),
        b.as_ref(),
        &Tolerance {
            channel: 4,
            pixels: 0,
        },
    )
    .unwrap();
    let px = |x: u32| {
        let c = img.pixel(x, 0).unwrap();
        [c.red(), c.green(), c.blue(), c.alpha()]
    };
    // Over the bound: opaque red.
    assert_eq!(px(2), [255, 0, 0, 255]);
    // Within the bound but not equal: opaque yellow.
    assert_eq!(px(1), [255, 255, 0, 255]);
    // Equal: the expected image, faded to grey and opaque.
    let same = px(0);
    assert_eq!(same[3], 255);
    assert_eq!(same[0], same[1]);
    assert_eq!(same[1], same[2]);
    assert!(same[0] > 128, "{same:?}");
}
