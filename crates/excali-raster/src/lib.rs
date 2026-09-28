//! Native PNG rendering of a display list.
//!
//! Upstream counterpart: `exportToCanvas` path of `scene/export.ts`.
//!
//! The backend knows the display list (`excali_scene::display`) and
//! tiny-skia, nothing about elements (ADR-008). [`render`] replays a list
//! into a [`Pixmap`] with the list's canvas semantics:
//!
//! - fills and strokes anti-aliased, source-over, in the fill rule, width,
//!   caps, joins, miter limit and dash the item gives;
//! - opacity as `globalAlpha`: multiplied into each draw's paint on its own;
//! - clips as anti-aliased coverage masks, intersected down the group
//!   stack;
//! - images from an [`ImageStore`] through a pattern shader, with the
//!   source rectangle clipped to the bitmap as `drawImage` does, bilinear
//!   or nearest sampling from the item's smoothing flag, and the
//!   dark-theme filter applied to the unpremultiplied pixels;
//! - text through the caller's [`TextRasterizer`], which gets the run with
//!   its resolved colour, matrix and clip (glyph outlines come from the
//!   font files, `excali-text`).
//!
//! Paths go through `Path::canonical`, so implicit subpath starts,
//! non-finite arguments and arcs follow the canvas rules.
//!
//! Targets: native. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-scene`.

use std::collections::HashMap;

use excali_scene::display::{
    Clip, DisplayList, FillRule, ImageItem, LineCap, LineJoin, PaintState, Painter, Path,
    PathCommand, Rect, Rgba, Stroke, TextRun, Transform,
};
pub use tiny_skia;
use tiny_skia::{FilterQuality, Mask, Paint, Pixmap, PixmapRef, SpreadMode};

/// Where the backend finds the bitmaps the display list names: premultiplied
/// RGBA pixmaps by id (upstream's `imageCache`, keyed by `fileId`).
pub trait ImageStore {
    /// The image `id`, or `None` when there is none (nothing is drawn).
    fn image(&self, id: &str) -> Option<PixmapRef<'_>>;
}

impl ImageStore for HashMap<String, Pixmap> {
    fn image(&self, id: &str) -> Option<PixmapRef<'_>> {
        self.get(id).map(Pixmap::as_ref)
    }
}

/// Draws text runs: `fillText` for the raster backend. tiny-skia has no
/// text, so glyph shaping and outlines come from the caller (the font
/// pipeline in `excali-text`).
pub trait TextRasterizer {
    /// Paint `run` into `target` in `color` (its alpha already multiplied
    /// by the draw's alpha), under `transform`, inside `clip` when set.
    fn fill_text(
        &mut self,
        target: &mut Pixmap,
        run: &TextRun,
        color: tiny_skia::Color,
        transform: tiny_skia::Transform,
        clip: Option<&Mask>,
    );
}

/// Paint `list` into `pixmap` from a fresh state (identity matrix, alpha 1).
pub fn render<I: ImageStore, T: TextRasterizer>(
    list: &DisplayList,
    pixmap: &mut Pixmap,
    images: &I,
    text: &mut T,
) {
    list.replay(&mut RasterPainter::new(pixmap, images, text));
}

/// Paint `list` into `pixmap` scaled by `scale` (the device pixel ratio, or
/// the export scale), as `bootstrapCanvas` scales before drawing
/// (`renderer/helpers.ts:73-127`).
pub fn render_scaled<I: ImageStore, T: TextRasterizer>(
    list: &DisplayList,
    pixmap: &mut Pixmap,
    scale: f64,
    images: &I,
    text: &mut T,
) {
    let base = PaintState {
        transform: Transform::scale(scale, scale),
        alpha: 1.0,
    };
    list.replay_from(&mut RasterPainter::new(pixmap, images, text), base);
}

struct RasterPainter<'a, I, T> {
    pixmap: &'a mut Pixmap,
    images: &'a I,
    text: &'a mut T,
    /// The intersected clip of every pushed clip, innermost last.
    clips: Vec<Mask>,
}

impl<'a, I: ImageStore, T: TextRasterizer> RasterPainter<'a, I, T> {
    fn new(pixmap: &'a mut Pixmap, images: &'a I, text: &'a mut T) -> Self {
        Self {
            pixmap,
            images,
            text,
            clips: Vec::new(),
        }
    }

    fn clip(&self) -> Option<&Mask> {
        self.clips.last()
    }
}

fn transform(t: &Transform) -> tiny_skia::Transform {
    tiny_skia::Transform::from_row(
        t.a as f32, t.b as f32, t.c as f32, t.d as f32, t.e as f32, t.f as f32,
    )
}

fn fill_rule(rule: FillRule) -> tiny_skia::FillRule {
    match rule {
        FillRule::NonZero => tiny_skia::FillRule::Winding,
        FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
    }
}

/// The colour with the draw's alpha multiplied in, or `None` when nothing
/// would show.
fn color(c: Rgba, alpha: f64) -> Option<tiny_skia::Color> {
    let a = (c.a * alpha).clamp(0.0, 1.0);
    if a == 0.0 {
        return None;
    }
    tiny_skia::Color::from_rgba(
        f32::from(c.r) / 255.0,
        f32::from(c.g) / 255.0,
        f32::from(c.b) / 255.0,
        a as f32,
    )
}

fn solid_paint(c: tiny_skia::Color) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(c);
    paint.anti_alias = true;
    paint
}

/// The canonical path as a tiny-skia path, or `None` when it has no
/// geometry.
fn skia_path(path: &Path) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for command in path.canonical().commands {
        match command {
            PathCommand::MoveTo(x, y) => pb.move_to(x as f32, y as f32),
            PathCommand::LineTo(x, y) => pb.line_to(x as f32, y as f32),
            PathCommand::QuadTo(cx, cy, x, y) => {
                pb.quad_to(cx as f32, cy as f32, x as f32, y as f32)
            }
            PathCommand::CubicTo(a, b, c, d, x, y) => {
                pb.cubic_to(a as f32, b as f32, c as f32, d as f32, x as f32, y as f32)
            }
            PathCommand::Close => pb.close(),
            PathCommand::Arc { .. } => unreachable!("canonical paths have no arcs"),
        }
    }
    pb.finish()
}

fn skia_stroke(stroke: &Stroke) -> tiny_skia::Stroke {
    tiny_skia::Stroke {
        width: stroke.effective_width() as f32,
        miter_limit: stroke.effective_miter_limit() as f32,
        line_cap: match stroke.cap {
            LineCap::Butt => tiny_skia::LineCap::Butt,
            LineCap::Round => tiny_skia::LineCap::Round,
            LineCap::Square => tiny_skia::LineCap::Square,
        },
        line_join: match stroke.join {
            LineJoin::Miter => tiny_skia::LineJoin::Miter,
            LineJoin::Round => tiny_skia::LineJoin::Round,
            LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
        },
        dash: stroke.dash.as_ref().and_then(|d| {
            tiny_skia::StrokeDash::new(
                d.segments().iter().map(|&s| s as f32).collect(),
                d.offset() as f32,
            )
        }),
    }
}

/// `drawImage`'s rectangles after its clipping step: the source rectangle
/// clipped to the bitmap, and the destination clipped in proportion. `None`
/// when nothing is left.
fn clip_image_rects(source: Rect, dest: Rect, width: f64, height: f64) -> Option<(Rect, Rect)> {
    let s = source.normalized();
    let d = dest.normalized();
    if s.is_empty() || d.is_empty() {
        return None;
    }
    let x0 = s.x.max(0.0);
    let y0 = s.y.max(0.0);
    let x1 = (s.x + s.width).min(width);
    let y1 = (s.y + s.height).min(height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let sx = d.width / s.width;
    let sy = d.height / s.height;
    let clipped_source = Rect::new(x0, y0, x1 - x0, y1 - y0);
    let clipped_dest = Rect::new(
        d.x + (x0 - s.x) * sx,
        d.y + (y0 - s.y) * sy,
        (x1 - x0) * sx,
        (y1 - y0) * sy,
    );
    Some((clipped_source, clipped_dest))
}

impl<I: ImageStore, T: TextRasterizer> Painter for RasterPainter<'_, I, T> {
    fn fill(&mut self, path: &Path, c: Rgba, rule: FillRule, state: &PaintState) {
        let (Some(c), Some(path)) = (color(c, state.alpha), skia_path(path)) else {
            return;
        };
        let clip = self.clips.last();
        self.pixmap.fill_path(
            &path,
            &solid_paint(c),
            fill_rule(rule),
            transform(&state.transform),
            clip,
        );
    }

    fn stroke(&mut self, path: &Path, stroke: &Stroke, c: Rgba, state: &PaintState) {
        let (Some(c), Some(path)) = (color(c, state.alpha), skia_path(path)) else {
            return;
        };
        let clip = self.clips.last();
        self.pixmap.stroke_path(
            &path,
            &solid_paint(c),
            &skia_stroke(stroke),
            transform(&state.transform),
            clip,
        );
    }

    fn image(&mut self, image: &ImageItem, state: &PaintState) {
        let Some(bitmap) = self.images.image(&image.id) else {
            return;
        };
        let (w, h) = (f64::from(bitmap.width()), f64::from(bitmap.height()));
        let source = image.source.unwrap_or(Rect::new(0.0, 0.0, w, h));
        let Some((source, dest)) = clip_image_rects(source, image.dest, w, h) else {
            return;
        };
        let alpha = state.alpha.clamp(0.0, 1.0);
        if alpha == 0.0 {
            return;
        }
        let filtered;
        let bitmap = match image.filter {
            Some(filter) => {
                let mut copy = bitmap.to_owned();
                for px in copy.pixels_mut() {
                    let c = px.demultiply();
                    let (r, g, b) = filter.apply_rgb(c.red(), c.green(), c.blue());
                    *px = tiny_skia::ColorU8::from_rgba(r, g, b, c.alpha()).premultiply();
                }
                filtered = copy;
                filtered.as_ref()
            }
            None => bitmap,
        };
        // Image pixels to user space: the source rectangle onto the
        // destination.
        let sx = dest.width / source.width;
        let sy = dest.height / source.height;
        let pattern_transform = tiny_skia::Transform::from_row(
            sx as f32,
            0.0,
            0.0,
            sy as f32,
            (dest.x - source.x * sx) as f32,
            (dest.y - source.y * sy) as f32,
        );
        let quality = if image.smoothing {
            FilterQuality::Bilinear
        } else {
            FilterQuality::Nearest
        };
        let paint = Paint {
            shader: tiny_skia::Pattern::new(
                bitmap,
                SpreadMode::Pad,
                quality,
                alpha as f32,
                pattern_transform,
            ),
            anti_alias: true,
            ..Paint::default()
        };
        let Some(rect) = tiny_skia::Rect::from_xywh(
            dest.x as f32,
            dest.y as f32,
            dest.width as f32,
            dest.height as f32,
        ) else {
            return;
        };
        let clip = self.clips.last();
        self.pixmap
            .fill_rect(rect, &paint, transform(&state.transform), clip);
    }

    fn text(&mut self, run: &TextRun, c: Rgba, state: &PaintState) {
        let Some(c) = color(c, state.alpha) else {
            return;
        };
        let clip = self.clips.last();
        self.text
            .fill_text(self.pixmap, run, c, transform(&state.transform), clip);
    }

    fn push_clip(&mut self, clip: &Clip, t: &Transform) {
        let mut mask = match self.clip() {
            Some(current) => current.clone(),
            None => {
                let mut full = Mask::new(self.pixmap.width(), self.pixmap.height())
                    .expect("the pixmap's size is a valid mask size");
                full.data_mut().fill(255);
                full
            }
        };
        match skia_path(&clip.path) {
            Some(path) => mask.intersect_path(&path, fill_rule(clip.rule), true, transform(t)),
            // An empty clip path clips everything away.
            None => mask.clear(),
        }
        self.clips.push(mask);
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
    }
}
