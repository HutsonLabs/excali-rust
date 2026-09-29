//! [`Context2d`] over the browser's `CanvasRenderingContext2D` (`web-sys`).

use std::collections::HashMap;

use excali_scene::display::{builtin_image, DisplayList, Rect, Transform, BUILTIN_IMAGE_NAMES};
use web_sys::wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, CanvasWindingRule, HtmlCanvasElement, HtmlImageElement};

use crate::{paint, Context2d};

/// A browser 2D context and the images the display list names, keyed by
/// id (upstream's `imageCache`, keyed by `fileId`). [`WebCanvas::new`]
/// starts the map with the built-in images
/// (`excali_scene::display::BuiltinImage`: upstream's image
/// placeholders and link icons, under their `excalidraw:…` ids), which
/// load as upstream's own do, from SVG data URLs
/// ([`crate::builtin_image_src`]).
///
/// Canvas methods that throw do so only for arguments the canvas rejects
/// without drawing (`arc` with a negative radius, `drawImage` of a broken
/// image, `setTransform` with a non-finite value); upstream's own call
/// would throw out of the render at that point, and here the draw is
/// skipped instead, leaving the canvas as the rejected call leaves it.
///
/// [`WebCanvas::bitmaps`] holds canvases by id (the scene's cached
/// bitmaps, `excali_scene::display::bitmap_id`), which [`Context2d::image_size`]
/// and [`Context2d::draw_image`] look up before the images.
pub struct WebCanvas {
    pub context: CanvasRenderingContext2d,
    pub images: HashMap<String, HtmlImageElement>,
    pub bitmaps: HashMap<String, HtmlCanvasElement>,
}

impl WebCanvas {
    pub fn new(context: CanvasRenderingContext2d) -> Self {
        let mut images = HashMap::new();
        for builtin in BUILTIN_IMAGE_NAMES
            .iter()
            .filter_map(|name| builtin_image(name))
        {
            // Outside a document (a worker) there is no Image(): the
            // built-in images are then not drawn, as before they load.
            if let Ok(image) = HtmlImageElement::new() {
                image.set_src(&crate::builtin_image_src(&builtin));
                images.insert(builtin.id.to_owned(), image);
            }
        }
        Self {
            context,
            images,
            bitmaps: HashMap::new(),
        }
    }

    /// A new canvas of `width` × `height` device pixels in this canvas's
    /// document with `list` painted into it from its identity matrix, as
    /// `generateElementCanvas` draws a bitmap (`renderElement.ts:271-339`),
    /// with this canvas's images. `None` outside a document or when the
    /// canvas has no 2D context.
    pub fn rasterize(
        &self,
        width: f64,
        height: f64,
        list: &DisplayList,
    ) -> Option<HtmlCanvasElement> {
        let document = self.context.canvas()?.owner_document()?;
        let canvas: HtmlCanvasElement = document.create_element("canvas").ok()?.dyn_into().ok()?;
        // `canvas.width = width`: whole pixels
        canvas.set_width(width as u32);
        canvas.set_height(height as u32);
        let context: CanvasRenderingContext2d = canvas.get_context("2d").ok()??.dyn_into().ok()?;
        let mut target = WebCanvas {
            context,
            images: self.images.clone(),
            bitmaps: HashMap::new(),
        };
        paint(list, &mut target);
        Some(canvas)
    }
}

fn winding(rule: &str) -> CanvasWindingRule {
    if rule == "evenodd" {
        CanvasWindingRule::Evenodd
    } else {
        CanvasWindingRule::Nonzero
    }
}

impl Context2d for WebCanvas {
    fn save(&mut self) {
        self.context.save();
    }

    fn restore(&mut self) {
        self.context.restore();
    }

    fn set_transform(&mut self, t: &Transform) {
        // Throws only for non-finite values, which the canvas ignores.
        let _ = self.context.set_transform(t.a, t.b, t.c, t.d, t.e, t.f);
    }

    fn set_global_alpha(&mut self, alpha: f64) {
        self.context.set_global_alpha(alpha);
    }

    fn set_fill_style(&mut self, css: &str) {
        self.context.set_fill_style_str(css);
    }

    fn set_stroke_style(&mut self, css: &str) {
        self.context.set_stroke_style_str(css);
    }

    fn set_line_width(&mut self, width: f64) {
        self.context.set_line_width(width);
    }

    fn set_line_cap(&mut self, cap: &str) {
        self.context.set_line_cap(cap);
    }

    fn set_line_join(&mut self, join: &str) {
        self.context.set_line_join(join);
    }

    fn set_miter_limit(&mut self, limit: f64) {
        self.context.set_miter_limit(limit);
    }

    fn set_line_dash(&mut self, segments: &[f64]) {
        let list: js_sys::Array = segments.iter().map(|&s| js_sys::Number::from(s)).collect();
        // Throws only for a value that is not a sequence of numbers.
        let _ = self.context.set_line_dash(&list);
    }

    fn set_line_dash_offset(&mut self, offset: f64) {
        self.context.set_line_dash_offset(offset);
    }

    fn begin_path(&mut self) {
        self.context.begin_path();
    }

    fn move_to(&mut self, x: f64, y: f64) {
        self.context.move_to(x, y);
    }

    fn line_to(&mut self, x: f64, y: f64) {
        self.context.line_to(x, y);
    }

    fn quadratic_curve_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        self.context.quadratic_curve_to(cx, cy, x, y);
    }

    fn bezier_curve_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64) {
        self.context.bezier_curve_to(c1x, c1y, c2x, c2y, x, y);
    }

    fn arc(&mut self, cx: f64, cy: f64, radius: f64, start: f64, end: f64, anticlockwise: bool) {
        // IndexSizeError for a negative radius: nothing is added.
        let _ = self
            .context
            .arc_with_anticlockwise(cx, cy, radius, start, end, anticlockwise);
    }

    fn close_path(&mut self) {
        self.context.close_path();
    }

    fn fill(&mut self, rule: &str) {
        self.context.fill_with_canvas_winding_rule(winding(rule));
    }

    fn fill_rect(&mut self, rect: &Rect) {
        self.context
            .fill_rect(rect.x, rect.y, rect.width, rect.height);
    }

    fn stroke(&mut self) {
        self.context.stroke();
    }

    fn clip(&mut self, rule: &str) {
        self.context.clip_with_canvas_winding_rule(winding(rule));
    }

    fn set_font(&mut self, css: &str) {
        self.context.set_font(css);
    }

    fn set_text_align(&mut self, align: &str) {
        self.context.set_text_align(align);
    }

    fn set_direction(&mut self, direction: &str) {
        // Upstream sets the canvas element's `dir` attribute and attaches
        // the canvas to the document for it to apply
        // (`renderElement.ts:627-634`). The context's own `direction`
        // property (part of the drawing state, so `restore()` resets it)
        // gives the same base direction without touching the DOM; web-sys
        // has no binding for it, so it is set by name.
        let _ = js_sys::Reflect::set(
            &self.context,
            &js_sys::JsString::from("direction"),
            &js_sys::JsString::from(direction),
        );
    }

    fn fill_text(&mut self, text: &str, x: f64, y: f64) {
        // Throws only for a non-finite maxWidth, which is not passed.
        let _ = self.context.fill_text(text, x, y);
    }

    fn set_image_smoothing_enabled(&mut self, enabled: bool) {
        self.context.set_image_smoothing_enabled(enabled);
    }

    fn set_filter(&mut self, css: &str) {
        self.context.set_filter(css);
    }

    fn image_size(&self, id: &str) -> Option<(f64, f64)> {
        if let Some(canvas) = self.bitmaps.get(id) {
            return Some((f64::from(canvas.width()), f64::from(canvas.height())));
        }
        let image = self.images.get(id)?;
        if !image.complete() || image.natural_width() == 0 {
            return None;
        }
        Some((
            f64::from(image.natural_width()),
            f64::from(image.natural_height()),
        ))
    }

    fn draw_image(&mut self, id: &str, source: &Rect, dest: &Rect) {
        if let Some(canvas) = self.bitmaps.get(id) {
            // InvalidStateError for a canvas of width or height 0: nothing
            // is drawn.
            let _ = self
                .context
                .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    canvas,
                    source.x,
                    source.y,
                    source.width,
                    source.height,
                    dest.x,
                    dest.y,
                    dest.width,
                    dest.height,
                );
            return;
        }
        let Some(image) = self.images.get(id) else {
            return;
        };
        // InvalidStateError for a broken image: nothing is drawn.
        let _ = self
            .context
            .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                image,
                source.x,
                source.y,
                source.width,
                source.height,
                dest.x,
                dest.y,
                dest.width,
                dest.height,
            );
    }
}
