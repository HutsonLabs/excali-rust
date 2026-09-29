//! The eye dropper (`components/EyeDropper.tsx`, `EyeDropper.scss`): a
//! backdrop over the canvas with the dropper cursor and a preview swatch
//! beside the pointer, sampling the canvas pixel under it; a press
//! previews the colour live, the release picks it, Escape or a window
//! blur cancels.

use std::cell::Cell;
use std::rc::Rc;

use excali_core::color::{is_color_dark, remove_dark_mode_filter, rgb_to_hex};
use excali_scene::display::encode_uri_component;
use excali_scene::shape::Theme;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    CanvasRenderingContext2d, Document, EventTarget, HtmlCanvasElement, HtmlElement, KeyboardEvent,
    PointerEvent,
};

use crate::icons;

/// `eyeDropperCursor` (`EyeDropper.tsx:26-35`): the dropper's two paths
/// outlined white under a dark stroke, as a `data:` SVG cursor with its
/// hotspot at the tip.
pub fn eye_dropper_cursor() -> String {
    let icons::Markup::Paths(paths) = icons::eyeDropperIconSvgPaths.markup else {
        unreachable!("eyeDropperIconSvgPaths is a path list");
    };
    let paths: String = paths
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let fill = if i == 0 { "#fff" } else { "" };
            format!(r#"<path fill="{fill}" d="{d}" />"#)
        })
        .collect();
    let svg = format!(
        r##"<svg viewBox="0 0 24 24" width="24" height="24" fill="none" xmlns="http://www.w3.org/2000/svg" stroke-linecap="round" stroke-linejoin="round"><g stroke="#fff" stroke-width="5">{paths}</g><g stroke="#1b1b1f" stroke-width="1.25">{paths}</g></svg>"##
    );
    // MIME_TYPES.svg
    format!(
        "url(data:image/svg+xml,{}) 2 21, auto",
        encode_uri_component(&svg)
    )
}

/// A bounding client rect's position and size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContainerRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

/// `positionAxis` (`positionElementBesideCursor.ts:3-23`).
fn position_axis(cursor: f64, size: f64, container: f64, gap: f64) -> f64 {
    let position = if cursor + gap + size > container {
        cursor - gap - size
    } else {
        cursor + gap
    };
    // clamp(value, min, max) of @excalidraw/math
    position.max(0.0).min(0.0_f64.max(container - size))
}

/// `positionElementBesideCursor` (`positionElementBesideCursor.ts:34-63`):
/// the container-local `(left, top)` of an element `gap` beside the
/// client-space `cursor`, flipped to the cursor's other side on an axis
/// where it would overflow, and kept inside the container as far as it
/// fits.
pub fn position_element_beside_cursor(
    cursor: (f64, f64),
    element: (f64, f64),
    container: ContainerRect,
    gap: f64,
) -> (f64, f64) {
    (
        position_axis(cursor.0 - container.left, element.0, container.width, gap),
        position_axis(cursor.1 - container.top, element.1, container.height, gap),
    )
}

/// The canvas pixel the dropper samples for a client point
/// (`EyeDropper.tsx:95-104`): the point less the canvas offset, times the
/// device pixel ratio.
pub fn sample_point(client: (f64, f64), offset: (f64, f64), device_pixel_ratio: f64) -> (f64, f64) {
    (
        (client.0 - offset.0) * device_pixel_ratio,
        (client.1 - offset.1) * device_pixel_ratio,
    )
}

/// The colour a sampled pixel shows (`rgbToHex`) and the colour picking
/// it applies: in the dark theme the canvas is drawn through the dark-mode
/// filter, so the pick removes it (`EyeDropper.tsx:106-107`).
pub fn sampled_colors(rgb: (u8, u8, u8), theme: Theme) -> (String, String) {
    let shown = rgb_to_hex(f64::from(rgb.0), f64::from(rgb.1), f64::from(rgb.2), None);
    let applied = match theme {
        Theme::Dark => remove_dark_mode_filter(&shown),
        Theme::Light => shown.clone(),
    };
    (shown, applied)
}

/// The preview's border colour for the colour it shows
/// (`--eye-dropper-preview-border-color`, `EyeDropper.tsx:139-142`).
pub fn preview_border_color(color: &str) -> &'static str {
    if is_color_dark(color, None) {
        "#fff"
    } else {
        "#222"
    }
}

/// What the dropper reads and reports.
#[derive(Clone)]
pub struct EyeDropperProps {
    /// The canvas sampled (the static scene canvas).
    pub canvas: HtmlCanvasElement,
    /// `appState.offsetLeft` and `offsetTop`.
    pub offset: (f64, f64),
    pub theme: Theme,
    /// The pointer's last client position, for the first preview.
    pub last_pointer: (f64, f64),
    /// Called while the pointer is held down, with the colour to apply and
    /// whether Alt is held (`onChange`, a live preview).
    pub on_change: Rc<dyn Fn(String, bool)>,
    /// Called on release with the colour to apply (`onSelect`).
    pub on_select: Rc<dyn Fn(String)>,
    /// Escape, a window blur or a press outside (`onCancel`).
    pub on_cancel: Rc<dyn Fn()>,
}

type Listener = (
    EventTarget,
    &'static str,
    Closure<dyn FnMut(web_sys::Event)>,
);

/// A mounted eye dropper; dropping it removes the backdrop and its
/// listeners.
pub struct EyeDropper {
    backdrop: HtmlElement,
    listeners: Vec<Listener>,
}

impl Drop for EyeDropper {
    fn drop(&mut self) {
        for (target, event, closure) in &self.listeners {
            let _ = target.remove_event_listener_with_callback_and_bool(
                event,
                closure.as_ref().unchecked_ref(),
                false,
            );
        }
        self.backdrop.remove();
    }
}

impl EyeDropper {
    /// Mounts the dropper in `parent` (the editor's
    /// `.excalidraw-eye-dropper-container`): the
    /// `excalidraw-eye-dropper-backdrop` with the dropper cursor, focused
    /// for its keys, and the `excalidraw-eye-dropper-preview` in it.
    pub fn mount(
        document: &Document,
        parent: &web_sys::Element,
        props: EyeDropperProps,
    ) -> Result<EyeDropper, JsValue> {
        let window = document
            .default_view()
            .ok_or_else(|| JsValue::from_str("the document has no window"))?;
        let backdrop: HtmlElement = document.create_element("div")?.dyn_into()?;
        backdrop.set_class_name("excalidraw-eye-dropper-backdrop");
        backdrop
            .style()
            .set_property("cursor", &eye_dropper_cursor())?;
        let preview: HtmlElement = document.create_element("div")?.dyn_into()?;
        preview.set_class_name("excalidraw-eye-dropper-preview");
        backdrop.append_child(&preview)?;
        parent.append_child(&backdrop)?;
        backdrop.set_tab_index(-1);
        backdrop.focus()?;

        let ctx: CanvasRenderingContext2d = props
            .canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("the canvas has no 2d context"))?
            .dyn_into()?;
        let dpr = window.device_pixel_ratio();
        let props = Rc::new(props);
        let sample = {
            let props = props.clone();
            move |x: f64, y: f64| -> (String, String) {
                let (sx, sy) = sample_point((x, y), props.offset, dpr);
                let rgb = ctx
                    .get_image_data(sx, sy, 1.0, 1.0)
                    .map(|d| {
                        let data = d.data();
                        (
                            data.first().copied().unwrap_or(0),
                            data.get(1).copied().unwrap_or(0),
                            data.get(2).copied().unwrap_or(0),
                        )
                    })
                    .unwrap_or((0, 0, 0));
                sampled_colors(rgb, props.theme)
            }
        };
        let sample = Rc::new(sample);
        let holding = Rc::new(Cell::new(false));

        let on_move = {
            let (sample, holding, props, preview, backdrop) = (
                sample.clone(),
                holding.clone(),
                props.clone(),
                preview.clone(),
                backdrop.clone(),
            );
            move |x: f64, y: f64, alt: bool| {
                let rect = backdrop.get_bounding_client_rect();
                let (left, top) = position_element_beside_cursor(
                    (x, y),
                    (
                        f64::from(preview.offset_width()),
                        f64::from(preview.offset_height()),
                    ),
                    ContainerRect {
                        left: rect.left(),
                        top: rect.top(),
                        width: rect.width(),
                        height: rect.height(),
                    },
                    7.0,
                );
                let style = preview.style();
                let _ = style.set_property("top", &format!("{top}px"));
                let _ = style.set_property("left", &format!("{left}px"));
                let (shown, applied) = sample(x, y);
                if holding.get() {
                    (props.on_change)(applied, alt);
                }
                let _ = style.set_property("background", &shown);
                let _ = style.set_property(
                    "--eye-dropper-preview-border-color",
                    preview_border_color(&shown),
                );
            }
        };
        let on_move = Rc::new(on_move);
        // the preview shows before the first move
        on_move(props.last_pointer.0, props.last_pointer.1, false);

        let mut listeners: Vec<Listener> = Vec::new();
        let mut listen = |target: &EventTarget,
                          event: &'static str,
                          f: Box<dyn FnMut(web_sys::Event)>|
         -> Result<(), JsValue> {
            let closure = Closure::<dyn FnMut(web_sys::Event)>::new(f);
            target.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref())?;
            listeners.push((target.clone(), event, closure));
            Ok(())
        };

        {
            let props = props.clone();
            listen(
                backdrop.as_ref(),
                "keydown",
                Box::new(move |e| {
                    if let Some(e) = e.dyn_ref::<KeyboardEvent>() {
                        if e.key() == "Escape" {
                            e.prevent_default();
                            e.stop_immediate_propagation();
                            (props.on_cancel)();
                        }
                    }
                }),
            )?;
        }
        {
            let holding = holding.clone();
            listen(
                backdrop.as_ref(),
                "pointerdown",
                Box::new(move |e| {
                    holding.set(true);
                    // not preventDefault: that would stop pointermove
                    e.stop_immediate_propagation();
                }),
            )?;
        }
        {
            let (holding, sample, props, parent) = (
                holding.clone(),
                sample.clone(),
                props.clone(),
                parent.clone(),
            );
            listen(
                backdrop.as_ref(),
                "pointerup",
                Box::new(move |e| {
                    holding.set(false);
                    // focus went to the body: give it back to the editor
                    if let Some(container) = parent
                        .closest(".excalidraw")
                        .ok()
                        .flatten()
                        .and_then(|c| c.dyn_into::<HtmlElement>().ok())
                    {
                        let _ = container.focus();
                    }
                    e.stop_immediate_propagation();
                    e.prevent_default();
                    if let Some(p) = e.dyn_ref::<PointerEvent>() {
                        let (_, applied) = sample(f64::from(p.client_x()), f64::from(p.client_y()));
                        (props.on_select)(applied);
                    }
                }),
            )?;
        }
        {
            let on_move = on_move.clone();
            listen(
                window.as_ref(),
                "pointermove",
                Box::new(move |e| {
                    if let Some(p) = e.dyn_ref::<PointerEvent>() {
                        on_move(
                            f64::from(p.client_x()),
                            f64::from(p.client_y()),
                            p.alt_key(),
                        );
                    }
                }),
            )?;
        }
        {
            let props = props.clone();
            listen(
                window.as_ref(),
                "blur",
                Box::new(move |_| (props.on_cancel)()),
            )?;
        }
        {
            // a press outside the preview, the backdrop and the dropper's
            // trigger cancels (useOutsideClick, EyeDropper.tsx:264-279)
            let (props, backdrop) = (props.clone(), backdrop.clone());
            listen(
                document.as_ref(),
                "pointerdown",
                Box::new(move |e| {
                    let inside = e
                        .target()
                        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                        .is_some_and(|t| {
                            backdrop.contains(Some(&t))
                                || t
                                    .closest(
                                        ".excalidraw-eye-dropper-trigger, .excalidraw-eye-dropper-backdrop",
                                    )
                                    .ok()
                                    .flatten()
                                    .is_some()
                        });
                    if !inside {
                        (props.on_cancel)();
                    }
                }),
            )?;
        }

        Ok(EyeDropper {
            backdrop,
            listeners,
        })
    }
}
