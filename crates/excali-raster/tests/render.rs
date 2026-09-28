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

/// ADR-008: backends know nothing about elements. This crate's only
/// workspace dependency is `excali-scene`, it reaches into that crate only
/// through `excali_scene::display`, and no identifier in its code names an
/// element (`HtmlImageElement`, the browser's image type, aside).
#[test]
fn no_element_knowledge() {
    let manifest = include_str!("../Cargo.toml");
    let internal: Vec<&str> = manifest
        .lines()
        .filter_map(|l| l.trim().strip_prefix("excali-"))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(internal, ["scene"], "internal dependencies: {internal:?}");
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
