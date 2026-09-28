//! The tiny-skia backend consumes the display list (ex-216): fills with
//! their rule, strokes with width, caps, joins and dashes, opacity as
//! `globalAlpha` on every draw, transforms, nested clips, images (crop,
//! smoothing, the dark-theme filter) and text through the caller's
//! rasterizer. Pixels are read back from small pixmaps.

use std::collections::HashMap;

use excali_raster::tiny_skia::{self, Mask, Pixmap, PremultipliedColorU8};
use excali_raster::{render, render_from, render_scaled, TextRasterizer};
use excali_scene::display::{
    Clip, Color, Dash, DisplayItem, DisplayList, FillRule, Font, Group, ImageFilter, ImageItem,
    LineCap, PaintState, Path, Rect, Rgba, Stroke, TextRun, Transform,
};

/// Records what the backend hands the text rasterizer, and marks the
/// run's origin pixel so the call is visible in the output.
#[derive(Default)]
struct Texts(Vec<(String, [f32; 4], [f32; 6], bool)>);

impl TextRasterizer for Texts {
    fn fill_text(
        &mut self,
        target: &mut Pixmap,
        run: &TextRun,
        color: tiny_skia::Color,
        transform: tiny_skia::Transform,
        clip: Option<&Mask>,
    ) {
        let t = transform;
        self.0.push((
            run.text.clone(),
            [color.red(), color.green(), color.blue(), color.alpha()],
            [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty],
            clip.is_some(),
        ));
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(color);
        let rect = tiny_skia::Rect::from_xywh(run.x as f32, run.y as f32, 1.0, 1.0).unwrap();
        target.fill_rect(rect, &paint, transform, clip);
    }
}

type Images = HashMap<String, Pixmap>;

fn draw(list: &DisplayList, w: u32, h: u32) -> Pixmap {
    draw_with(list, w, h, &Images::new(), &mut Texts::default())
}

fn draw_with(list: &DisplayList, w: u32, h: u32, images: &Images, texts: &mut Texts) -> Pixmap {
    let mut pixmap = Pixmap::new(w, h).unwrap();
    render(list, &mut pixmap, images, texts);
    pixmap
}

fn px(p: &Pixmap, x: u32, y: u32) -> (u8, u8, u8, u8) {
    let c: PremultipliedColorU8 = p.pixel(x, y).unwrap();
    (c.red(), c.green(), c.blue(), c.alpha())
}

fn near(actual: (u8, u8, u8, u8), expected: (u8, u8, u8, u8)) -> bool {
    let d = |a: u8, b: u8| (i16::from(a) - i16::from(b)).abs() <= 1;
    d(actual.0, expected.0)
        && d(actual.1, expected.1)
        && d(actual.2, expected.2)
        && d(actual.3, expected.3)
}

const CLEAR: (u8, u8, u8, u8) = (0, 0, 0, 0);
const RED: (u8, u8, u8, u8) = (255, 0, 0, 255);

fn fill(path: Path, color: &str, rule: FillRule) -> DisplayItem {
    DisplayItem::Fill {
        path,
        color: Color::new(color),
        rule,
    }
}

fn list(items: Vec<DisplayItem>) -> DisplayList {
    items.into_iter().collect()
}

fn group(
    transform: Transform,
    opacity: f64,
    clip: Option<Clip>,
    items: Vec<DisplayItem>,
) -> DisplayItem {
    DisplayItem::Group(Group {
        transform,
        opacity,
        clip,
        items,
    })
}

#[test]
fn fills_a_rectangle() {
    let p = draw(
        &list(vec![fill(
            Path::rect(2.0, 2.0, 4.0, 4.0),
            "#ff0000",
            FillRule::NonZero,
        )]),
        10,
        10,
    );
    assert_eq!(px(&p, 3, 3), RED);
    assert_eq!(px(&p, 5, 5), RED);
    assert_eq!(px(&p, 1, 1), CLEAR);
    assert_eq!(px(&p, 6, 6), CLEAR);
}

#[test]
fn fill_rules() {
    // Two squares wound the same way: nonzero fills the middle, evenodd
    // leaves a hole (renderElement.ts:786-818 punches arrow-label holes with
    // an evenodd clip).
    let mut path = Path::rect(0.0, 0.0, 10.0, 10.0);
    path.commands
        .extend(Path::rect(3.0, 3.0, 4.0, 4.0).commands);
    let nonzero = draw(
        &list(vec![fill(path.clone(), "red", FillRule::NonZero)]),
        10,
        10,
    );
    let evenodd = draw(&list(vec![fill(path, "red", FillRule::EvenOdd)]), 10, 10);
    assert_eq!(px(&nonzero, 5, 5), RED);
    assert_eq!(px(&evenodd, 5, 5), CLEAR);
    assert_eq!(px(&evenodd, 1, 1), RED);
}

#[test]
fn opacity_multiplies_every_draw() {
    // globalAlpha 0.5 on an opaque red: premultiplied (128, 0, 0, 128).
    let l = list(vec![group(
        Transform::IDENTITY,
        0.5,
        None,
        vec![fill(
            Path::rect(0.0, 0.0, 4.0, 4.0),
            "#ff0000",
            FillRule::NonZero,
        )],
    )]);
    let p = draw(&l, 4, 4);
    assert!(near(px(&p, 1, 1), (128, 0, 0, 128)), "{:?}", px(&p, 1, 1));
    // The colour's own alpha multiplies too: 0.5 × 0.5 of blue.
    let l = list(vec![group(
        Transform::IDENTITY,
        0.5,
        None,
        vec![fill(
            Path::rect(0.0, 0.0, 4.0, 4.0),
            "rgba(0, 0, 255, 0.5)",
            FillRule::NonZero,
        )],
    )]);
    let p = draw(&l, 4, 4);
    assert!(near(px(&p, 1, 1), (0, 0, 64, 64)), "{:?}", px(&p, 1, 1));
    // Overlapping draws in one group compound, as they do on a canvas
    // (upstream's rough strokes darken where they cross).
    let l = list(vec![group(
        Transform::IDENTITY,
        0.5,
        None,
        vec![
            fill(Path::rect(0.0, 0.0, 4.0, 4.0), "#ff0000", FillRule::NonZero),
            fill(Path::rect(0.0, 0.0, 4.0, 4.0), "#ff0000", FillRule::NonZero),
        ],
    )]);
    let p = draw(&l, 4, 4);
    assert!(near(px(&p, 1, 1), (191, 0, 0, 191)), "{:?}", px(&p, 1, 1));
}

#[test]
fn transforms_compose() {
    let l = list(vec![group(
        Transform::translate(5.0, 0.0),
        1.0,
        None,
        vec![group(
            Transform::scale(2.0, 2.0),
            1.0,
            None,
            vec![fill(
                Path::rect(0.0, 0.0, 1.0, 1.0),
                "red",
                FillRule::NonZero,
            )],
        )],
    )]);
    let p = draw(&l, 10, 10);
    assert_eq!(px(&p, 5, 0), RED);
    assert_eq!(px(&p, 6, 1), RED);
    assert_eq!(px(&p, 4, 0), CLEAR);
    assert_eq!(px(&p, 7, 2), CLEAR);
}

#[test]
fn render_scaled_is_the_device_pixel_ratio() {
    let l = list(vec![fill(
        Path::rect(0.0, 0.0, 2.0, 2.0),
        "red",
        FillRule::NonZero,
    )]);
    let mut p = Pixmap::new(8, 8).unwrap();
    render_scaled(&l, &mut p, 2.0, &Images::new(), &mut Texts::default());
    assert_eq!(px(&p, 3, 3), RED);
    assert_eq!(px(&p, 4, 4), CLEAR);
}

fn hline(x0: f64, x1: f64, y: f64) -> Path {
    let mut p = Path::new();
    p.move_to(x0, y).line_to(x1, y);
    p
}

#[test]
fn strokes_have_width() {
    let l = list(vec![DisplayItem::Stroke {
        path: hline(0.0, 20.0, 5.0),
        stroke: Stroke::new(Color::new("red"), 2.0),
    }]);
    let p = draw(&l, 20, 10);
    assert_eq!(px(&p, 10, 4), RED);
    assert_eq!(px(&p, 10, 5), RED);
    assert_eq!(px(&p, 10, 3), CLEAR);
    assert_eq!(px(&p, 10, 6), CLEAR);
}

#[test]
fn stroke_caps() {
    // Butt ends at the end point; round and square extend by half the width.
    let at = |cap: LineCap| {
        let l = list(vec![DisplayItem::Stroke {
            path: hline(6.0, 14.0, 5.0),
            stroke: Stroke::new(Color::new("red"), 4.0).with_cap(cap),
        }]);
        px(&draw(&l, 20, 10), 4, 5)
    };
    assert_eq!(at(LineCap::Butt), CLEAR);
    assert_eq!(at(LineCap::Square), RED);
    assert_ne!(at(LineCap::Round), CLEAR);
}

#[test]
fn strokes_are_dashed() {
    let l = list(vec![DisplayItem::Stroke {
        path: hline(0.0, 20.0, 5.0),
        stroke: Stroke::new(Color::new("red"), 2.0).with_dash(Dash::new(&[4.0, 4.0], 0.0)),
    }]);
    let p = draw(&l, 20, 10);
    assert_eq!(px(&p, 1, 5), RED);
    assert_eq!(px(&p, 5, 5), CLEAR);
    assert_eq!(px(&p, 9, 5), RED);
    // The offset shifts the pattern.
    let l = list(vec![DisplayItem::Stroke {
        path: hline(0.0, 20.0, 5.0),
        stroke: Stroke::new(Color::new("red"), 2.0).with_dash(Dash::new(&[4.0, 4.0], 4.0)),
    }]);
    let p = draw(&l, 20, 10);
    assert_eq!(px(&p, 1, 5), CLEAR);
    assert_eq!(px(&p, 5, 5), RED);
}

#[test]
fn a_width_canvas_ignores_strokes_one_pixel() {
    let l = list(vec![DisplayItem::Stroke {
        path: hline(0.0, 20.0, 5.0),
        stroke: Stroke::new(Color::new("red"), 0.0),
    }]);
    let p = draw(&l, 20, 10);
    assert!(near(px(&p, 10, 4), (128, 0, 0, 128)), "{:?}", px(&p, 10, 4));
    assert!(near(px(&p, 10, 5), (128, 0, 0, 128)), "{:?}", px(&p, 10, 5));
}

#[test]
fn arcs_draw() {
    let mut circle = Path::new();
    circle.arc(10.0, 10.0, 8.0, 0.0, std::f64::consts::TAU, false);
    let p = draw(&list(vec![fill(circle, "red", FillRule::NonZero)]), 20, 20);
    assert_eq!(px(&p, 10, 10), RED);
    assert_eq!(px(&p, 10, 3), RED);
    assert_eq!(px(&p, 1, 1), CLEAR);
}

#[test]
fn clips_intersect_and_pop() {
    let everything = || fill(Path::rect(0.0, 0.0, 10.0, 10.0), "red", FillRule::NonZero);
    let l = list(vec![
        group(
            Transform::IDENTITY,
            1.0,
            Some(Clip {
                path: Path::rect(0.0, 0.0, 6.0, 10.0),
                rule: FillRule::NonZero,
            }),
            vec![group(
                Transform::IDENTITY,
                1.0,
                Some(Clip {
                    path: Path::rect(4.0, 0.0, 6.0, 5.0),
                    rule: FillRule::NonZero,
                }),
                vec![everything()],
            )],
        ),
        group(
            Transform::translate(0.0, 8.0),
            1.0,
            None,
            vec![fill(
                Path::rect(0.0, 0.0, 10.0, 2.0),
                "#0000ff",
                FillRule::NonZero,
            )],
        ),
    ]);
    let p = draw(&l, 10, 10);
    assert_eq!(px(&p, 5, 2), RED, "inside both clips");
    assert_eq!(px(&p, 2, 2), CLEAR, "outside the inner clip");
    assert_eq!(px(&p, 7, 2), CLEAR, "outside the outer clip");
    assert_eq!(px(&p, 5, 7), CLEAR, "below the inner clip");
    assert_eq!(px(&p, 8, 9), (0, 0, 255, 255), "clips popped");
}

#[test]
fn clips_are_in_the_group_space() {
    let l = list(vec![group(
        Transform::translate(5.0, 0.0),
        1.0,
        Some(Clip {
            path: Path::rect(0.0, 0.0, 2.0, 10.0),
            rule: FillRule::NonZero,
        }),
        vec![fill(
            Path::rect(-5.0, 0.0, 10.0, 10.0),
            "red",
            FillRule::NonZero,
        )],
    )]);
    let p = draw(&l, 10, 10);
    assert_eq!(px(&p, 5, 5), RED);
    assert_eq!(px(&p, 6, 5), RED);
    assert_eq!(px(&p, 4, 5), CLEAR);
    assert_eq!(px(&p, 7, 5), CLEAR);
}

#[test]
fn an_empty_clip_hides_everything() {
    let l = list(vec![group(
        Transform::IDENTITY,
        1.0,
        Some(Clip {
            path: Path::new(),
            rule: FillRule::NonZero,
        }),
        vec![fill(
            Path::rect(0.0, 0.0, 4.0, 4.0),
            "red",
            FillRule::NonZero,
        )],
    )]);
    let p = draw(&l, 4, 4);
    assert_eq!(px(&p, 1, 1), CLEAR);
}

#[test]
fn a_colour_the_canvas_ignores_paints_in_the_current_style() {
    // The canvas keeps the current style when a fillStyle/strokeStyle
    // assignment does not parse; on a fresh context that is black, so an
    // element whose strokeColor is "blue-ish" draws black upstream.
    // "transparent" is a colour and paints nothing.
    const BLACK: (u8, u8, u8, u8) = (0, 0, 0, 255);
    let l = list(vec![
        fill(Path::rect(0.0, 0.0, 2.0, 4.0), "", FillRule::NonZero),
        DisplayItem::Stroke {
            path: hline(2.0, 4.0, 1.0),
            stroke: Stroke::new(Color::new("blue-ish"), 2.0),
        },
        fill(
            Path::rect(2.0, 2.0, 2.0, 2.0),
            "transparent",
            FillRule::NonZero,
        ),
    ]);
    let p = draw(&l, 4, 4);
    assert_eq!(px(&p, 1, 1), BLACK);
    assert_eq!(px(&p, 3, 0), BLACK);
    assert_eq!(px(&p, 3, 3), CLEAR);

    // From a base state the current styles are the base's.
    let base = PaintState {
        fill_style: Rgba {
            r: 0,
            g: 0,
            b: 255,
            a: 1.0,
        },
        stroke_style: Rgba {
            r: 0,
            g: 255,
            b: 0,
            a: 1.0,
        },
        ..PaintState::ROOT
    };
    let mut p = Pixmap::new(4, 4).unwrap();
    render_from(&l, &mut p, base, &Images::new(), &mut Texts::default());
    assert_eq!(px(&p, 1, 1), (0, 0, 255, 255));
    assert_eq!(px(&p, 3, 0), (0, 255, 0, 255));
}

#[test]
fn css_color_4_colours_paint_as_on_the_canvas() {
    // rgb() with a slash alpha is half-transparent; hsl() with a unit and
    // hwb() are colours; a hex string without "#" is not (black).
    let l = list(vec![
        fill(
            Path::rect(0.0, 0.0, 1.0, 1.0),
            "rgb(255 0 0 / 50%)",
            FillRule::NonZero,
        ),
        fill(
            Path::rect(1.0, 0.0, 1.0, 1.0),
            "hsl(120deg, 100%, 50%)",
            FillRule::NonZero,
        ),
        fill(
            Path::rect(2.0, 0.0, 1.0, 1.0),
            "hwb(0 0% 0%)",
            FillRule::NonZero,
        ),
        fill(Path::rect(3.0, 0.0, 1.0, 1.0), "ff0000", FillRule::NonZero),
    ]);
    let p = draw(&l, 4, 1);
    assert!(near(px(&p, 0, 0), (128, 0, 0, 128)), "{:?}", px(&p, 0, 0));
    assert_eq!(px(&p, 1, 0), (0, 255, 0, 255));
    assert_eq!(px(&p, 2, 0), RED);
    assert_eq!(px(&p, 3, 0), (0, 0, 0, 255));
}

/// A 2×2 image: red, green / blue, white.
fn quad() -> Pixmap {
    let mut p = Pixmap::new(2, 2).unwrap();
    let colors = [
        tiny_skia::Color::from_rgba8(255, 0, 0, 255),
        tiny_skia::Color::from_rgba8(0, 255, 0, 255),
        tiny_skia::Color::from_rgba8(0, 0, 255, 255),
        tiny_skia::Color::from_rgba8(255, 255, 255, 255),
    ];
    for (i, c) in colors.into_iter().enumerate() {
        p.pixels_mut()[i] = c.premultiply().to_color_u8();
    }
    p
}

fn images() -> Images {
    let mut m = Images::new();
    m.insert("quad".into(), quad());
    m
}

fn image(dest: Rect) -> ImageItem {
    let mut i = ImageItem::new("quad", dest);
    i.smoothing = false;
    i
}

#[test]
fn images_scale_into_the_destination() {
    let l = list(vec![DisplayItem::Image(image(Rect::new(
        0.0, 0.0, 4.0, 4.0,
    )))]);
    let p = draw_with(&l, 6, 6, &images(), &mut Texts::default());
    assert_eq!(px(&p, 0, 0), RED);
    assert_eq!(px(&p, 1, 1), RED);
    assert_eq!(px(&p, 3, 0), (0, 255, 0, 255));
    assert_eq!(px(&p, 0, 3), (0, 0, 255, 255));
    assert_eq!(px(&p, 3, 3), (255, 255, 255, 255));
    assert_eq!(px(&p, 4, 4), CLEAR);
}

#[test]
fn images_crop_to_the_source_rect() {
    // element.crop: drawImage(img, crop.x, crop.y, crop.width, crop.height, …).
    let mut i = image(Rect::new(0.0, 0.0, 4.0, 4.0));
    i.source = Some(Rect::new(1.0, 0.0, 1.0, 1.0));
    let p = draw_with(
        &list(vec![DisplayItem::Image(i)]),
        4,
        4,
        &images(),
        &mut Texts::default(),
    );
    for (x, y) in [(0, 0), (3, 3), (0, 3)] {
        assert_eq!(px(&p, x, y), (0, 255, 0, 255));
    }
}

#[test]
fn image_source_outside_the_bitmap_is_clipped() {
    // drawImage: "the source rectangle is clipped to the image, and the
    // destination rectangle is clipped in the same proportion".
    let mut i = image(Rect::new(0.0, 0.0, 8.0, 4.0));
    i.source = Some(Rect::new(0.0, 0.0, 4.0, 2.0));
    let p = draw_with(
        &list(vec![DisplayItem::Image(i)]),
        8,
        4,
        &images(),
        &mut Texts::default(),
    );
    assert_eq!(px(&p, 0, 0), RED);
    assert_eq!(px(&p, 3, 3), (255, 255, 255, 255));
    assert_eq!(px(&p, 5, 1), CLEAR);
}

#[test]
fn image_smoothing_interpolates() {
    let mut i = image(Rect::new(0.0, 0.0, 8.0, 8.0));
    i.smoothing = true;
    let p = draw_with(
        &list(vec![DisplayItem::Image(i)]),
        8,
        8,
        &images(),
        &mut Texts::default(),
    );
    let (r, g, _, a) = px(&p, 4, 1);
    assert_eq!(a, 255);
    assert!(
        r > 0 && g > 0,
        "a blend of red and green, got {:?}",
        px(&p, 4, 1)
    );
}

#[test]
fn images_take_the_opacity_and_the_dark_filter() {
    let l = list(vec![group(
        Transform::IDENTITY,
        0.5,
        None,
        vec![DisplayItem::Image(image(Rect::new(0.0, 0.0, 2.0, 2.0)))],
    )]);
    let p = draw_with(&l, 2, 2, &images(), &mut Texts::default());
    assert!(near(px(&p, 0, 0), (128, 0, 0, 128)), "{:?}", px(&p, 0, 0));

    let mut i = image(Rect::new(0.0, 0.0, 2.0, 2.0));
    i.filter = Some(ImageFilter::DarkTheme);
    let p = draw_with(
        &list(vec![DisplayItem::Image(i)]),
        2,
        2,
        &images(),
        &mut Texts::default(),
    );
    let (r, g, b) = ImageFilter::DarkTheme.apply_rgb(255, 255, 255);
    assert_eq!(px(&p, 1, 1), (r, g, b, 255));
    let (r, g, b) = ImageFilter::DarkTheme.apply_rgb(255, 0, 0);
    assert_eq!(px(&p, 0, 0), (r, g, b, 255));
}

#[test]
fn unknown_images_draw_nothing() {
    let mut i = image(Rect::new(0.0, 0.0, 2.0, 2.0));
    i.id = "missing".into();
    let p = draw_with(
        &list(vec![DisplayItem::Image(i)]),
        2,
        2,
        &images(),
        &mut Texts::default(),
    );
    assert_eq!(px(&p, 0, 0), CLEAR);
}

#[test]
fn text_goes_to_the_rasterizer_with_resolved_state() {
    let run = TextRun::new(
        "hi",
        1.0,
        2.0,
        Font::new(20.0, "Virgil"),
        Color::new("#ff0000"),
    );
    let l = list(vec![
        group(
            Transform::translate(3.0, 4.0),
            0.5,
            Some(Clip {
                path: Path::rect(0.0, 0.0, 10.0, 10.0),
                rule: FillRule::NonZero,
            }),
            vec![DisplayItem::Text(run.clone())],
        ),
        DisplayItem::Text(run),
    ]);
    let mut texts = Texts::default();
    let p = draw_with(&l, 10, 10, &Images::new(), &mut texts);
    assert_eq!(
        texts.0,
        vec![
            (
                "hi".to_string(),
                [1.0, 0.0, 0.0, 0.5],
                [1.0, 0.0, 0.0, 1.0, 3.0, 4.0],
                true
            ),
            (
                "hi".to_string(),
                [1.0, 0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                false
            ),
        ]
    );
    assert_eq!(px(&p, 1, 2), RED);
}

/// ADR-008: backends know nothing about elements. This crate's library
/// code depends on no workspace package but `excali-scene` (by package
/// name, as `cargo metadata` resolves it, so a renamed dependency is seen),
/// it reaches into that crate only through `excali_scene::display`, and no
/// identifier in its code names an element (`HtmlImageElement`, the
/// browser's image type, aside). The tests and the `png_export` example
/// build their scenes from elements (`excali-core`, through
/// `excali_scene::canvas_export`, measuring text with `excali-text`), as the
/// callers of the backend do; the list of those is fixed too.
#[test]
fn no_element_knowledge() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let (normal, all) = workspace_dependencies(std::path::Path::new(manifest));
    assert_eq!(normal, ["excali-scene"], "dependencies: {normal:?}");
    assert_eq!(
        all,
        ["excali-core", "excali-scene", "excali-text"],
        "all dependencies: {all:?}"
    );
    const ALLOWED: [&str; 1] = ["HtmlImageElement"];
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut sources = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let code = code_without_comments(&std::fs::read_to_string(&path).unwrap());
        sources += 1;
        for p in paths(&code) {
            let segments: Vec<&str> = p.split("::").collect();
            for s in &segments {
                assert!(
                    !s.contains("Element") || ALLOWED.contains(s),
                    "{} names {s}",
                    path.display()
                );
            }
        }
        for r in roots(&code, |w| w.starts_with("excali_")) {
            assert_eq!(r, "excali_scene::display", "{} uses {r}", path.display());
        }
    }
    assert!(sources > 0);
}

/// The workspace packages the package at `manifest` depends on: its normal
/// dependencies (target-specific ones included), and all of them (dev and
/// build too), by package name, from `cargo metadata --format-version 1
/// --no-deps`: a renamed dependency (`ec = { package = "excali-core", .. }`)
/// is listed as the package it names, and a path dependency counts as a
/// workspace one.
fn workspace_dependencies(manifest: &std::path::Path) -> (Vec<String>, Vec<String>) {
    let out = std::process::Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "cargo metadata: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let members: Vec<&str> = packages
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    let manifest = manifest.canonicalize().unwrap();
    let this = packages
        .iter()
        .find(|p| {
            std::path::Path::new(p["manifest_path"].as_str().unwrap())
                .canonicalize()
                .is_ok_and(|m| m == manifest)
        })
        .unwrap();
    let internal: Vec<&serde_json::Value> = this["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| members.contains(&d["name"].as_str().unwrap()) || d.get("path").is_some())
        .collect();
    let names = |normal_only: bool| {
        let mut v: Vec<String> = internal
            .iter()
            .filter(|d| !normal_only || d["kind"].is_null())
            .map(|d| d["name"].as_str().unwrap().to_string())
            .collect();
        v.sort();
        v.dedup();
        v
    };
    (names(true), names(false))
}

/// A throwaway workspace with `excali-scene`, `excali-core` and a backend
/// whose `[dependencies]`/`[dev-dependencies]` section is `deps`; returns the
/// backend's manifest.
fn fake_backend(name: &str, deps: &str) -> std::path::PathBuf {
    let root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("no-element-knowledge")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    let krate = |dir: &str, package: &str, extra: &str| {
        std::fs::create_dir_all(root.join(dir).join("src")).unwrap();
        std::fs::write(root.join(dir).join("src/lib.rs"), "").unwrap();
        std::fs::write(
            root.join(dir).join("Cargo.toml"),
            format!(
                "[package]\nname = \"{package}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n{extra}"
            ),
        )
        .unwrap();
    };
    krate("scene", "excali-scene", "");
    krate("core", "excali-core", "");
    krate("backend", "backend", deps);
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"scene\", \"core\", \"backend\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    root.join("backend/Cargo.toml")
}

#[test]
fn the_dependency_check_sees_renamed_packages() {
    let scene = "excali-scene = { path = \"../scene\" }\n";
    let ok = fake_backend("ok", &format!("[dependencies]\n{scene}"));
    let (normal, all) = workspace_dependencies(&ok);
    assert_eq!(normal, ["excali-scene"]);
    assert_eq!(all, ["excali-scene"]);
    for (name, deps) in [
        (
            "renamed",
            format!("[dependencies]\n{scene}ec = {{ package = \"excali-core\", path = \"../core\" }}\n"),
        ),
        (
            "renamed-table",
            format!("[dependencies]\n{scene}[dependencies.ec]\npackage = \"excali-core\"\npath = \"../core\"\n"),
        ),
        (
            "dev",
            format!("[dependencies]\n{scene}[dev-dependencies]\nec = {{ package = \"excali-core\", path = \"../core\" }}\n"),
        ),
        (
            "target",
            format!("[dependencies]\n{scene}[target.'cfg(unix)'.dependencies]\nec = {{ package = \"excali-core\", path = \"../core\" }}\n"),
        ),
    ] {
        let manifest = fake_backend(name, &deps);
        let (normal, all) = workspace_dependencies(&manifest);
        assert_eq!(all, ["excali-core", "excali-scene"], "{name}");
        // a dev-dependency is not the library's; a target-specific one is
        let expected: &[&str] = if name == "dev" {
            &["excali-scene"]
        } else {
            &["excali-core", "excali-scene"]
        };
        assert_eq!(normal, expected, "{name}");
    }
}

#[test]
fn the_element_check_sees_code_and_skips_comments() {
    let code = code_without_comments(
        "use excali_core::element::TextElement; // renderElement.ts\n/* ElementKind */ let x = 1;",
    );
    assert_eq!(
        paths(&code),
        ["use", "excali_core::element::TextElement", "let", "x", "1"]
    );
}

#[test]
fn the_path_check_sees_groups_globs_and_renames() {
    let code = "use excali_scene::display::{Path}; use excali_scene::{shape::ShapeCache};\n\
                use excali_scene :: * ; use excali_scene as s; use ::excali_scene::shape;\n\
                use excali_core::x; pub(crate) fn f() {}";
    assert_eq!(
        roots(code, |w| w.starts_with("excali_")),
        [
            "excali_scene::display",
            "excali_scene::{",
            "excali_scene::*",
            "excali_scene",
            "excali_scene::shape",
            "excali_core::x",
        ]
    );
}

/// The code of a Rust source with `//` and `/* */` comments removed (doc
/// comments cite upstream files such as `renderElement.ts`).
fn code_without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let line = rest.find("//");
        let block = rest.find("/*");
        match (line, block) {
            (Some(l), b) if b.is_none_or(|b| l < b) => {
                out.push_str(&rest[..l]);
                rest = rest[l..].find('\n').map_or("", |e| &rest[l + e..]);
            }
            (_, Some(b)) => {
                out.push_str(&rest[..b]);
                rest = rest[b..].find("*/").map_or("", |e| &rest[b + e + 2..]);
            }
            _ => {
                out.push_str(rest);
                rest = "";
            }
        }
    }
    out
}

/// Every identifier and `::` path in `code`.
fn paths(code: &str) -> Vec<String> {
    code.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
        .map(|p| p.trim_matches(':'))
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Where every path that starts at a root `is_root` accepts leads: `root::next`
/// with the segment after the first `::` (`{` for a group, `*` for a glob),
/// `root)` for a visibility such as `pub(crate)`, or `root` alone (a bare or
/// renamed import). Every occurrence counts, so a root inside a group
/// (`use a::{super::x}`) or after a leading `::` is seen, and `super::super`
/// shows as such.
fn roots(code: &str, is_root: impl Fn(&str) -> bool) -> Vec<String> {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut rest = code;
    while let Some(start) = rest.find(ident) {
        let len = rest[start..]
            .find(|c: char| !ident(c))
            .unwrap_or(rest.len() - start);
        let word = &rest[start..start + len];
        rest = &rest[start + len..];
        if !is_root(word) {
            continue;
        }
        let after = rest.trim_start();
        out.push(match after.strip_prefix("::").map(str::trim_start) {
            Some(next) => {
                let n = next.find(|c: char| !ident(c)).unwrap_or(next.len());
                let segment = match n {
                    0 => next.chars().next().map_or(String::new(), String::from),
                    _ => next[..n].to_owned(),
                };
                format!("{word}::{segment}")
            }
            None if after.starts_with(')') => format!("{word})"),
            None => word.to_owned(),
        });
    }
    out
}

fn coverage(p: &Pixmap) -> usize {
    p.pixels().iter().filter(|c| c.alpha() > 0).count()
}

fn huge_triangle() -> Path {
    let mut p = Path::new();
    p.move_to(-1e30, -1e30)
        .line_to(1e30, 5.0)
        .line_to(5.0, 1e30)
        .close();
    p
}

#[test]
fn huge_finite_coordinates_draw_without_overflow() {
    // Bounds past the i32 range: Skia's safeRoundOut pins them to
    // +-(2^31 - 1) >> 2 before any width or height is taken. Chrome 153
    // covers the whole 200x200 canvas with the triangle.
    let p = draw(
        &list(vec![fill(huge_triangle(), "red", FillRule::NonZero)]),
        200,
        200,
    );
    assert_eq!(coverage(&p), 200 * 200);

    let stroke = |path: Path, width: f64| DisplayItem::Stroke {
        path,
        stroke: Stroke::new(Color::new("red"), width),
    };
    draw(&list(vec![stroke(huge_triangle(), 3.0)]), 200, 200);

    let mut curve = Path::new();
    curve
        .move_to(0.0, 0.0)
        .cubic_to(1e20, 0.0, -1e20, 200.0, 200.0, 200.0);
    draw(&list(vec![fill(curve, "red", FillRule::NonZero)]), 200, 200);

    let mut quad = Path::new();
    quad.move_to(0.5, 0.5).quad_to(1e15, 1e15, 199.0, 0.5);
    let p = draw(&list(vec![stroke(quad, 5.0)]), 200, 200);
    // The curve leaves (0.5, 0.5) along the diagonal.
    assert_ne!(px(&p, 20, 20), CLEAR);

    // A dashed, round-capped cubic with control points past 1e20: past
    // SkDashPath's 1,000,000 dashes, so it is stroked undashed, and the
    // stroker's CheckCubicLinear finds the cubic in line with a maximum
    // curvature that solves to NaN, which SkTPin takes to 0: a line to the
    // end. Chrome 153 draws the line from (0.5, 0.5) to (150, 150) (the
    // extremes fixture), where a NaN point in the outline once reached
    // f64::clamp.
    for huge in [1e20, 1e30] {
        let p = draw(
            &list(vec![dashed_huge_cubic(huge, 4.0, LineCap::Round)]),
            200,
            200,
        );
        assert_eq!(px(&p, 20, 20), RED, "{huge}");
        assert_eq!(px(&p, 140, 140), RED, "{huge}");
        assert_eq!(px(&p, 20, 5), CLEAR, "{huge}");
        assert_eq!(px(&p, 170, 170), CLEAR, "{huge}");
    }
}

fn dashed_huge_cubic(huge: f64, width: f64, cap: LineCap) -> DisplayItem {
    let mut curve = Path::new();
    curve
        .move_to(0.5, 0.5)
        .cubic_to(huge, -huge, -huge, huge, 150.0, 150.0);
    DisplayItem::Stroke {
        path: curve,
        stroke: Stroke::new(Color::new("red"), width)
            .with_cap(cap)
            .with_dash(Dash::new(&[5.0, 5.0], 0.0)),
    }
}

#[test]
fn dashed_hairlines_of_huge_cubics_draw_as_skia_does() {
    // Undashed (too many dashes), the hairline's cubic has coordinates
    // whose products overflow f32: tiny-skia's hairliner takes that for a
    // non-finite point (a debug assertion, and the cubic dropped), where
    // Skia draws it as 512 lines. Chrome 153 covers the pixel at the start
    // (the extremes fixture), and the lines through the middle of the
    // curve cross the canvas along x + y = 150.
    for huge in [1e20, 1e30] {
        for width in [0.5, 1.0] {
            let item = dashed_huge_cubic(huge, width, LineCap::Butt);
            let p = draw(&list(vec![item]), 200, 200);
            assert_ne!(px(&p, 0, 0), CLEAR, "{huge} {width}");
            assert_eq!(px(&p, 20, 20), CLEAR, "{huge} {width}");
            assert_eq!(px(&p, 20, 100), CLEAR, "{huge} {width}");
        }
    }
}

#[test]
fn too_many_dashes_stroke_undashed() {
    // SkDashPath gives up past 1,000,000 dashes and the stroke is drawn
    // without its dash. Chrome 153 paints 200 pixels for this hairline.
    let mut line = Path::new();
    line.move_to(0.0, 0.0).line_to(1e6, 1e6);
    let dashed = |width: f64, cap: LineCap| {
        list(vec![DisplayItem::Stroke {
            path: line.clone(),
            stroke: Stroke::new(Color::new("red"), width)
                .with_cap(cap)
                .with_dash(Dash::new(&[0.0001, 0.0001], 0.0)),
        }])
    };
    assert_eq!(coverage(&draw(&dashed(1.0, LineCap::Butt), 200, 200)), 200);
    let round = draw(&dashed(3.0, LineCap::Round), 200, 200);
    assert_eq!(px(&round, 100, 100), RED);
    // A butt-capped line has become a fill of the line (SpecialLineRec)
    // before the dasher gives up, which covers nothing.
    assert_eq!(coverage(&draw(&dashed(3.0, LineCap::Butt), 200, 200)), 0);
}

#[test]
fn huge_coordinates_in_every_shape_and_stroke_draw_without_panicking() {
    // Every finite coordinate from 1e10 to 3e38 in lines, quadratics,
    // cubics, arcs, rectangles, round rectangles and clip paths, filled,
    // stroked (butt, round), as hairlines (0.5, 1), each undashed and
    // dashed. Chrome draws them all; the port must not panic (in debug,
    // tiny-skia's assertions included).
    let shapes = |h: f64| -> Vec<Path> {
        let mut line = Path::new();
        line.move_to(0.5, 0.5).line_to(h, h);
        let mut triangle = Path::new();
        triangle
            .move_to(-h, -h)
            .line_to(h, 5.0)
            .line_to(5.0, h)
            .close();
        let mut quad = Path::new();
        quad.move_to(0.5, 0.5).quad_to(h, -h, 50.0, 60.0);
        let mut cubic = Path::new();
        cubic.move_to(0.5, 0.5).cubic_to(h, -h, -h, h, 50.0, 60.0);
        let mut far_cubic = Path::new();
        far_cubic
            .move_to(h, 0.0)
            .cubic_to(0.0, h, -h, 0.0, 30.0, 30.0);
        let mut arc = Path::new();
        arc.arc(10.0, 10.0, h, 0.0, 1.5, false);
        let mut far_arc = Path::new();
        far_arc.arc(h, -h, h, 0.0, 3.0, true);
        vec![
            line,
            triangle,
            quad,
            cubic,
            far_cubic,
            arc,
            far_arc,
            Path::rect(-h, -h, 2.0 * h, 2.0 * h),
            Path::round_rect(-h, -h, 2.0 * h, 2.0 * h, 10.0),
            Path::round_rect(0.0, 0.0, 40.0, 40.0, h),
        ]
    };
    let styles = |path: &Path, dash: bool| -> Vec<DisplayItem> {
        let stroke = |width: f64, cap: LineCap| {
            let mut s = Stroke::new(Color::new("red"), width).with_cap(cap);
            if dash {
                s = s.with_dash(Dash::new(&[5.0, 5.0], 0.0));
            }
            DisplayItem::Stroke {
                path: path.clone(),
                stroke: s,
            }
        };
        vec![
            fill(path.clone(), "red", FillRule::NonZero),
            fill(path.clone(), "red", FillRule::EvenOdd),
            stroke(3.0, LineCap::Butt),
            stroke(4.0, LineCap::Round),
            stroke(3.0, LineCap::Square),
            stroke(0.5, LineCap::Butt),
            stroke(1.0, LineCap::Butt),
            stroke(1.0, LineCap::Round),
        ]
    };
    let transforms = [Transform::scale(2.0, 2.0), Transform::scale(0.25, 3.0)];
    for h in [1e10, 1e15, 1e20, 1e25, 1e30, 1e35, 3e38] {
        for path in shapes(h) {
            for dash in [false, true] {
                for item in styles(&path, dash) {
                    draw(&list(vec![item.clone()]), 64, 64);
                    for t in transforms {
                        let scaled = group(t, 1.0, None, vec![item.clone()]);
                        draw(&list(vec![scaled]), 64, 64);
                    }
                    let clip = Clip {
                        path: path.clone(),
                        rule: FillRule::NonZero,
                    };
                    let inner = fill(Path::rect(0.0, 0.0, 64.0, 64.0), "red", FillRule::NonZero);
                    draw(
                        &list(vec![group(
                            Transform::IDENTITY,
                            1.0,
                            Some(clip),
                            vec![inner, item],
                        )]),
                        64,
                        64,
                    );
                }
            }
        }
    }
}

#[test]
fn huge_hairline_curves_are_cut_in_device_space() {
    // Skia cuts a hairline's curves into lines after the matrix: half the
    // cubic scaled by 2 at width 0.5 (coverage 1) is the whole cubic at
    // width 1, pixel for pixel.
    let cubic = |scale: f64| {
        let mut p = Path::new();
        p.move_to(0.5 / scale, 0.5 / scale).cubic_to(
            1e20 / scale,
            -1e20 / scale,
            -1e20 / scale,
            1e20 / scale,
            150.0 / scale,
            150.0 / scale,
        );
        p
    };
    let stroke = |path: Path, width: f64| DisplayItem::Stroke {
        path,
        stroke: Stroke::new(Color::new("red"), width),
    };
    let whole = draw(&list(vec![stroke(cubic(1.0), 1.0)]), 200, 200);
    let scaled = draw(
        &list(vec![group(
            Transform::scale(2.0, 2.0),
            1.0,
            None,
            vec![stroke(cubic(2.0), 0.5)],
        )]),
        200,
        200,
    );
    assert!(coverage(&whole) > 0);
    assert_eq!(whole.data(), scaled.data());
}

fn hairline(path: Path, cap: LineCap) -> DisplayItem {
    DisplayItem::Stroke {
        path,
        stroke: Stroke::new(Color::new("red"), 1.0).with_cap(cap),
    }
}

/// A closed contour whose first segment is a cubic with control points at
/// +-`h`: from (10, 10) to (100, 10), then down to (100, 100) and closed
/// back to (10, 10).
fn closed_with_huge_cubic(h: f64) -> Path {
    let mut p = Path::new();
    p.move_to(10.0, 10.0)
        .cubic_to(h, h, -h, h, 100.0, 10.0)
        .line_to(100.0, 100.0)
        .close();
    p
}

#[test]
fn a_skipped_hairline_curve_leaves_the_rest_of_its_contour() {
    // Skia's hair_cubic draws a cubic only when every point it evaluates
    // is finite; at +-8e37 the coefficients overflow and the cubic alone is
    // skipped. The line after it and the closing line to the contour's
    // first point still draw (Chrome 153: the hairline-skips fixture), and
    // a closed contour takes no caps, so round is the same as butt.
    let butt = draw(
        &list(vec![hairline(closed_with_huge_cubic(8e37), LineCap::Butt)]),
        120,
        120,
    );
    for cap in [LineCap::Butt, LineCap::Round, LineCap::Square] {
        let p = draw(
            &list(vec![hairline(closed_with_huge_cubic(8e37), cap)]),
            120,
            120,
        );
        assert_ne!(px(&p, 100, 50), CLEAR, "the line after the cubic, {cap:?}");
        assert_ne!(px(&p, 40, 40), CLEAR, "the closing line, {cap:?}");
        assert_ne!(px(&p, 70, 70), CLEAR, "the closing line, {cap:?}");
        assert_eq!(px(&p, 50, 30), CLEAR, "no cubic, {cap:?}");
        assert_eq!(p.data(), butt.data(), "{cap:?}");
    }
}

#[test]
fn hairline_segments_beside_a_skipped_curve_take_no_cap() {
    // Round caps extend a hairline only at a contour's start and end
    // (extend_pts: the previous verb a move, the next a move, a close or
    // none). Beside the skipped cubic the neighbouring verb is the cubic,
    // so the lines end flush at x = 50 and x = 70, as Chrome draws them.
    let mut p = Path::new();
    p.move_to(10.0, 50.5)
        .line_to(50.0, 50.5)
        .cubic_to(8e37, 8e37, -8e37, 8e37, 70.0, 50.5)
        .line_to(110.0, 50.5);
    let round = draw(&list(vec![hairline(p, LineCap::Round)]), 120, 120);
    assert_ne!(px(&round, 9, 50), CLEAR, "the start's cap");
    assert_ne!(px(&round, 110, 50), CLEAR, "the end's cap");
    assert_eq!(px(&round, 50, 50), CLEAR, "no cap before the cubic");
    assert_eq!(px(&round, 69, 50), CLEAR, "no cap after the cubic");
    assert_eq!(px(&round, 30, 50), RED);
    assert_eq!(px(&round, 90, 50), RED);
}

#[test]
fn paths_past_a_quarter_of_the_f32_range_draw_nothing() {
    // SkDraw::drawDevPath returns before drawing when the device path's
    // bounds pass SK_ScalarMax / 4 (SkPathPriv::TooBigForMath): Chrome 153
    // draws a line to 8.5070587e37 (2^126) and nothing past it, for fills,
    // strokes and hairlines alike, whatever else the path holds.
    let line_and_quad = |h: f64| {
        let mut p = Path::new();
        p.move_to(10.0, 10.0)
            .line_to(100.0, 100.0)
            .move_to(0.0, 110.0)
            .quad_to(h, h, 50.0, 110.0);
        p
    };
    let triangle = |h: f64| {
        let mut p = Path::new();
        p.move_to(10.0, 10.0)
            .line_to(h, 10.0)
            .line_to(10.0, 100.0)
            .close();
        p
    };
    let wide = |h: f64| {
        let mut p = Path::new();
        p.move_to(10.0, 60.0).line_to(h, 60.0);
        DisplayItem::Stroke {
            path: p,
            stroke: Stroke::new(Color::new("red"), 3.0),
        }
    };
    let thin = |path: Path, cap: LineCap, width: f64| DisplayItem::Stroke {
        path,
        stroke: Stroke::new(Color::new("red"), width).with_cap(cap),
    };
    // Hairlines of `width` in user space.
    let items_of = |h: f64, width: f64| {
        vec![
            thin(line_and_quad(h), LineCap::Butt, width),
            thin(closed_with_huge_cubic(h), LineCap::Butt, width),
            thin(triangle(h), LineCap::Round, width),
            fill(triangle(h), "red", FillRule::NonZero),
            wide(h),
        ]
    };
    let items = |h: f64| items_of(h, 1.0);
    for (i, item) in items(8e37).into_iter().enumerate() {
        assert!(
            coverage(&draw(&list(vec![item]), 120, 120)) > 0,
            "{i} at 8e37"
        );
    }
    for h in [8.6e37, 1e38, 3e38] {
        for (i, item) in items(h).into_iter().enumerate() {
            assert_eq!(
                coverage(&draw(&list(vec![item]), 120, 120)),
                0,
                "{i} at {h}"
            );
        }
    }
    // The bounds are the device path's: under a scale of 2, 5e37 is past
    // (the hairlines are 0.5 wide, 1 in device space, as Chrome draws
    // them; a wider stroke's outline is the stroker's, whose points stay
    // near the canvas).
    for (i, item) in items_of(5e37, 0.5).into_iter().enumerate() {
        assert!(
            coverage(&draw(&list(vec![item.clone()]), 120, 120)) > 0,
            "{i}"
        );
        let scaled = group(Transform::scale(2.0, 2.0), 1.0, None, vec![item]);
        assert_eq!(
            coverage(&draw(&list(vec![scaled]), 120, 120)),
            0,
            "{i} scaled"
        );
    }
}

#[test]
fn fill_rect_is_drawrect_with_fine_edges() {
    // fillRect is Skia's drawRect: AntiFillRect places edges to 1/256 of a
    // pixel, where a large path fill's analytic edges snap to quarter
    // pixels. A bottom edge at y = 26.4 covers 40% of row 26 as a
    // rectangle and 50% as a path.
    let rect = Rect::new(2.0, 2.0, 48.0, 24.4);
    let as_rect = draw(
        &list(vec![DisplayItem::FillRect {
            rect,
            color: Color::new("#000"),
        }]),
        60,
        30,
    );
    let as_path = draw(
        &list(vec![fill(
            Path::rect(rect.x, rect.y, rect.width, rect.height),
            "#000",
            FillRule::NonZero,
        )]),
        60,
        30,
    );
    assert_eq!(px(&as_rect, 10, 26).3, 102);
    assert_eq!(px(&as_path, 10, 26).3, 128);
    assert_eq!(px(&as_rect, 10, 10), (0, 0, 0, 255));
    // Rotated, it is the rectangle's path, as SkDraw::drawRect falls back
    // to drawPath when the matrix does not keep rectangles rectangles.
    let turned = |item: DisplayItem| {
        draw(
            &list(vec![DisplayItem::Group(Group {
                transform: Transform::rotate(0.3),
                opacity: 1.0,
                clip: None,
                items: vec![item],
            })]),
            60,
            40,
        )
    };
    let a = turned(DisplayItem::FillRect {
        rect,
        color: Color::new("#000"),
    });
    let b = turned(fill(
        Path::rect(rect.x, rect.y, rect.width, rect.height),
        "#000",
        FillRule::NonZero,
    ));
    assert_eq!(a.data(), b.data());
    // Nothing for an empty or non-finite rectangle.
    for r in [
        Rect::new(0.0, 0.0, 0.0, 10.0),
        Rect::new(0.0, 0.0, f64::NAN, 10.0),
        Rect::new(0.0, 0.0, f64::INFINITY, 10.0),
    ] {
        let p = draw(
            &list(vec![DisplayItem::FillRect {
                rect: r,
                color: Color::new("#000"),
            }]),
            10,
            10,
        );
        assert!(p.pixels().iter().all(|c| c.alpha() == 0), "{r:?}");
    }
}

#[test]
fn built_in_images_resolve_by_their_excalidraw_ids() {
    use excali_scene::display::{
        builtin_image, ELEMENT_LINK_ID, EXTERNAL_LINK_ID, IMAGE_ERROR_PLACEHOLDER_ID,
        IMAGE_PLACEHOLDER_ID,
    };
    // the ids the static scene names upstream's placeholders and link
    // icons by draw without the caller giving the backend any image
    let drawn = |id: &str| {
        let p = draw(
            &list(vec![DisplayItem::Image(ImageItem::new(
                id,
                Rect::new(0.0, 0.0, 32.0, 32.0),
            ))]),
            32,
            32,
        );
        p.pixels().iter().filter(|c| c.alpha() > 0).count()
    };
    for id in [
        IMAGE_PLACEHOLDER_ID,
        IMAGE_ERROR_PLACEHOLDER_ID,
        EXTERNAL_LINK_ID,
        ELEMENT_LINK_ID,
    ] {
        assert!(drawn(id) > 50, "{id} draws");
    }
    // the placeholder's icon is #888
    let p = draw(
        &list(vec![DisplayItem::Image(ImageItem::new(
            IMAGE_PLACEHOLDER_ID,
            Rect::new(0.0, 0.0, 64.0, 64.0),
        ))]),
        64,
        64,
    );
    assert!(near(px(&p, 4, 32), (0x88, 0x88, 0x88, 255)));
    // an id that is no built-in image and no file draws nothing, among
    // them the scheme the port no longer uses
    for id in [
        "builtin:image-placeholder",
        "excalidraw:unknown",
        "image-placeholder",
    ] {
        assert_eq!(drawn(id), 0, "{id}");
    }
    assert_eq!(
        builtin_image("image-placeholder").map(|b| b.id),
        Some(IMAGE_PLACEHOLDER_ID)
    );
}
