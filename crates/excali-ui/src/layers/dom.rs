//! The three canvases in a document (`web-sys`).

use std::collections::HashMap;

use excali_canvas2d::WebCanvas;
use excali_scene::display::{DisplayList, Rect};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    CanvasRenderingContext2d, Document, Element, HtmlCanvasElement, HtmlElement, HtmlImageElement,
};

use super::{
    paint_interactive_layer, paint_new_element_layer, paint_static_layer, BackingSize, Layer,
    LayerContext,
};

impl LayerContext for WebCanvas {
    fn clear_rect(&mut self, rect: &Rect) {
        self.context
            .clear_rect(rect.x, rect.y, rect.width, rect.height);
    }
}

/// `t("labels.drawingCanvas")` (`locales/en.json`), the interactive
/// canvas's fallback content (`InteractiveCanvas.tsx:231`).
const DRAWING_CANVAS_LABEL: &str = "Drawing canvas";

/// One layer's canvas and the context its display lists are painted into.
struct LayerCanvas {
    canvas: HtmlCanvasElement,
    painter: WebCanvas,
}

impl LayerCanvas {
    fn new(document: &Document, layer: Layer) -> Result<LayerCanvas, JsValue> {
        let canvas: HtmlCanvasElement = document.create_element("canvas")?.dyn_into()?;
        canvas.set_class_name(layer.class_name());
        let context: CanvasRenderingContext2d = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("the canvas has no 2d context"))?
            .dyn_into()?;
        Ok(LayerCanvas {
            canvas,
            painter: WebCanvas::new(context),
        })
    }

    /// The CSS box and the backing store for `size` at `scale`.
    fn size(&self, layer: Layer, size: (f64, f64), scale: f64) -> Result<(), JsValue> {
        let style = self.canvas.style();
        style.set_property("width", &css_px(size.0))?;
        style.set_property("height", &css_px(size.1))?;
        let backing = layer.backing_size(size.0, size.1, scale);
        self.canvas.set_width(backing.width);
        self.canvas.set_height(backing.height);
        Ok(())
    }

    fn backing_size(&self) -> BackingSize {
        BackingSize {
            width: self.canvas.width(),
            height: self.canvas.height(),
        }
    }
}

/// `${n}px`, as the static canvas writes its size and React writes a
/// number `style` value.
fn css_px(n: f64) -> String {
    format!("{}px", excali_core::json::number_to_string(n))
}

/// The layered canvases mounted in the editor's container, as `App.tsx`
/// renders them: `div.excalidraw__canvas-wrapper` holding the static
/// canvas, the new-element canvas while there is a preview, and the
/// interactive canvas.
///
/// [`CanvasLayers::resize`] gives every canvas the CSS size and a backing
/// store of that size times the device pixel ratio
/// ([`Layer::backing_size`]); the `paint_*` methods draw a layer's frame
/// (its bootstrap, then its display list) on its canvas.
pub struct CanvasLayers {
    document: Document,
    wrapper: HtmlElement,
    static_layer: LayerCanvas,
    new_element: Option<LayerCanvas>,
    interactive: LayerCanvas,
    /// The images registered with [`CanvasLayers::add_image`], which a
    /// new-element canvas gets when it is mounted.
    images: HashMap<String, HtmlImageElement>,
    /// The CSS size and device pixel ratio of the last resize.
    css_size: (f64, f64),
    scale: f64,
}

impl CanvasLayers {
    /// Creates the canvases and appends them to `parent`: the wrapper with
    /// the static canvas, then the interactive canvas. They have the
    /// canvas's default size until [`CanvasLayers::resize`].
    pub fn mount(parent: &Element) -> Result<CanvasLayers, JsValue> {
        let document = parent
            .owner_document()
            .ok_or_else(|| JsValue::from_str("the parent is not in a document"))?;
        let wrapper: HtmlElement = document.create_element("div")?.dyn_into()?;
        wrapper.set_class_name("excalidraw__canvas-wrapper");
        let static_layer = LayerCanvas::new(&document, Layer::Static)?;
        wrapper.append_child(&static_layer.canvas)?;
        let interactive = LayerCanvas::new(&document, Layer::Interactive)?;
        interactive
            .canvas
            .set_text_content(Some(DRAWING_CANVAS_LABEL));
        parent.append_child(&wrapper)?;
        parent.append_child(&interactive.canvas)?;
        let scale = device_pixel_ratio(&document);
        Ok(CanvasLayers {
            document,
            wrapper,
            static_layer,
            new_element: None,
            interactive,
            images: HashMap::new(),
            css_size: (
                f64::from(DEFAULT_SIZE.width),
                f64::from(DEFAULT_SIZE.height),
            ),
            scale,
        })
    }

    /// The owner window's `devicePixelRatio` (what `App.tsx` passes every
    /// canvas as `scale`), 1 without a window.
    pub fn device_pixel_ratio(&self) -> f64 {
        device_pixel_ratio(&self.document)
    }

    /// Sizes every canvas: CSS size `width` × `height`, backing store that
    /// times `scale` (the device pixel ratio). Setting a canvas's size
    /// clears it; paint the layers after.
    pub fn resize(&mut self, width: f64, height: f64, scale: f64) {
        self.css_size = (width, height);
        self.scale = scale;
        for (layer, canvas) in self.canvases() {
            // Style and size assignments on a canvas the document owns do
            // not throw.
            let _ = canvas.size(layer, (width, height), scale);
        }
    }

    /// The device pixel ratio of the last resize.
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// The CSS size of the last resize.
    pub fn css_size(&self) -> (f64, f64) {
        self.css_size
    }

    /// A layer's backing store as its canvas holds it (the default size of
    /// the new-element canvas when it is not mounted).
    pub fn backing_size(&self, layer: Layer) -> BackingSize {
        match self.layer(layer) {
            Some(canvas) => canvas.backing_size(),
            None => layer.backing_size(self.css_size.0, self.css_size.1, self.scale),
        }
    }

    /// A layer's canvas, `None` for the new-element canvas without a
    /// preview.
    pub fn canvas(&self, layer: Layer) -> Option<&HtmlCanvasElement> {
        self.layer(layer).map(|l| &l.canvas)
    }

    /// `div.excalidraw__canvas-wrapper`, the static canvas's parent.
    pub fn wrapper(&self) -> &HtmlElement {
        &self.wrapper
    }

    /// Mounts the new-element canvas between the static and interactive
    /// ones, sized as they are, at CSS `opacity` when given (a tool dragged
    /// out of the toolbar previews at `TOOL_DRAG_PREVIEW_OPACITY`,
    /// `App.tsx:2717-2723`). Upstream mounts a fresh canvas whenever a
    /// preview starts (`App.tsx:2692`); an existing one only takes the
    /// opacity.
    pub fn show_new_element(&mut self, opacity: Option<f64>) -> Result<(), JsValue> {
        if self.new_element.is_none() {
            let mut layer = LayerCanvas::new(&self.document, Layer::NewElement)?;
            layer.painter.images.extend(self.images.clone());
            layer.size(Layer::NewElement, self.css_size, self.scale)?;
            let parent = self
                .interactive
                .canvas
                .parent_node()
                .ok_or_else(|| JsValue::from_str("the canvases are not mounted"))?;
            parent.insert_before(&layer.canvas, Some(&self.interactive.canvas))?;
            self.new_element = Some(layer);
        }
        if let Some(layer) = &self.new_element {
            let style = layer.canvas.style();
            match opacity {
                Some(o) => {
                    style.set_property("opacity", &excali_core::json::number_to_string(o))?
                }
                None => {
                    style.remove_property("opacity")?;
                }
            }
        }
        Ok(())
    }

    /// Removes the new-element canvas (the preview ended).
    pub fn hide_new_element(&mut self) {
        if let Some(layer) = self.new_element.take() {
            layer.canvas.remove();
        }
    }

    /// Registers the image `id` (a file id) with every layer's context.
    pub fn add_image(&mut self, id: &str, image: HtmlImageElement) {
        for layer in self.canvases_mut() {
            layer.painter.images.insert(id.to_owned(), image.clone());
        }
        self.images.insert(id.to_owned(), image);
    }

    /// The static canvas's frame: [`super::paint_static_layer`].
    pub fn paint_static(&mut self, view_background_color: Option<&str>, list: &DisplayList) {
        let size = self.static_layer.backing_size();
        paint_static_layer(
            &mut self.static_layer.painter,
            size,
            self.scale,
            view_background_color,
            list,
        );
    }

    /// The new-element canvas's frame, when it is mounted:
    /// [`super::paint_new_element_layer`].
    pub fn paint_new_element(&mut self, zoom: f64, drawing: Option<&DisplayList>) {
        let scale = self.scale;
        if let Some(layer) = &mut self.new_element {
            let size = layer.backing_size();
            paint_new_element_layer(&mut layer.painter, size, scale, zoom, drawing);
        }
    }

    /// The interactive canvas's frame: [`super::paint_interactive_layer`].
    pub fn paint_interactive(&mut self, list: &DisplayList) {
        let size = self.interactive.backing_size();
        paint_interactive_layer(&mut self.interactive.painter, size, self.scale, list);
    }

    /// Removes every canvas from the document.
    pub fn unmount(self) {
        if let Some(layer) = &self.new_element {
            layer.canvas.remove();
        }
        self.interactive.canvas.remove();
        self.wrapper.remove();
    }

    fn layer(&self, layer: Layer) -> Option<&LayerCanvas> {
        match layer {
            Layer::Static => Some(&self.static_layer),
            Layer::NewElement => self.new_element.as_ref(),
            Layer::Interactive => Some(&self.interactive),
        }
    }

    fn canvases(&self) -> impl Iterator<Item = (Layer, &LayerCanvas)> {
        Layer::ALL
            .into_iter()
            .filter_map(|layer| self.layer(layer).map(|c| (layer, c)))
    }

    fn canvases_mut(&mut self) -> impl Iterator<Item = &mut LayerCanvas> {
        [
            Some(&mut self.static_layer),
            self.new_element.as_mut(),
            Some(&mut self.interactive),
        ]
        .into_iter()
        .flatten()
    }
}

/// A canvas's size before it is sized.
const DEFAULT_SIZE: BackingSize = BackingSize {
    width: super::DEFAULT_CANVAS_WIDTH,
    height: super::DEFAULT_CANVAS_HEIGHT,
};

fn device_pixel_ratio(document: &Document) -> f64 {
    document
        .default_view()
        .map_or(1.0, |window| window.device_pixel_ratio())
}
