//! SVG files drawn with `drawImage` (ex-404). Chrome draws an SVG image as
//! vector content at the destination's resolution, with the source
//! rectangle (in the SVG's CSS pixels, `element.crop`) mapped onto the
//! destination and everything outside the destination clipped away; the
//! canvas's clip, `globalAlpha` and `filter` apply as to a bitmap. The
//! backend renders the tree with resvg under the draw's matrix instead of
//! scaling a bitmap of it.

use excali_raster::decode::{DecodedImage, ImageFiles};
use excali_raster::tiny_skia::{self, Mask, Pixmap};
use excali_raster::{render, Image, ImageStore, TextRasterizer};
use excali_scene::display::{
    Clip, DisplayItem, DisplayList, FillRule, Group, ImageFilter, ImageItem, Path, Rect, TextRun,
    Transform,
};

struct NoText;

impl TextRasterizer for NoText {
    fn fill_text(
        &mut self,
        _: &mut Pixmap,
        _: &TextRun,
        _: tiny_skia::Color,
        _: tiny_skia::Transform,
        _: Option<&Mask>,
    ) {
        panic!("no text here");
    }
}

/// A 10x10 SVG: its left half red, its right half blue.
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="5" height="10" fill="#ff0000"/><rect x="5" width="5" height="10" fill="#0000ff"/></svg>"##;

fn files() -> ImageFiles {
    let url = format!("data:image/svg+xml,{}", SVG.replace('#', "%23"));
    let files = ImageFiles::decode([("svg", "image/svg+xml", url.as_str())]);
    assert!(matches!(files.image("svg"), Some(Image::Svg(_))));
    files
}

fn draw(items: Vec<DisplayItem>, w: u32, h: u32) -> Pixmap {
    let mut pixmap = Pixmap::new(w, h).unwrap();
    render(
        &items.into_iter().collect::<DisplayList>(),
        &mut pixmap,
        &files(),
        &mut NoText,
    );
    pixmap
}

fn px(p: &Pixmap, x: u32, y: u32) -> (u8, u8, u8, u8) {
    let c = p.pixel(x, y).unwrap();
    (c.red(), c.green(), c.blue(), c.alpha())
}

const RED: (u8, u8, u8, u8) = (255, 0, 0, 255);
const BLUE: (u8, u8, u8, u8) = (0, 0, 255, 255);
const CLEAR: (u8, u8, u8, u8) = (0, 0, 0, 0);

#[test]
fn scaled_up_svg_stays_sharp() {
    // 10x10 into 80x80: the colour edge at x = 40 is a hard edge, where a
    // scaled bitmap would blend red and blue across the pixels around it.
    let p = draw(
        vec![DisplayItem::Image(ImageItem::new(
            "svg",
            Rect::new(0.0, 0.0, 80.0, 80.0),
        ))],
        80,
        80,
    );
    assert_eq!(px(&p, 39, 40), RED);
    assert_eq!(px(&p, 40, 40), BLUE);
    assert_eq!(px(&p, 0, 0), RED);
    assert_eq!(px(&p, 79, 79), BLUE);
}

#[test]
fn the_source_rectangle_crops_the_svg() {
    // The right half only, into 20x20.
    let mut item = ImageItem::new("svg", Rect::new(0.0, 0.0, 20.0, 20.0));
    item.source = Some(Rect::new(5.0, 0.0, 5.0, 10.0));
    let p = draw(vec![DisplayItem::Image(item)], 24, 24);
    assert_eq!(px(&p, 0, 10), BLUE);
    assert_eq!(px(&p, 19, 19), BLUE);
    assert_eq!(px(&p, 21, 10), CLEAR);
}

#[test]
fn the_svg_is_clipped_to_the_destination() {
    // A source running past the image is clipped with the destination, as
    // for bitmaps; nothing of the SVG is drawn outside it.
    let mut item = ImageItem::new("svg", Rect::new(0.0, 0.0, 40.0, 20.0));
    item.source = Some(Rect::new(0.0, 0.0, 20.0, 10.0));
    let p = draw(vec![DisplayItem::Image(item)], 40, 20);
    assert_eq!(px(&p, 5, 5), RED);
    assert_eq!(px(&p, 15, 5), BLUE);
    assert_eq!(px(&p, 25, 5), CLEAR);
}

#[test]
fn transforms_alpha_clips_and_the_filter_apply() {
    // Flipped horizontally (scale(-1, 1) about the centre): blue on the left.
    let flipped = DisplayItem::Group(Group {
        transform: Transform::new(-1.0, 0.0, 0.0, 1.0, 20.0, 0.0),
        opacity: 1.0,
        clip: None,
        items: vec![DisplayItem::Image(ImageItem::new(
            "svg",
            Rect::new(0.0, 0.0, 20.0, 20.0),
        ))],
    });
    let p = draw(vec![flipped], 20, 20);
    assert_eq!(px(&p, 2, 10), BLUE);
    assert_eq!(px(&p, 17, 10), RED);

    // Half alpha, inside a clip to the top half.
    let clipped = DisplayItem::Group(Group {
        transform: Transform::IDENTITY,
        opacity: 0.5,
        clip: Some(Clip {
            path: Path::rect(0.0, 0.0, 20.0, 10.0),
            rule: FillRule::NonZero,
        }),
        items: vec![DisplayItem::Image(ImageItem::new(
            "svg",
            Rect::new(0.0, 0.0, 20.0, 20.0),
        ))],
    });
    let p = draw(vec![clipped], 20, 20);
    assert_eq!(px(&p, 2, 5), (128, 0, 0, 128));
    assert_eq!(px(&p, 2, 15), CLEAR);

    // DARK_THEME_FILTER on the unpremultiplied pixels.
    let mut dark = ImageItem::new("svg", Rect::new(0.0, 0.0, 20.0, 20.0));
    dark.filter = Some(ImageFilter::DarkTheme);
    let p = draw(vec![DisplayItem::Image(dark)], 20, 20);
    let (r, g, b) = ImageFilter::DarkTheme.apply_rgb(255, 0, 0);
    assert_eq!(px(&p, 2, 10), (r, g, b, 255));
}

#[test]
fn later_shapes_paint_over_earlier_ones() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><circle cx="5" cy="5" r="4" fill="#ff0000" fill-opacity="0.8"/><path d="M0 5 L10 5" stroke="#00ff00" stroke-width="2"/></svg>"##;
    let url = format!("data:image/svg+xml,{}", svg.replace('#', "%23"));
    let files = ImageFiles::decode([("svg", "image/svg+xml", url.as_str())]);
    let mut pixmap = Pixmap::new(20, 20).unwrap();
    let list: DisplayList = [DisplayItem::Image(ImageItem::new(
        "svg",
        Rect::new(0.0, 0.0, 20.0, 20.0),
    ))]
    .into_iter()
    .collect();
    render(&list, &mut pixmap, &files, &mut NoText);
    // The stroke along y = 10 is on top of the circle.
    assert_eq!(px(&pixmap, 10, 10), (0, 255, 0, 255));
    // Elsewhere the circle at 80%.
    assert_eq!(px(&pixmap, 10, 5), (204, 0, 0, 204));
}

#[test]
fn decoded_images_are_the_store_entries() {
    let files = files();
    let Some(Image::Svg(tree)) = files.image("svg") else {
        panic!("an SVG")
    };
    assert_eq!(tree.size().width(), 10.0);
    // DecodedImage is what the store holds.
    let decoded = excali_raster::decode::decode_data_url(&format!(
        "data:image/svg+xml,{}",
        SVG.replace('#', "%23")
    ))
    .unwrap();
    assert!(matches!(decoded, DecodedImage::Svg(_)));
}
