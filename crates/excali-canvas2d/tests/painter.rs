//! The Canvas 2D backend consumes the display list (ex-216): every item
//! becomes the `CanvasRenderingContext2D` calls upstream makes for it
//! (`packages/element/src/renderElement.ts`), each item isolated in
//! `save()`/`restore()` with its absolute transform and alpha set first, and
//! clips pushed as `save(); …; clip(rule)` and popped with `restore()`.
//!
//! A recording [`Context2d`] stands in for the browser; the `web-sys`
//! implementation forwards the same calls one to one.

use std::collections::HashMap;
use std::f64::consts::FRAC_PI_2;

use excali_canvas2d::{paint, Context2d};
use excali_scene::display::{
    Clip, Color, Dash, Direction, DisplayItem, DisplayList, FillRule, Font, Group, ImageFilter,
    ImageItem, LineCap, LineJoin, Path, Rect, Stroke, TextAlign, TextRun, Transform,
};

#[derive(Default)]
struct Recording {
    log: Vec<String>,
    images: HashMap<String, (f64, f64)>,
}

fn n(x: f64) -> String {
    format!("{x}")
}

impl Context2d for Recording {
    fn save(&mut self) {
        self.log.push("save".into());
    }
    fn restore(&mut self) {
        self.log.push("restore".into());
    }
    fn set_transform(&mut self, t: &Transform) {
        self.log.push(format!(
            "setTransform({},{},{},{},{},{})",
            n(t.a),
            n(t.b),
            n(t.c),
            n(t.d),
            n(t.e),
            n(t.f)
        ));
    }
    fn set_global_alpha(&mut self, alpha: f64) {
        self.log.push(format!("globalAlpha={}", n(alpha)));
    }
    fn set_fill_style(&mut self, css: &str) {
        self.log.push(format!("fillStyle={css}"));
    }
    fn set_stroke_style(&mut self, css: &str) {
        self.log.push(format!("strokeStyle={css}"));
    }
    fn set_line_width(&mut self, width: f64) {
        self.log.push(format!("lineWidth={}", n(width)));
    }
    fn set_line_cap(&mut self, cap: &str) {
        self.log.push(format!("lineCap={cap}"));
    }
    fn set_line_join(&mut self, join: &str) {
        self.log.push(format!("lineJoin={join}"));
    }
    fn set_miter_limit(&mut self, limit: f64) {
        self.log.push(format!("miterLimit={}", n(limit)));
    }
    fn set_line_dash(&mut self, segments: &[f64]) {
        let s: Vec<String> = segments.iter().map(|&x| n(x)).collect();
        self.log.push(format!("setLineDash([{}])", s.join(",")));
    }
    fn set_line_dash_offset(&mut self, offset: f64) {
        self.log.push(format!("lineDashOffset={}", n(offset)));
    }
    fn begin_path(&mut self) {
        self.log.push("beginPath".into());
    }
    fn move_to(&mut self, x: f64, y: f64) {
        self.log.push(format!("moveTo({},{})", n(x), n(y)));
    }
    fn line_to(&mut self, x: f64, y: f64) {
        self.log.push(format!("lineTo({},{})", n(x), n(y)));
    }
    fn quadratic_curve_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        self.log.push(format!(
            "quadraticCurveTo({},{},{},{})",
            n(cx),
            n(cy),
            n(x),
            n(y)
        ));
    }
    fn bezier_curve_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64) {
        self.log.push(format!(
            "bezierCurveTo({},{},{},{},{},{})",
            n(c1x),
            n(c1y),
            n(c2x),
            n(c2y),
            n(x),
            n(y)
        ));
    }
    fn arc(&mut self, cx: f64, cy: f64, radius: f64, start: f64, end: f64, anticlockwise: bool) {
        self.log.push(format!(
            "arc({},{},{},{},{},{anticlockwise})",
            n(cx),
            n(cy),
            n(radius),
            n(start),
            n(end)
        ));
    }
    fn close_path(&mut self) {
        self.log.push("closePath".into());
    }
    fn fill(&mut self, rule: &str) {
        self.log.push(format!("fill({rule})"));
    }
    fn stroke(&mut self) {
        self.log.push("stroke".into());
    }
    fn clip(&mut self, rule: &str) {
        self.log.push(format!("clip({rule})"));
    }
    fn set_font(&mut self, css: &str) {
        self.log.push(format!("font={css}"));
    }
    fn set_text_align(&mut self, align: &str) {
        self.log.push(format!("textAlign={align}"));
    }
    fn set_direction(&mut self, direction: &str) {
        self.log.push(format!("direction={direction}"));
    }
    fn fill_text(&mut self, text: &str, x: f64, y: f64) {
        self.log.push(format!("fillText({text},{},{})", n(x), n(y)));
    }
    fn set_image_smoothing_enabled(&mut self, enabled: bool) {
        self.log.push(format!("imageSmoothingEnabled={enabled}"));
    }
    fn set_filter(&mut self, css: &str) {
        self.log.push(format!("filter={css}"));
    }
    fn image_size(&self, id: &str) -> Option<(f64, f64)> {
        self.images.get(id).copied()
    }
    fn draw_image(&mut self, id: &str, source: &Rect, dest: &Rect) {
        self.log.push(format!(
            "drawImage({id},{},{},{},{},{},{},{},{})",
            n(source.x),
            n(source.y),
            n(source.width),
            n(source.height),
            n(dest.x),
            n(dest.y),
            n(dest.width),
            n(dest.height)
        ));
    }
}

fn run(list: &DisplayList) -> Vec<String> {
    let mut ctx = Recording::default();
    ctx.images.insert("img".into(), (640.0, 480.0));
    paint(list, &mut ctx);
    ctx.log
}

fn one(item: DisplayItem) -> Vec<String> {
    let mut list = DisplayList::new();
    list.push(item);
    run(&list)
}

const ID: &str = "setTransform(1,0,0,1,0,0)";

#[test]
fn fill_is_one_isolated_fill_call() {
    let log = one(DisplayItem::Fill {
        path: Path::rect(0.0, 0.0, 10.0, 20.0),
        color: Color::new("#1e1e1e"),
        rule: FillRule::EvenOdd,
    });
    assert_eq!(
        log,
        [
            "save",
            ID,
            "globalAlpha=1",
            "fillStyle=rgba(30, 30, 30, 1)",
            "beginPath",
            "moveTo(0,0)",
            "lineTo(10,0)",
            "lineTo(10,20)",
            "lineTo(0,20)",
            "closePath",
            "moveTo(0,0)",
            "fill(evenodd)",
            "restore",
        ]
    );
}

#[test]
fn stroke_sets_every_line_property() {
    // renderElement.ts:473-495 sets lineJoin/lineCap "round" before rc.draw;
    // roughjs sets strokeStyle, lineWidth and the dash (bin/canvas.js).
    let mut path = Path::new();
    path.move_to(0.0, 0.0)
        .quad_to(1.0, 2.0, 3.0, 4.0)
        .cubic_to(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)
        .arc(5.0, 5.0, 2.0, 0.0, FRAC_PI_2, true);
    let stroke = Stroke::new(Color::new("#e03131"), 2.5)
        .with_cap(LineCap::Round)
        .with_join(LineJoin::Round)
        .with_dash(Dash::new(&[8.0, 10.5], 2.0));
    let log = one(DisplayItem::Stroke { path, stroke });
    assert_eq!(
        log,
        [
            "save",
            ID,
            "globalAlpha=1",
            "strokeStyle=rgba(224, 49, 49, 1)",
            "lineWidth=2.5",
            "lineCap=round",
            "lineJoin=round",
            "miterLimit=10",
            "setLineDash([8,10.5])",
            "lineDashOffset=2",
            "beginPath",
            "moveTo(0,0)",
            "quadraticCurveTo(1,2,3,4)",
            "bezierCurveTo(1,2,3,4,5,6)",
            &format!("arc(5,5,2,0,{},true)", FRAC_PI_2),
            "stroke",
            "restore",
        ]
    );
}

#[test]
fn stroke_uses_the_effective_width_and_no_dash_when_solid() {
    let log = one(DisplayItem::Stroke {
        path: Path::rect(0.0, 0.0, 1.0, 1.0),
        stroke: Stroke::new(Color::new("#000"), 0.0),
    });
    assert!(log.contains(&"lineWidth=1".to_string()));
    assert!(log.contains(&"lineCap=butt".to_string()));
    assert!(log.contains(&"lineJoin=miter".to_string()));
    assert!(!log.iter().any(|c| c.starts_with("setLineDash")));
    assert!(!log.iter().any(|c| c.starts_with("lineDashOffset")));
}

#[test]
fn image_draws_the_source_rect_into_the_destination() {
    // renderElement.ts:552-620: drawImage(img, crop or natural size, 0, 0, w, h);
    // DARK_THEME_FILTER for SVG images in dark mode.
    let mut img = ImageItem::new("img", Rect::new(0.0, 0.0, 100.0, 75.0));
    img.source = Some(Rect::new(10.0, 20.0, 300.0, 200.0));
    img.smoothing = false;
    img.filter = Some(ImageFilter::DarkTheme);
    let log = one(DisplayItem::Image(img));
    assert_eq!(
        log,
        [
            "save",
            ID,
            "globalAlpha=1",
            "imageSmoothingEnabled=false",
            "filter=invert(93%) hue-rotate(180deg)",
            "drawImage(img,10,20,300,200,0,0,100,75)",
            "restore",
        ]
    );
}

#[test]
fn image_without_crop_uses_the_natural_size() {
    let log = one(DisplayItem::Image(ImageItem::new(
        "img",
        Rect::new(1.0, 2.0, 64.0, 48.0),
    )));
    assert_eq!(
        log,
        [
            "save",
            ID,
            "globalAlpha=1",
            "imageSmoothingEnabled=true",
            "drawImage(img,0,0,640,480,1,2,64,48)",
            "restore",
        ]
    );
}

#[test]
fn image_not_loaded_draws_nothing() {
    // The scene draws upstream's placeholder (renderElement.ts:361-385) for
    // an image that is not ready; a backend given an unknown id skips it.
    let log = one(DisplayItem::Image(ImageItem::new(
        "missing",
        Rect::new(0.0, 0.0, 1.0, 1.0),
    )));
    assert!(log.is_empty(), "{log:?}");
}

#[test]
fn text_is_fill_text_with_font_align_and_direction() {
    // renderElement.ts:626-674.
    let mut run = TextRun::new(
        "مرحبا",
        50.0,
        17.5,
        Font::new(20.0, "Excalifont, Xiaolai, Segoe UI Emoji"),
        Color::new("#1e1e1e"),
    );
    run.align = TextAlign::Center;
    run.direction = Direction::Rtl;
    let log = one(DisplayItem::Text(run));
    assert_eq!(
        log,
        [
            "save",
            ID,
            "globalAlpha=1",
            "font=20px Excalifont, Xiaolai, Segoe UI Emoji",
            "fillStyle=rgba(30, 30, 30, 1)",
            "textAlign=center",
            "direction=rtl",
            "fillText(مرحبا,50,17.5)",
            "restore",
        ]
    );
}

#[test]
fn groups_set_absolute_state_and_clips_nest() {
    let mut list = DisplayList::new();
    list.push(DisplayItem::Group(Group {
        transform: Transform::translate(10.0, 20.0),
        opacity: 0.5,
        clip: Some(Clip {
            path: Path::rect(0.0, 0.0, 5.0, 5.0),
            rule: FillRule::EvenOdd,
        }),
        items: vec![DisplayItem::Fill {
            path: Path::rect(0.0, 0.0, 1.0, 1.0),
            color: Color::new("rgba(0, 0, 200, 0.04)"),
            rule: FillRule::NonZero,
        }],
    }));
    assert_eq!(
        run(&list),
        [
            "save",
            "setTransform(1,0,0,1,10,20)",
            "beginPath",
            "moveTo(0,0)",
            "lineTo(5,0)",
            "lineTo(5,5)",
            "lineTo(0,5)",
            "closePath",
            "moveTo(0,0)",
            "clip(evenodd)",
            "save",
            "setTransform(1,0,0,1,10,20)",
            "globalAlpha=0.5",
            "fillStyle=rgba(0, 0, 200, 0.04)",
            "beginPath",
            "moveTo(0,0)",
            "lineTo(1,0)",
            "lineTo(1,1)",
            "lineTo(0,1)",
            "closePath",
            "moveTo(0,0)",
            "fill(nonzero)",
            "restore",
            "restore",
        ]
    );
}

#[test]
fn paint_from_a_device_pixel_ratio() {
    // bootstrapCanvas: setTransform(1,0,0,1,0,0) then scale(dpr).
    let mut list = DisplayList::new();
    list.push(DisplayItem::Fill {
        path: Path::rect(0.0, 0.0, 1.0, 1.0),
        color: Color::new("#000"),
        rule: FillRule::NonZero,
    });
    let mut ctx = Recording::default();
    excali_canvas2d::paint_scaled(&list, &mut ctx, 2.0);
    assert_eq!(ctx.log[1], "setTransform(2,0,0,2,0,0)");
}

/// ADR-008: backends know nothing about elements. This crate's only
/// workspace dependency is `excali-scene`, and its sources name nothing
/// from the element model.
#[test]
fn no_element_knowledge() {
    let manifest = include_str!("../Cargo.toml");
    let internal: Vec<&str> = manifest
        .lines()
        .filter_map(|l| l.trim().strip_prefix("excali-"))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(internal, ["scene"], "internal dependencies: {internal:?}");
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for forbidden in [
            "excali_core",
            "excali_rough",
            "excali_freehand",
            "excali_text",
            "excali_scene::rough",
            "excali_scene::utils",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} mentions {forbidden}",
                path.display()
            );
        }
    }
}
