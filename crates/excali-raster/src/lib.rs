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
//!   source rectangle clipped to the bitmap as `drawImage` does, sampled
//!   as Chrome's software canvas samples (`bilerp.rs`: Skia's bitmap
//!   sampler, 4-bit bilinear weights) or nearest from the item's smoothing
//!   flag, and the dark-theme filter applied to the unpremultiplied pixels;
//!   SVG images drawn as vectors at the draw's resolution, as Chrome draws
//!   them (`svg.rs`: usvg's tree as fills and strokes through the same
//!   scan conversion, or resvg for what the display list cannot say);
//! - `fillRect` as Skia's `drawRect`;
//! - image files decoded from the scene's data URLs ([`decode`]): the
//!   raster formats Chrome reads, sniffed from their bytes, with their Exif
//!   orientation, and SVG;
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
use std::sync::OnceLock;

mod aaa;
mod bilerp;
mod dash;
pub mod decode;
pub mod diff;
mod edges;
mod hairline;
mod stroke;
mod svg;

use excali_scene::display::{builtin_image_by_id, BUILTIN_IMAGE_NAMES};
use excali_scene::display::{
    Clip, Color, DisplayList, FillRule, ImageFilter, ImageItem, LineCap, LineJoin, PaintState,
    Painter, Path, Rect, Rgba, Stroke, TextRun, Transform,
};
pub use resvg;
use resvg::usvg;
pub use tiny_skia;
use tiny_skia::{FilterQuality, Mask, Paint, Pixmap, PixmapRef, SpreadMode};

/// An image the backend draws: a premultiplied RGBA bitmap, or an SVG
/// document drawn as vectors.
#[derive(Clone, Copy, Debug)]
pub enum Image<'a> {
    Bitmap(PixmapRef<'a>),
    /// Its natural size is the tree's size, in CSS pixels.
    Svg(&'a usvg::Tree),
}

/// Where the backend finds the images the display list names, by id
/// (upstream's `imageCache`, keyed by `fileId`); [`decode::ImageFiles`]
/// holds a scene's files.
pub trait ImageStore {
    /// The image `id`, or `None` when there is none (nothing is drawn).
    fn image(&self, id: &str) -> Option<Image<'_>>;
}

impl ImageStore for HashMap<String, Pixmap> {
    fn image(&self, id: &str) -> Option<Image<'_>> {
        self.get(id).map(|p| Image::Bitmap(p.as_ref()))
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
    /// Chrome's `SkScan::AntiFillPath` covers it (`aaa.rs`), or nothing
    /// when the path is too big for Skia's math (`SkDraw::drawDevPath`).
    fn cover(&mut self, segs: &[edges::Seg], rule: FillRule, paint: &Paint<'_>) {
        if too_big_for_math(segs) {
            return;
        }
        let Some((bounds, force_rle)) = self.clip_bounds() else {
            return;
        };
        if let Some(fill) = aaa::fill_path(segs, rule == FillRule::EvenOdd, bounds, force_rle) {
            self.paint_fill(&fill, paint);
        }
    }

    /// Paint `paint` over the image destination `dest` (user space, drawn
    /// under `state`'s matrix), as `SkDraw::drawRect` with the image
    /// shader: an axis-aligned destination is AntiFillRect'ed when Blink
    /// anti-aliases the image (ShouldDrawImageAntialiased: under a device
    /// pixel either way) and rounded to whole pixels otherwise; any other
    /// is a path.
    fn fill_image_rect(&mut self, dest: Rect, state: &PaintState, paint: &Paint<'_>) {
        if let Some(fill) = self.image_fill(dest, state) {
            self.paint_fill(&fill, paint);
        }
    }

    /// The coverage [`RasterPainter::fill_image_rect`] paints through.
    fn image_fill(&self, dest: Rect, state: &PaintState) -> Option<aaa::Fill> {
        let (bounds, force_rle) = self.clip_bounds()?;
        let t = &state.transform;
        let scale_translate = t.b == 0.0 && t.c == 0.0;
        if scale_translate || (t.a == 0.0 && t.d == 0.0) {
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
        }
    }

    /// The device pixels `rect` (user space, under `t`) can touch inside
    /// the clip, `(left, top, right, bottom)`; `None` when there are none.
    fn device_bounds(&self, rect: &Rect, t: &Transform) -> Option<(i32, i32, i32, i32)> {
        let (bounds, _) = self.clip_bounds()?;
        let corners = [
            (rect.x, rect.y),
            (rect.x + rect.width, rect.y),
            (rect.x + rect.width, rect.y + rect.height),
            (rect.x, rect.y + rect.height),
        ]
        .map(|(x, y)| t.apply(x, y));
        let min_x = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
        let min_y = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
        let max_x = corners
            .iter()
            .map(|c| c.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_y = corners
            .iter()
            .map(|c| c.1)
            .fold(f64::NEG_INFINITY, f64::max);
        if ![min_x, min_y, max_x, max_y].iter().all(|v| v.is_finite()) {
            return None;
        }
        let left = (min_x.floor() as i32).max(bounds.left);
        let top = (min_y.floor() as i32).max(bounds.top);
        let right = (max_x.ceil() as i32).min(bounds.right);
        let bottom = (max_y.ceil() as i32).min(bounds.bottom);
        (left < right && top < bottom).then_some((left, top, right, bottom))
    }

    /// `drawImage` of an SVG image as Chrome draws it
    /// (`SVGImage::DrawForContainer`): the document as vector content under
    /// the source-to-destination transform, clipped to the destination, at
    /// full alpha in a layer when `globalAlpha` or a filter applies
    /// (`DrawNeedsLayer`), the filter on the layer, then the layer
    /// composited with the alpha inside the canvas's clip.
    ///
    /// The destination clip is the canvas's anti-aliased `clip()` of the
    /// destination rectangle, which is what Chrome 153 paints under a
    /// matrix that keeps rectangles axis-aligned (the placeholder of a
    /// 12 x 30 image, `image-elements`). Under a rotation Chrome's edge
    /// pixels differ from any anti-aliased or whole-pixel clip of the
    /// rectangle by up to 17 levels where the drawing touches the
    /// destination's edge (a rotated placeholder); inside it the vector
    /// drawing agrees.
    ///
    /// The layer covers the device pixels the destination can touch
    /// inside the clip.
    fn draw_svg(
        &mut self,
        tree: &usvg::Tree,
        dest: &Rect,
        onto: &Transform,
        filter: Option<ImageFilter>,
        alpha: f64,
        state: &PaintState,
    ) {
        let Some((left, top, right, bottom)) = self.device_bounds(dest, &state.transform) else {
            return;
        };
        let Some(mut layer) = Pixmap::new((right - left) as u32, (bottom - top) as u32) else {
            return;
        };
        // User space to the layer's pixels.
        let t = Transform::translate(-f64::from(left), -f64::from(top)).concat(&state.transform);
        let svg_to_device = t.concat(onto);
        let mut text = NoText;
        let mut painter = RasterPainter::new(&mut layer, &NoImages, &mut text);
        painter.push_clip(
            &Clip {
                path: Path::rect(dest.x, dest.y, dest.width, dest.height),
                rule: FillRule::NonZero,
            },
            &t,
        );
        if painter.clip_bounds().is_none() {
            return;
        }
        match svg::display_items(tree) {
            Some(items) => {
                let list: DisplayList = items.into_iter().collect();
                list.replay_from(&mut painter, PaintState::new(svg_to_device, 1.0));
            }
            None => {
                let mask = painter.clips.pop().expect("the destination clip").mask;
                drop(painter);
                resvg::render(tree, transform(&svg_to_device), &mut layer.as_mut());
                for (px, &m) in layer.pixels_mut().iter_mut().zip(mask.data()) {
                    let c =
                        [px.red(), px.green(), px.blue(), px.alpha()].map(|v| mul_coverage(v, m));
                    *px = tiny_skia::PremultipliedColorU8::from_rgba(c[0], c[1], c[2], c[3])
                        .expect("scaled premultiplied channels stay premultiplied");
                }
            }
        }
        if let Some(filter) = filter {
            apply_filter(filter, &mut layer);
        }
        let paint = tiny_skia::PixmapPaint {
            opacity: alpha as f32,
            blend_mode: tiny_skia::BlendMode::SourceOver,
            quality: FilterQuality::Nearest,
        };
        let clip = self.clips.last().map(|c| &c.mask);
        self.pixmap.draw_pixmap(
            left,
            top,
            layer.as_ref(),
            &paint,
            tiny_skia::Transform::identity(),
            clip,
        );
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
    let (len0, len1) = hairline_lengths(width, ts);
    len0 <= 1.0 && len1 <= 1.0
}

/// The fast lengths of the width mapped by the matrix
/// (`SkDrawTreatAsHairline`).
fn hairline_lengths(width: f32, ts: tiny_skia::Transform) -> (f32, f32) {
    let fast_len = |x: f32, y: f32| {
        let (x, y) = (x.abs(), y.abs());
        let (big, small) = if x < y { (y, x) } else { (x, y) };
        big + small / 2.0
    };
    (
        fast_len(ts.sx * width, ts.ky * width),
        fast_len(ts.kx * width, ts.sy * width),
    )
}

/// The coverage tiny-skia scales a thin stroke's hairline by: the mean of
/// the two lengths. Under the identity a width of this coverage has the
/// same coverage.
fn hairline_coverage(width: f32, ts: tiny_skia::Transform) -> f32 {
    let (len0, len1) = hairline_lengths(width, ts);
    (len0 + len1) * 0.5
}

/// `SkPathPriv::TooBigForMath` on a device path's bounds (every point,
/// control points included): a coordinate past a quarter of the f32 range,
/// or one that is not finite. `SkDraw::drawDevPath` draws nothing for such
/// a path (a fill, a stroke's outline or a hairline), since the scan
/// converters add, subtract and multiply by small constants.
fn too_big_for_math(segs: &[edges::Seg]) -> bool {
    let max = f32::MAX * 0.25;
    seg_points(segs)
        .iter()
        .any(|p| !(p.0 >= -max && p.0 <= max && p.1 >= -max && p.1 <= max))
}

fn seg_points(segs: &[edges::Seg]) -> Vec<(f32, f32)> {
    let f = |p: edges::P| (p.0 as f32, p.1 as f32);
    let mut out = Vec::new();
    for s in segs {
        match *s {
            edges::Seg::Move(p) | edges::Seg::Line(p) => out.push(f(p)),
            edges::Seg::Quad(a, p) | edges::Seg::Conic(a, p, _) => out.extend([f(a), f(p)]),
            edges::Seg::Cubic(a, b, p) => out.extend([f(a), f(b), f(p)]),
            edges::Seg::Close => {}
        }
    }
    out
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
/// takes), or `None` when it has no geometry or any point is not finite
/// (SkDraw drops a path that is not finite, `SkPath::isFinite`). Arcs
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
    let path = pb.finish()?;
    path.points()
        .iter()
        .all(|p| p.x.is_finite() && p.y.is_finite())
        .then_some(path)
}

/// Skia's hairline of a device path (hairline.rs, `hair_path`) as a
/// tiny-skia path for its hairliner with butt caps: each draw its own
/// contour, the caps already applied.
fn hair_draws_path(draws: &[hairline::Draw]) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for draw in draws {
        match draw {
            hairline::Draw::Lines(pts) => {
                let Some((first, rest)) = pts.split_first() else {
                    continue;
                };
                pb.move_to(first.0, first.1);
                for p in rest {
                    pb.line_to(p.0, p.1);
                }
            }
            hairline::Draw::Quad([s, c, e]) => {
                pb.move_to(s.0, s.1);
                pb.quad_to(c.0, c.1, e.0, e.1);
            }
            hairline::Draw::Cubic([s, a, b, e]) => {
                pb.move_to(s.0, s.1);
                pb.cubic_to(a.0, a.1, b.0, b.1, e.0, e.1);
            }
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

/// The canvas matrix as Skia holds it (`f32`).
fn matrix32(t: &Transform) -> bilerp::Matrix32 {
    bilerp::Matrix32 {
        sx: t.a as f32,
        kx: t.c as f32,
        tx: t.e as f32,
        ky: t.b as f32,
        sy: t.d as f32,
        ty: t.f as f32,
    }
}

/// `SkMatrix::RectToRect(src, dst)` of `SkRect`s built from `f32` origins
/// and sizes (their widths are `right - left` in `f32`).
fn rect_to_rect(src: &Rect, dst: &Rect) -> bilerp::Matrix32 {
    let side = |origin: f64, length: f64| {
        let o = origin as f32;
        (o, (o + length as f32) - o)
    };
    let (sl, sw) = side(src.x, src.width);
    let (st, sh) = side(src.y, src.height);
    let (dl, dw) = side(dst.x, dst.width);
    let (dt, dh) = side(dst.y, dst.height);
    let sx = dw / sw;
    let sy = dh / sh;
    bilerp::Matrix32 {
        sx,
        kx: 0.0,
        tx: dl - sl * sx,
        ky: 0.0,
        sy,
        ty: dt - st * sy,
    }
}

/// No images: what an SVG image's own drawing is given.
struct NoImages;

impl ImageStore for NoImages {
    fn image(&self, _: &str) -> Option<Image<'_>> {
        None
    }
}

/// No text: an SVG image's text arrives as paths.
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
    }
}

/// The document of the built-in image named by `id`
/// (`excali_scene::display::BuiltinImage`: upstream's image
/// placeholders and link icons), parsed once; `None` when `id` is not one.
fn builtin_tree(id: &str) -> Option<&'static usvg::Tree> {
    static TREES: [OnceLock<usvg::Tree>; BUILTIN_IMAGE_NAMES.len()] =
        [const { OnceLock::new() }; BUILTIN_IMAGE_NAMES.len()];
    let image = builtin_image_by_id(id)?;
    let slot = BUILTIN_IMAGE_NAMES
        .iter()
        .position(|name| image.id.strip_prefix("excalidraw:") == Some(*name))?;
    Some(TREES[slot].get_or_init(|| {
        usvg::Tree::from_str(image.svg, &decode::svg_options())
            .expect("the built-in SVG documents parse")
    }))
}

/// `filter` on each unpremultiplied pixel of `pixmap`.
fn apply_filter(filter: ImageFilter, pixmap: &mut Pixmap) {
    for px in pixmap.pixels_mut() {
        let c = px.demultiply();
        let (r, g, b) = filter.apply_rgb(c.red(), c.green(), c.blue());
        *px = tiny_skia::ColorU8::from_rgba(r, g, b, c.alpha()).premultiply();
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

    fn fill_rect(&mut self, rect: &Rect, css: &Color, c: Rgba, state: &PaintState) {
        // SkDraw::drawRect: under a matrix that keeps rectangles
        // rectangles, the device rectangle through SkScan::AntiFillRect
        // (edges to 1/256 of a pixel); under any other, the path.
        let t = &state.transform;
        if !((t.b == 0.0 && t.c == 0.0) || (t.a == 0.0 && t.d == 0.0)) {
            let path = Path::rect(rect.x, rect.y, rect.width, rect.height);
            self.fill(&path, css, c, FillRule::NonZero, state);
            return;
        }
        let Some(c) = color(c, state.alpha) else {
            return;
        };
        if rect.is_empty() {
            return;
        }
        let (x0, y0) = t.apply(rect.x, rect.y);
        let (x1, y1) = t.apply(rect.x + rect.width, rect.y + rect.height);
        let r = [
            x0.min(x1) as f32,
            y0.min(y1) as f32,
            x0.max(x1) as f32,
            y0.max(y1) as f32,
        ];
        if !r.iter().all(|v| v.is_finite()) {
            return;
        }
        let Some((bounds, _)) = self.clip_bounds() else {
            return;
        };
        if let Some(fill) = aaa::anti_fill_rect(r, bounds) {
            self.paint_fill(&fill, &solid_paint(c));
        }
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
            let device = edges::transform_segs(&src, &state.transform);
            if too_big_for_math(&device) {
                return;
            }
            let (path, sk, ts) = if hairline::overflows(&seg_points(&device)) {
                // A path with curves past what tiny-skia's hairliner takes
                // is walked as Skia's hair_path walks it, in device space
                // (hairline.rs): caps applied, curves cut into Skia's
                // lines, a curve with a point that is not finite skipped
                // alone. Drawn untransformed with butt caps, a width of
                // tiny-skia's coverage (the mean of the fast lengths of
                // the width under the matrix) keeps the alpha it scales
                // the paint by.
                let cap = match stroke.cap {
                    LineCap::Butt => hairline::Cap::Butt,
                    LineCap::Round => hairline::Cap::Round,
                    LineCap::Square => hairline::Cap::Square,
                };
                let sk = tiny_skia::Stroke {
                    width: hairline_coverage(sk.width, ts),
                    line_cap: tiny_skia::LineCap::Butt,
                    ..sk
                };
                (
                    hair_draws_path(&hairline::hair_path(&device, cap)),
                    sk,
                    tiny_skia::Transform::identity(),
                )
            } else {
                (skia_path(&src), sk, ts)
            };
            let Some(path) = path else {
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
        let found = match self.images.image(&image.id) {
            Some(found) => found,
            None => match builtin_tree(&image.id) {
                Some(tree) => Image::Svg(tree),
                None => return,
            },
        };
        let (w, h) = match found {
            Image::Bitmap(bitmap) => (f64::from(bitmap.width()), f64::from(bitmap.height())),
            Image::Svg(tree) => (
                f64::from(tree.size().width()),
                f64::from(tree.size().height()),
            ),
        };
        let source = image.source.unwrap_or(Rect::new(0.0, 0.0, w, h));
        let Some((source, dest)) = clip_image_rects(source, image.dest, w, h) else {
            return;
        };
        let alpha = state.alpha.clamp(0.0, 1.0);
        if alpha == 0.0 {
            return;
        }
        // Image pixels (or the SVG's CSS pixels) to user space: the source
        // rectangle onto the destination.
        let sx = dest.width / source.width;
        let sy = dest.height / source.height;
        let onto = Transform::new(
            sx,
            0.0,
            0.0,
            sy,
            dest.x - source.x * sx,
            dest.y - source.y * sy,
        );
        match found {
            Image::Bitmap(bitmap) => {
                let filtered;
                let bitmap = match image.filter {
                    Some(filter) => {
                        let mut copy = bitmap.to_owned();
                        apply_filter(filter, &mut copy);
                        filtered = copy;
                        filtered.as_ref()
                    }
                    None => bitmap,
                };
                // Skia's sampler: drawImageRect's local matrix (the source
                // rectangle onto the destination, SkMatrix::RectToRect)
                // after the canvas matrix, in f32. An integer translation
                // samples without filtering.
                let total = matrix32(&state.transform).concat(&rect_to_rect(&source, &dest));
                let integer_translate = total.sx == 1.0
                    && total.sy == 1.0
                    && total.kx == 0.0
                    && total.ky == 0.0
                    && total.tx.fract() == 0.0
                    && total.ty.fract() == 0.0;
                if image.smoothing && !integer_translate {
                    // Bilinear with Skia's 4-bit weights, sampled for the
                    // pixels the destination covers and laid on them 1:1.
                    // The blitter shades each run of equal coverage (after
                    // the clip) as its own span.
                    let Some(fill) = self.image_fill(dest, state) else {
                        return;
                    };
                    let width = self.pixmap.width() as i32;
                    let clip = self.clip().map(Mask::data);
                    let r = fill.rect;
                    let coverage = |x: i32, y: i32| {
                        let c =
                            fill.data[((y - r.top) * (r.right - r.left) + (x - r.left)) as usize];
                        match clip {
                            Some(m) => mul_coverage(c, m[(y * width + x) as usize]),
                            None => c,
                        }
                    };
                    let Some(sampled) = total.invert().and_then(|inverse| {
                        bilerp::sample(bitmap, &inverse, r.left, r.top, r.right, r.bottom, coverage)
                    }) else {
                        return;
                    };
                    let paint = Paint {
                        shader: tiny_skia::Pattern::new(
                            sampled.as_ref(),
                            SpreadMode::Pad,
                            FilterQuality::Nearest,
                            alpha as f32,
                            tiny_skia::Transform::from_translate(r.left as f32, r.top as f32),
                        ),
                        anti_alias: true,
                        ..Paint::default()
                    };
                    self.paint_fill(&fill, &paint);
                } else {
                    let paint = Paint {
                        shader: tiny_skia::Pattern::new(
                            bitmap,
                            SpreadMode::Pad,
                            FilterQuality::Nearest,
                            alpha as f32,
                            transform(&state.transform.concat(&onto)),
                        ),
                        anti_alias: true,
                        ..Paint::default()
                    };
                    self.fill_image_rect(dest, state, &paint);
                }
            }
            Image::Svg(tree) => self.draw_svg(tree, &dest, &onto, image.filter, alpha, state),
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
