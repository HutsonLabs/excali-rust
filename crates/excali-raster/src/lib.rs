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
//! # Chrome's pixels
//!
//! Upstream draws with Chrome's canvas, so the backend reproduces the
//! geometry and coverage Chrome's Skia computes, and uses tiny-skia to
//! composite (the raster pipeline, shaders and the anti-aliased hairline):
//!
//! - `edges.rs`: the path as Blink builds it (`arc()` as conics, a lone
//!   filled circle as `drawArc`'s oval) and the curve geometry of Skia's
//!   edge builder, in `f32`;
//! - `aaa.rs`: Skia's analytic anti-aliasing (`SkScan_AAAPath`,
//!   `SkAnalyticEdge`, the edge clipper, convexity, `blitFatAntiRect`,
//!   `AntiFillRect`) producing the coverage mask of each fill, clip and
//!   image rectangle;
//! - `stroke.rs` and `dash.rs`: Skia's stroker and dasher, which keep round
//!   caps, joins and arcs as conics;
//! - `diff.rs`: the pixel comparison the fixtures use.
//!
//! `tests/fixtures/` holds display lists with the PNGs headless Chrome
//! paints for them and a tolerance each (see its README); no fixture pixel
//! is more than 5 levels from Chrome.
//!
//! Targets: native. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-scene`.

use std::collections::HashMap;

mod aaa;
mod dash;
pub mod diff;
mod edges;
mod stroke;

use excali_scene::display::{
    Clip, Color, DisplayList, FillRule, ImageItem, LineCap, LineJoin, PaintState, Painter, Path,
    Rect, Rgba, Stroke, TextRun, Transform,
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
    let base = PaintState::new(Transform::scale(scale, scale), 1.0);
    list.replay_from(&mut RasterPainter::new(pixmap, images, text), base);
}

/// Paint `list` into `pixmap` from `base`: its matrix, alpha and the
/// current styles a draw whose colour the canvas would ignore paints in.
pub fn render_from<I: ImageStore, T: TextRasterizer>(
    list: &DisplayList,
    pixmap: &mut Pixmap,
    base: PaintState,
    images: &I,
    text: &mut T,
) {
    list.replay_from(&mut RasterPainter::new(pixmap, images, text), base);
}

struct RasterPainter<'a, I, T> {
    pixmap: &'a mut Pixmap,
    images: &'a I,
    text: &'a mut T,
    /// The intersected clip of every pushed clip, innermost last.
    clips: Vec<ClipState>,
    /// A pixmap-sized mask, all zero between draws: one draw's coverage
    /// times the clip while it is painted.
    scratch: Option<Mask>,
}

/// A clip as Chrome's `SkRasterClip` holds it: coverage over the pixmap,
/// its bounds, and whether it is anti-aliased (an `SkAAClip`) or a
/// whole-pixel region.
struct ClipState {
    mask: Mask,
    bounds: aaa::IRect,
    anti_aliased: bool,
}

impl<'a, I: ImageStore, T: TextRasterizer> RasterPainter<'a, I, T> {
    fn new(pixmap: &'a mut Pixmap, images: &'a I, text: &'a mut T) -> Self {
        Self {
            pixmap,
            images,
            text,
            clips: Vec::new(),
            scratch: None,
        }
    }

    fn clip(&self) -> Option<&Mask> {
        self.clips.last().map(|c| &c.mask)
    }

    fn canvas(&self) -> aaa::IRect {
        aaa::IRect {
            left: 0,
            top: 0,
            right: self.pixmap.width() as i32,
            bottom: self.pixmap.height() as i32,
        }
    }

    /// The bounds draws are clipped to and whether they go through Skia's
    /// anti-aliased clip blitter (which forces the run-length blitters).
    fn clip_bounds(&self) -> Option<(aaa::IRect, bool)> {
        match self.clips.last() {
            None => Some((self.canvas(), false)),
            Some(c) if c.bounds.left >= c.bounds.right => None,
            Some(c) => Some((c.bounds, c.anti_aliased)),
        }
    }

    /// `computeConservativeLocalClipBounds`: the clip's bounds outset by a
    /// pixel, mapped into the draw's coordinates, for the dasher's cull.
    fn local_cull(&self, t: &Transform) -> Option<[f32; 4]> {
        let (b, _) = self.clip_bounds()?;
        let det = t.a * t.d - t.b * t.c;
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let inv = Transform::new(
            t.d / det,
            -t.b / det,
            -t.c / det,
            t.a / det,
            (t.c * t.f - t.d * t.e) / det,
            (t.b * t.e - t.a * t.f) / det,
        );
        let corners = [
            (f64::from(b.left - 1), f64::from(b.top - 1)),
            (f64::from(b.right + 1), f64::from(b.top - 1)),
            (f64::from(b.right + 1), f64::from(b.bottom + 1)),
            (f64::from(b.left - 1), f64::from(b.bottom + 1)),
        ]
        .map(|(x, y)| inv.apply(x, y));
        let xs = corners.map(|c| c.0 as f32);
        let ys = corners.map(|c| c.1 as f32);
        let min = |v: [f32; 4]| v.iter().copied().fold(f32::INFINITY, f32::min);
        let max = |v: [f32; 4]| v.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        Some([min(xs), min(ys), max(xs), max(ys)])
    }

    fn empty_mask(&self) -> Mask {
        Mask::new(self.pixmap.width(), self.pixmap.height())
            .expect("the pixmap's size is a valid mask size")
    }

    /// Paint `paint` through `fill`'s coverage, inside the current clip.
    fn paint_fill(&mut self, fill: &aaa::Fill, paint: &Paint<'_>) {
        let w = self.pixmap.width();
        let mut mask = match self.scratch.take() {
            Some(mask) => mask,
            None => self.empty_mask(),
        };
        write_coverage(&mut mask, fill, self.clip(), w);
        let r = fill.rect;
        let rect = tiny_skia::Rect::from_ltrb(
            r.left as f32,
            r.top as f32,
            r.right as f32,
            r.bottom as f32,
        )
        .expect("a fill covers a non-empty rectangle");
        let mut paint = paint.clone();
        paint.anti_alias = false;
        self.pixmap
            .fill_rect(rect, &paint, tiny_skia::Transform::identity(), Some(&mask));
        let data = mask.data_mut();
        for y in r.top..r.bottom {
            let start = (y as u32 * w + r.left as u32) as usize;
            data[start..start + (r.right - r.left) as usize].fill(0);
        }
        self.scratch = Some(mask);
    }

    /// Fill the device-space path `segs` under `rule` with `paint`, as
    /// Chrome's `SkScan::AntiFillPath` covers it (`aaa.rs`).
    fn cover(&mut self, segs: &[edges::Seg], rule: FillRule, paint: &Paint<'_>) {
        let Some((bounds, force_rle)) = self.clip_bounds() else {
            return;
        };
        if let Some(fill) = aaa::fill_path(segs, rule == FillRule::EvenOdd, bounds, force_rle) {
            self.paint_fill(&fill, paint);
        }
    }
}

/// `a * b / 255`, rounded (`SkMulDiv255Round`, as clips multiply).
fn mul_coverage(a: u8, b: u8) -> u8 {
    let prod = u32::from(a) * u32::from(b) + 128;
    ((prod + (prod >> 8)) >> 8) as u8
}

/// Writes `fill`'s coverage, times `clip` when there is one, into `mask`
/// (whose rows are `width` long) over the fill's rectangle.
fn write_coverage(mask: &mut Mask, fill: &aaa::Fill, clip: Option<&Mask>, width: u32) {
    let clip = clip.map(Mask::data);
    let data = mask.data_mut();
    let r = fill.rect;
    let fw = (r.right - r.left) as usize;
    for (row, y) in (r.top..r.bottom).enumerate() {
        let start = (y as u32 * width + r.left as u32) as usize;
        let end = start + fw;
        let src = &fill.data[row * fw..(row + 1) * fw];
        let dst = &mut data[start..end];
        match clip {
            Some(clip) => {
                for ((d, s), c) in dst.iter_mut().zip(src).zip(&clip[start..end]) {
                    *d = mul_coverage(*s, *c);
                }
            }
            None => dst.copy_from_slice(src),
        }
    }
}

/// Whether Skia draws a stroke of `width` under `ts` as a hairline with
/// its alpha scaled (`SkDrawTreatAAStrokeAsHairline`, which tiny-skia's
/// `stroke_path` ports): both axes of the stroke's width map to at most one
/// device pixel.
fn is_hairline(width: f32, ts: tiny_skia::Transform) -> bool {
    let fast_len = |x: f32, y: f32| {
        let (x, y) = (x.abs(), y.abs());
        let (big, small) = if x < y { (y, x) } else { (x, y) };
        big + small / 2.0
    };
    let len0 = fast_len(ts.sx * width, ts.ky * width);
    let len1 = fast_len(ts.kx * width, ts.sy * width);
    len0 <= 1.0 && len1 <= 1.0
}

fn transform(t: &Transform) -> tiny_skia::Transform {
    tiny_skia::Transform::from_row(
        t.a as f32, t.b as f32, t.c as f32, t.d as f32, t.e as f32, t.f as f32,
    )
}

/// `nearly_integral` in `SkRasterClip.cpp`: within 1/8 of a whole pixel.
fn nearly_integral(x: f32) -> bool {
    let domain = 1.0 / 4.0;
    let x = x + domain / 2.0;
    x - x.floor() < domain
}

/// The smallest rectangle holding the mask's non-zero pixels, as
/// `SkAAClip` trims its bounds.
fn nonzero_bounds(mask: &Mask) -> Option<aaa::IRect> {
    let (w, h) = (mask.width() as i32, mask.height() as i32);
    let data = mask.data();
    let (mut l, mut t, mut r, mut b) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if data[(y * w + x) as usize] != 0 {
                l = l.min(x);
                t = t.min(y);
                r = r.max(x + 1);
                b = b.max(y + 1);
            }
        }
    }
    (l < r && t < b).then_some(aaa::IRect {
        left: l,
        top: t,
        right: r,
        bottom: b,
    })
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

/// A path's segments as a tiny-skia path (the one tiny-skia's hairliner
/// takes), or `None` when it has no geometry. Arcs
/// are Skia's conics, as quadratics within 1/4 unit.
fn skia_path(segs: &[edges::Seg]) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    let mut last = (0.0, 0.0);
    let f = |p: edges::P| (p.0 as f32, p.1 as f32);
    for &seg in segs {
        match seg {
            edges::Seg::Move(p) => {
                pb.move_to(f(p).0, f(p).1);
                last = p;
            }
            edges::Seg::Line(p) => {
                pb.line_to(f(p).0, f(p).1);
                last = p;
            }
            edges::Seg::Quad(c, p) => {
                pb.quad_to(f(c).0, f(c).1, f(p).0, f(p).1);
                last = p;
            }
            edges::Seg::Conic(c, p, w) => {
                for (_, c, q) in edges::conic_quads(last, c, p, w) {
                    pb.quad_to(f(c).0, f(c).1, f(q).0, f(q).1);
                }
                last = p;
            }
            edges::Seg::Cubic(a, b, p) => {
                pb.cubic_to(f(a).0, f(a).1, f(b).0, f(b).1, f(p).0, f(p).1);
                last = p;
            }
            edges::Seg::Close => pb.close(),
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
        // The dash is dash.rs's (SkDashPath), applied before the stroke.
        dash: None,
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
    fn fill(&mut self, path: &Path, _: &Color, c: Rgba, rule: FillRule, state: &PaintState) {
        let Some(c) = color(c, state.alpha) else {
            return;
        };
        let segs = edges::blink_arc_fill(path, &state.transform)
            .unwrap_or_else(|| edges::from_display(path, &state.transform));
        self.cover(&segs, rule, &solid_paint(c));
    }

    fn stroke(&mut self, path: &Path, stroke: &Stroke, c: Rgba, state: &PaintState) {
        let Some(c) = color(c, state.alpha) else {
            return;
        };
        let ts = transform(&state.transform);
        let sk = skia_stroke(stroke);
        let paint = solid_paint(c);
        let hairline = is_hairline(sk.width, ts);
        // As SkDraw::drawPath: the dash (dash.rs, SkDashPath), then Skia's
        // stroker (stroke.rs), both in user space at the matrix's
        // resolution scale, then the outline filled nonzero in device
        // space. Undashed paths keep their arcs as conics. A hairline
        // (modifyPaintForHairlines: width 0) is dashed the same way.
        let res_scale = tiny_skia::PathStroker::compute_resolution_scale(&ts);
        let src = edges::from_display(path, &Transform::IDENTITY);
        let src = match &stroke.dash {
            Some(pattern) => {
                let intervals: Vec<f32> = pattern.segments().iter().map(|&v| v as f32).collect();
                let dash_stroke = dash::DashStroke {
                    width: if hairline { 0.0 } else { sk.width },
                    butt_cap: stroke.cap == LineCap::Butt,
                    miter_join: stroke.join == LineJoin::Miter,
                    miter_limit: sk.miter_limit,
                    res_scale,
                    cull: self.local_cull(&state.transform),
                };
                match dash::dash(&src, &intervals, pattern.offset() as f32, &dash_stroke) {
                    dash::Dashed::Stroke(dashed) => dashed,
                    dash::Dashed::Fill(outline) => {
                        // SpecialLineRec: the dashes are already the outline.
                        let device = edges::transform_segs(&outline, &state.transform);
                        self.cover(&device, FillRule::NonZero, &paint);
                        return;
                    }
                    // FillPathWithPaint draws the source undashed.
                    dash::Dashed::Failed { filled: false } => src,
                    dash::Dashed::Failed { filled: true } => {
                        let device = edges::transform_segs(&src, &state.transform);
                        self.cover(&device, FillRule::NonZero, &paint);
                        return;
                    }
                }
            }
            None => src,
        };
        if hairline {
            // Skia's anti-aliased hairline with the alpha scaled by the
            // width, as the browser draws it; tiny-skia ports it.
            let Some(path) = skia_path(&src) else {
                return;
            };
            let clip = self.clips.last().map(|c| &c.mask);
            self.pixmap.stroke_path(&path, &paint, &sk, ts, clip);
            return;
        }
        let style = stroke::StrokeStyle {
            width: sk.width,
            miter_limit: sk.miter_limit,
            cap: match stroke.cap {
                LineCap::Butt => stroke::Cap::Butt,
                LineCap::Round => stroke::Cap::Round,
                LineCap::Square => stroke::Cap::Square,
            },
            join: match stroke.join {
                LineJoin::Miter => stroke::Join::Miter,
                LineJoin::Round => stroke::Join::Round,
                LineJoin::Bevel => stroke::Join::Bevel,
            },
            res_scale,
        };
        let outline = stroke::stroke_path(&src, &style);
        let device = edges::transform_segs(&outline, &state.transform);
        self.cover(&device, FillRule::NonZero, &paint);
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
        let ts = transform(&state.transform);
        let paint = Paint {
            shader: tiny_skia::Pattern::new(
                bitmap,
                SpreadMode::Pad,
                quality,
                alpha as f32,
                ts.pre_concat(pattern_transform),
            ),
            anti_alias: true,
            ..Paint::default()
        };
        // SkDraw::drawRect with the image shader: an axis-aligned
        // destination is AntiFillRect'ed when Blink anti-aliases the image
        // (ShouldDrawImageAntialiased: under a device pixel either way) and
        // rounded to whole pixels otherwise; any other is a path.
        let Some((bounds, force_rle)) = self.clip_bounds() else {
            return;
        };
        let t = &state.transform;
        let scale_translate = t.b == 0.0 && t.c == 0.0;
        let fill = if scale_translate || (t.a == 0.0 && t.d == 0.0) {
            let (x0, y0) = t.apply(dest.x, dest.y);
            let (x1, y1) = t.apply(dest.x + dest.width, dest.y + dest.height);
            let r = [
                x0.min(x1) as f32,
                y0.min(y1) as f32,
                x0.max(x1) as f32,
                y0.max(y1) as f32,
            ];
            let (wx, hy) = if scale_translate {
                (t.a, t.d)
            } else {
                (t.b, t.c)
            };
            if dest.width * wx.abs() < 1.0 || dest.height * hy.abs() < 1.0 {
                aaa::anti_fill_rect(r, bounds)
            } else {
                aaa::fill_rect(r, bounds)
            }
        } else {
            let rect = Path::rect(dest.x, dest.y, dest.width, dest.height);
            aaa::fill_path(&edges::from_display(&rect, t), false, bounds, force_rle)
        };
        if let Some(fill) = fill {
            self.paint_fill(&fill, &paint);
        }
    }

    fn text(&mut self, run: &TextRun, c: Rgba, state: &PaintState) {
        let Some(c) = color(c, state.alpha) else {
            return;
        };
        let clip = self.clips.last().map(|c| &c.mask);
        self.text
            .fill_text(self.pixmap, run, c, transform(&state.transform), clip);
    }

    fn push_clip(&mut self, clip: &Clip, t: &Transform) {
        // The clip path's coverage times the enclosing clip, zero
        // elsewhere, so an empty clip path clips everything away. As
        // SkRasterClip: a rectangle under a scale/translate matrix whose
        // edges lie within 1/8 px of whole pixels, while the clip is still
        // whole-pixel, stays a whole-pixel region (rounded); anything else
        // becomes an anti-aliased SkAAClip, built with the run-length
        // blitters and bounded by its non-zero pixels.
        let w = self.pixmap.width();
        let mut mask = self.empty_mask();
        let segs = edges::from_display(&clip.path, t);
        let empty = aaa::IRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let (bounds, anti_aliased) = self.clip_bounds().unwrap_or((empty, false));
        let scale_translate = t.b == 0.0 && t.c == 0.0;
        let bw_rect = (!anti_aliased && scale_translate)
            .then(|| aaa::rect_of(&segs))
            .flatten()
            .filter(|r| r.iter().all(|&v| nearly_integral(v)));
        let state = if bounds.left >= bounds.right {
            ClipState {
                mask,
                bounds: empty,
                anti_aliased,
            }
        } else if let Some(r) = bw_rect {
            let round = |v: f32| (v + 0.5).floor() as i32;
            let rect = aaa::IRect {
                left: round(r[0]),
                top: round(r[1]),
                right: round(r[2]),
                bottom: round(r[3]),
            };
            let bounds = rect.intersect(&bounds).unwrap_or(empty);
            let fill = aaa::Fill {
                rect: bounds,
                data: vec![
                    255;
                    ((bounds.right - bounds.left) * (bounds.bottom - bounds.top)) as usize
                ],
            };
            if bounds.left < bounds.right {
                write_coverage(&mut mask, &fill, self.clip(), w);
            }
            ClipState {
                mask,
                bounds,
                anti_aliased: false,
            }
        } else {
            let fill = aaa::fill_path(&segs, clip.rule == FillRule::EvenOdd, bounds, true);
            if let Some(fill) = &fill {
                write_coverage(&mut mask, fill, self.clip(), w);
            }
            let bounds = nonzero_bounds(&mask).unwrap_or(empty);
            ClipState {
                mask,
                bounds,
                anti_aliased: true,
            }
        };
        self.clips.push(state);
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
    }
}
