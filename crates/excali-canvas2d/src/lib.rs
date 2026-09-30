//! Canvas 2D backend that paints a display list in the browser.
//!
//! Upstream counterpart: `renderElement.ts` canvas paths.
//!
//! The backend knows the display list (`excali_scene::display`) and the
//! canvas, nothing about elements (ADR-008). [`paint`] replays a list into
//! any [`Context2d`]: the calls of `CanvasRenderingContext2D` it needs, so
//! the translation is tested natively against a recording context and
//! [`WebCanvas`] forwards the same calls to the browser one to one.
//!
//! Each draw is isolated: `save()`, `setTransform` to its absolute matrix,
//! `globalAlpha`, its style properties, the path and the draw call,
//! `restore()`. A clip is `save()`, `setTransform`, the path, `clip(rule)`,
//! and its pop is the matching `restore()`, so clips nest as the list's
//! groups do. Colours are assigned as the display list holds them, the
//! element's own string, as upstream assigns `element.strokeColor`
//! (`renderElement.ts`), so the browser's CSS parser decides; an
//! assignment it ignores leaves the style the context had before the draw.
//!
//! In the browser, `scripts/web/canvas2d-fixtures.sh` (ex-502) paints every
//! excali-raster fixture display list with [`paint_scaled`] through
//! [`WebCanvas`] in Chromium and requires each canvas to be within the
//! fixture's tolerance of an independent canvas reading of the list and of
//! excali-raster's render (`tools/canvas2d-fixtures`, `tests/web/canvas2d`).
//!
//! The editor draws most elements from a bitmap of their own (the scene's
//! `element_canvas` decides each bitmap and its blit): [`WebCanvas::rasterize`]
//! paints a bitmap's display list into a new canvas, [`WebCanvas::bitmaps`]
//! holds the canvases by id, and [`blit`] draws one under its absolute,
//! pixel-snapped matrix.
//!
//! Targets: wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-scene`.

mod web;

use excali_scene::display::{
    Blit, Clip, Color, DisplayList, FillRule, ImageItem, PaintState, Painter, Path, PathCommand,
    Rect, Rgba, Stroke, TextRun, Transform,
};

pub use web::{Bitmap, WebCanvas};

/// The `src` [`WebCanvas`] loads a built-in image from: upstream's own data
/// URL (`BuiltinImage::data_url`) when the document has a `width`, as the
/// link icons do, and otherwise the document with `width` and `height` set
/// to its square `viewBox` (the same drawing): an `<img>` of an SVG with
/// neither has no natural size to answer [`Context2d::image_size`] with.
pub fn builtin_image_src(image: &excali_scene::display::BuiltinImage) -> String {
    let svg = image.svg;
    let root = &svg[..svg.find('>').unwrap_or(svg.len())];
    if root.contains(" width=") {
        return image.data_url.clone();
    }
    let side = root
        .find("viewBox=\"")
        .map(|i| &root[i + 9..])
        .and_then(|v| v.split('"').next())
        .and_then(|v| v.split(' ').nth(2))
        .unwrap_or("0");
    let sized = svg.replacen(
        "<svg ",
        &format!("<svg width=\"{side}\" height=\"{side}\" "),
        1,
    );
    format!(
        "data:image/svg+xml,{}",
        excali_scene::display::encode_uri_component(&sized)
    )
}

/// The `CanvasRenderingContext2D` surface the backend draws through. Each
/// method is the canvas method or property of the same name; images are
/// named by the display list's ids and resolved by the implementation.
pub trait Context2d {
    fn save(&mut self);
    fn restore(&mut self);
    /// `setTransform(a, b, c, d, e, f)`.
    fn set_transform(&mut self, t: &Transform);
    fn set_global_alpha(&mut self, alpha: f64);
    fn set_fill_style(&mut self, css: &str);
    fn set_stroke_style(&mut self, css: &str);
    fn set_line_width(&mut self, width: f64);
    fn set_line_cap(&mut self, cap: &str);
    fn set_line_join(&mut self, join: &str);
    fn set_miter_limit(&mut self, limit: f64);
    fn set_line_dash(&mut self, segments: &[f64]);
    fn set_line_dash_offset(&mut self, offset: f64);
    fn begin_path(&mut self);
    fn move_to(&mut self, x: f64, y: f64);
    fn line_to(&mut self, x: f64, y: f64);
    fn quadratic_curve_to(&mut self, cx: f64, cy: f64, x: f64, y: f64);
    fn bezier_curve_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64);
    fn arc(&mut self, cx: f64, cy: f64, radius: f64, start: f64, end: f64, anticlockwise: bool);
    fn close_path(&mut self);
    /// `fill(rule)`.
    fn fill(&mut self, rule: &str);
    /// `fillRect(x, y, w, h)`.
    fn fill_rect(&mut self, rect: &Rect);
    fn stroke(&mut self);
    /// `clip(rule)`.
    fn clip(&mut self, rule: &str);
    fn set_font(&mut self, css: &str);
    fn set_text_align(&mut self, align: &str);
    fn set_direction(&mut self, direction: &str);
    fn fill_text(&mut self, text: &str, x: f64, y: f64);
    fn set_image_smoothing_enabled(&mut self, enabled: bool);
    fn set_filter(&mut self, css: &str);
    /// The natural size of the image `id`, or `None` when there is no such
    /// image or it has not loaded.
    fn image_size(&self, id: &str) -> Option<(f64, f64)>;
    /// `drawImage(image, sx, sy, sw, sh, dx, dy, dw, dh)`.
    fn draw_image(&mut self, id: &str, source: &Rect, dest: &Rect);
}

/// Paint `list` into a fresh `ctx` (identity matrix, alpha 1, black
/// styles).
pub fn paint<C: Context2d>(list: &DisplayList, ctx: &mut C) {
    let mut painter = CanvasPainter::new(ctx);
    list.replay(&mut painter);
    painter.end_images();
}

/// Paint `list` into a fresh `ctx` scaled by `device_pixel_ratio`, as
/// `bootstrapCanvas` does before drawing (`renderer/helpers.ts:73-127`).
pub fn paint_scaled<C: Context2d>(list: &DisplayList, ctx: &mut C, device_pixel_ratio: f64) {
    let base = PaintState::new(
        Transform::scale(device_pixel_ratio, device_pixel_ratio),
        1.0,
    );
    let mut painter = CanvasPainter::new(ctx);
    list.replay_from(&mut painter, base);
    painter.end_images();
}

/// Paint `list` into `ctx` from `base`: inside one `save()`/`restore()`,
/// set `fillStyle` and `strokeStyle` to the base styles, which a draw
/// whose colour the browser ignores keeps, then paint every draw with its
/// matrix and alpha taken from `base` down.
pub fn paint_from<C: Context2d>(list: &DisplayList, ctx: &mut C, base: PaintState) {
    ctx.save();
    ctx.set_fill_style(&base.fill_style.css());
    ctx.set_stroke_style(&base.stroke_style.css());
    let mut painter = CanvasPainter::new(ctx);
    list.replay_from(&mut painter, base);
    painter.end_images();
    ctx.restore();
}

/// Draw a cached bitmap ([`Blit`]; upstream's `drawElementFromCanvas`,
/// `renderElement.ts:762-934`) into `ctx`: inside one
/// `save()`/`restore()`, `globalAlpha`, `imageSmoothingEnabled` when the
/// blit sets it, the clip under its own matrix, then `setTransform` to the
/// blit's absolute matrix and `drawImage` of the whole bitmap into its
/// destination. Nothing when `ctx` has no bitmap of that id.
pub fn blit<C: Context2d>(ctx: &mut C, blit: &Blit) {
    let Some((width, height)) = ctx.image_size(&blit.id) else {
        return;
    };
    ctx.save();
    ctx.set_global_alpha(blit.alpha);
    if let Some(smoothing) = blit.smoothing {
        ctx.set_image_smoothing_enabled(smoothing);
    }
    if let Some((clip, transform)) = &blit.clip {
        ctx.set_transform(transform);
        CanvasPainter::new(&mut *ctx).trace(&clip.path);
        ctx.clip(clip.rule.as_css());
    }
    ctx.set_transform(&blit.transform);
    ctx.draw_image(&blit.id, &Rect::new(0.0, 0.0, width, height), &blit.dest);
    ctx.restore();
}

/// The [`Painter`] that turns each draw into context calls.
struct CanvasPainter<'a, C: Context2d> {
    ctx: &'a mut C,
    /// An open run of images: what its `save()` has since been set to.
    images: Option<ImageRun>,
}

/// The matrix, alpha and smoothing assigned in an open run of images.
struct ImageRun {
    transform: Transform,
    alpha: f64,
    smoothing: bool,
}

impl<'a, C: Context2d> CanvasPainter<'a, C> {
    fn new(ctx: &'a mut C) -> Self {
        CanvasPainter { ctx, images: None }
    }

    /// Closes an open run of images (its `restore()`).
    fn end_images(&mut self) {
        if self.images.take().is_some() {
            self.ctx.restore();
        }
    }

    fn begin(&mut self, state: &PaintState) {
        self.end_images();
        self.ctx.save();
        self.ctx.set_transform(&state.transform);
        self.ctx.set_global_alpha(state.alpha);
    }

    fn trace(&mut self, path: &Path) {
        self.ctx.begin_path();
        for command in &path.commands {
            match *command {
                PathCommand::MoveTo(x, y) => self.ctx.move_to(x, y),
                PathCommand::LineTo(x, y) => self.ctx.line_to(x, y),
                PathCommand::QuadTo(cx, cy, x, y) => self.ctx.quadratic_curve_to(cx, cy, x, y),
                PathCommand::CubicTo(a, b, c, d, x, y) => {
                    self.ctx.bezier_curve_to(a, b, c, d, x, y)
                }
                PathCommand::Arc {
                    cx,
                    cy,
                    radius,
                    start,
                    end,
                    anticlockwise,
                } => self.ctx.arc(cx, cy, radius, start, end, anticlockwise),
                PathCommand::Close => self.ctx.close_path(),
            }
        }
    }
}

impl<C: Context2d> Painter for CanvasPainter<'_, C> {
    fn fill(&mut self, path: &Path, color: &Color, _: Rgba, rule: FillRule, state: &PaintState) {
        self.begin(state);
        self.ctx.set_fill_style(color.as_str());
        self.trace(path);
        self.ctx.fill(rule.as_css());
        self.ctx.restore();
    }

    fn fill_rect(&mut self, rect: &Rect, color: &Color, _: Rgba, state: &PaintState) {
        self.begin(state);
        self.ctx.set_fill_style(color.as_str());
        self.ctx.fill_rect(rect);
        self.ctx.restore();
    }

    fn stroke(&mut self, path: &Path, stroke: &Stroke, _: Rgba, state: &PaintState) {
        self.begin(state);
        self.ctx.set_stroke_style(stroke.color.as_str());
        self.ctx.set_line_width(stroke.effective_width());
        self.ctx.set_line_cap(stroke.cap.as_css());
        self.ctx.set_line_join(stroke.join.as_css());
        self.ctx.set_miter_limit(stroke.effective_miter_limit());
        if let Some(dash) = &stroke.dash {
            self.ctx.set_line_dash(dash.segments());
            self.ctx.set_line_dash_offset(dash.offset());
        }
        self.trace(path);
        self.ctx.stroke();
        self.ctx.restore();
    }

    /// Consecutive images without a filter (the editor's bitmap blits)
    /// share one `save()`/`restore()`, assigning the matrix, alpha and
    /// smoothing only when they change: `drawImage` reads nothing else an
    /// image sets, so each draws as it would isolated.
    fn image(&mut self, image: &ImageItem, state: &PaintState) {
        let Some((width, height)) = self.ctx.image_size(&image.id) else {
            return;
        };
        let source = image
            .source
            .unwrap_or_else(|| Rect::new(0.0, 0.0, width, height));
        if image.filter.is_none() {
            match &mut self.images {
                None => {
                    self.ctx.save();
                    self.ctx.set_transform(&state.transform);
                    self.ctx.set_global_alpha(state.alpha);
                    self.ctx.set_image_smoothing_enabled(image.smoothing);
                    self.images = Some(ImageRun {
                        transform: state.transform,
                        alpha: state.alpha,
                        smoothing: image.smoothing,
                    });
                }
                Some(run) => {
                    if run.transform != state.transform {
                        self.ctx.set_transform(&state.transform);
                        run.transform = state.transform;
                    }
                    if run.alpha != state.alpha {
                        self.ctx.set_global_alpha(state.alpha);
                        run.alpha = state.alpha;
                    }
                    if run.smoothing != image.smoothing {
                        self.ctx.set_image_smoothing_enabled(image.smoothing);
                        run.smoothing = image.smoothing;
                    }
                }
            }
            self.ctx.draw_image(&image.id, &source, &image.dest);
            return;
        }
        self.begin(state);
        self.ctx.set_image_smoothing_enabled(image.smoothing);
        if let Some(filter) = image.filter {
            self.ctx.set_filter(filter.css());
        }
        self.ctx.draw_image(&image.id, &source, &image.dest);
        self.ctx.restore();
    }

    fn text(&mut self, run: &TextRun, _: Rgba, state: &PaintState) {
        self.begin(state);
        self.ctx.set_font(&run.font.css());
        self.ctx.set_fill_style(run.color.as_str());
        self.ctx.set_text_align(run.align.as_css());
        self.ctx.set_direction(run.direction.as_css());
        self.ctx.fill_text(&run.text, run.x, run.y);
        self.ctx.restore();
    }

    fn push_clip(&mut self, clip: &Clip, transform: &Transform) {
        self.end_images();
        self.ctx.save();
        self.ctx.set_transform(transform);
        self.trace(&clip.path);
        self.ctx.clip(clip.rule.as_css());
    }

    fn pop_clip(&mut self) {
        self.end_images();
        self.ctx.restore();
    }
}
