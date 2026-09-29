//! ex-503 harness: excali-ui's layered canvases in the browser.
//!
//! [`Layers`] mounts [`excali_ui::layers::CanvasLayers`] in an element of
//! the page, sizes it from the page's `devicePixelRatio` as the editor does,
//! and paints its layers: the static canvas from a list of elements
//! (`excali_scene::static_scene::render_static_scene`), the new-element
//! canvas from one element (`render_new_element_scene`), and the
//! interactive canvas cleared. The backing size rules are exported so the
//! Playwright suite `tests/web/layers` can hold them to Chromium's own
//! `HTMLCanvasElement`. `scripts/web/canvas-layers.sh` builds it.

use excali_core::element::Element;
use excali_scene::bounds::ElementsMap;
use excali_scene::display::DisplayList;
use excali_scene::new_element_scene::{render_new_element_scene, NewElementScene};
use excali_scene::static_scene::{
    render_static_scene, StaticCanvasAppState, StaticCanvasRenderConfig, StaticScene,
};
use excali_text::text_measurements::TextMetricsProvider;
use excali_ui::layers::{
    canvas_dimension_from_attribute, canvas_dimension_from_property, CanvasLayers, Layer,
    CANVAS_LAYER_CSS,
};
use wasm_bindgen::prelude::*;

/// Upstream's test text metrics: 10 px per UTF-16 code unit (only the
/// placeholder labels of iframes are measured by the static scene).
struct TenPxPerCodeUnit;

impl TextMetricsProvider for TenPxPerCodeUnit {
    fn get_line_width(&self, text: &str, _font: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

/// Elements from a JSON array of `.excalidraw` elements.
pub fn parse_elements(json: &str) -> Result<Vec<Element>, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let list = value
        .as_array()
        .ok_or_else(|| "expected a JSON array of elements".to_owned())?;
    list.iter()
        .map(|v| {
            let map = v
                .as_object()
                .ok_or_else(|| "an element is not an object".to_owned())?;
            Element::from_map(map.clone()).map_err(|e| e.to_string())
        })
        .collect()
}

/// `canvas.width = x` as the static canvas assigns it.
#[wasm_bindgen(js_name = canvasDimensionFromProperty)]
pub fn dimension_from_property(x: f64, default: u32) -> u32 {
    canvas_dimension_from_property(x, default)
}

/// `width={x}` as React writes the other canvases' attribute.
#[wasm_bindgen(js_name = canvasDimensionFromAttribute)]
pub fn dimension_from_attribute(x: f64, default: u32) -> u32 {
    canvas_dimension_from_attribute(x, default)
}

/// The canvases' stylesheet rules.
#[wasm_bindgen(js_name = canvasLayerCss)]
pub fn canvas_layer_css() -> String {
    CANVAS_LAYER_CSS.to_owned()
}

fn layer(name: &str) -> Result<Layer, JsError> {
    match name {
        "static" => Ok(Layer::Static),
        "new-element" => Ok(Layer::NewElement),
        "interactive" => Ok(Layer::Interactive),
        _ => Err(JsError::new(&format!("no layer {name:?}"))),
    }
}

/// `new Layers(parent)`: the three canvases mounted in `parent`.
#[wasm_bindgen]
pub struct Layers {
    layers: CanvasLayers,
}

#[wasm_bindgen]
impl Layers {
    #[wasm_bindgen(constructor)]
    pub fn new(parent: web_sys::Element) -> Result<Layers, JsValue> {
        Ok(Layers {
            layers: CanvasLayers::mount(&parent)?,
        })
    }

    /// Sizes every canvas to `width` × `height` CSS pixels at the owner
    /// window's `devicePixelRatio`, which it returns.
    pub fn resize(&mut self, width: f64, height: f64) -> f64 {
        let scale = self.layers.device_pixel_ratio();
        self.layers.resize(width, height, scale);
        scale
    }

    /// The canvas of a layer (`"static"`, `"new-element"`,
    /// `"interactive"`), `undefined` when it is not mounted.
    pub fn canvas(&self, name: &str) -> Result<Option<web_sys::HtmlCanvasElement>, JsError> {
        Ok(self.layers.canvas(layer(name)?).cloned())
    }

    /// Mounts the new-element canvas (a preview), at `opacity` when given.
    #[wasm_bindgen(js_name = showNewElement)]
    pub fn show_new_element(&mut self, opacity: Option<f64>) -> Result<(), JsValue> {
        self.layers.show_new_element(opacity)
    }

    #[wasm_bindgen(js_name = hideNewElement)]
    pub fn hide_new_element(&mut self) {
        self.layers.hide_new_element();
    }

    /// Paints the static canvas: the elements at the scroll and zoom, over
    /// `background` (none when `undefined`), with the grid when `grid`.
    #[wasm_bindgen(js_name = paintStatic)]
    pub fn paint_static(
        &mut self,
        elements_json: &str,
        scroll_x: f64,
        scroll_y: f64,
        zoom: f64,
        background: Option<String>,
        grid: bool,
    ) -> Result<(), JsError> {
        let elements = parse_elements(elements_json).map_err(|e| JsError::new(&e))?;
        let map = ElementsMap::new(&elements);
        let visible: Vec<&Element> = elements.iter().collect();
        let state = StaticCanvasAppState {
            zoom,
            scroll_x,
            scroll_y,
            view_background_color: background,
            ..StaticCanvasAppState::default()
        };
        let config = StaticCanvasRenderConfig {
            render_grid: grid,
            ..StaticCanvasRenderConfig::default()
        };
        let scale = self.layers.scale();
        let size = self.layers.backing_size(Layer::Static);
        let list = render_static_scene(&StaticScene {
            canvas_width: size.width as f64,
            canvas_height: size.height as f64,
            scale,
            elements_map: &map,
            all_elements_map: &map,
            visible_elements: &visible,
            app_state: &state,
            render_config: &config,
            text_metrics: &TenPxPerCodeUnit,
        });
        self.layers
            .paint_static(state.view_background_color.as_deref(), &list);
        Ok(())
    }

    /// Paints the new-element canvas with `element_json` (one element, or
    /// `undefined` for none) at the scroll and zoom.
    #[wasm_bindgen(js_name = paintNewElement)]
    pub fn paint_new_element(
        &mut self,
        element_json: Option<String>,
        scroll_x: f64,
        scroll_y: f64,
        zoom: f64,
    ) -> Result<(), JsError> {
        let element = match element_json {
            Some(json) => {
                let mut list =
                    parse_elements(&format!("[{json}]")).map_err(|e| JsError::new(&e))?;
                list.pop()
            }
            None => None,
        };
        let empty = ElementsMap::new([]);
        let state = StaticCanvasAppState {
            zoom,
            scroll_x,
            scroll_y,
            ..StaticCanvasAppState::default()
        };
        let config = StaticCanvasRenderConfig {
            render_grid: false,
            ..StaticCanvasRenderConfig::default()
        };
        let scale = self.layers.scale();
        let size = self.layers.backing_size(Layer::NewElement);
        let drawing = render_new_element_scene(&NewElementScene {
            canvas_width: size.width as f64,
            canvas_height: size.height as f64,
            scale,
            new_element: element.as_ref(),
            elements_map: &empty,
            all_elements_map: &empty,
            app_state: &state,
            render_config: &config,
        });
        self.layers.paint_new_element(zoom, drawing.as_ref());
        Ok(())
    }

    /// Paints the interactive canvas with nothing on it (its bootstrap
    /// clear).
    #[wasm_bindgen(js_name = paintInteractive)]
    pub fn paint_interactive(&mut self) {
        self.layers.paint_interactive(&DisplayList::default());
    }

    /// Removes the canvases from the page.
    pub fn unmount(self) {
        self.layers.unmount();
    }
}
