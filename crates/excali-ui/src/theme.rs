//! The theme: upstream's CSS custom properties (`css/theme.scss`), light on
//! `.excalidraw` and dark on `.excalidraw.theme--dark`, as the embedded
//! stylesheet ([`PRIMITIVES_CSS`]) declares them, and the container's
//! `theme--dark` class and inline tokens.
//!
//! Upstream counterpart: `packages/excalidraw/css/theme.scss` (the tokens,
//! `site/content/design-system/tokens.md`), `App.tsx:2418-2455` (the
//! container's classes and inline style) and `App.tsx:4434-4437` (the
//! `theme--dark` toggle).
//!
//! A host themes the editor as upstream documents: its own rules on
//! `.excalidraw` and `.excalidraw.theme--dark` override the tokens.
//! [`install_stylesheet`](crate::primitives::install_stylesheet) puts the
//! stylesheet first in the document's head, so a host rule of the same
//! specificity wins wherever the host's stylesheet is.

use excali_scene::shape::Theme;
use wasm_bindgen::JsValue;
use web_sys::{Element, HtmlElement};

use crate::primitives::PRIMITIVES_CSS;

/// The class the container carries in the dark theme (`App.tsx:4434-4437`).
pub const THEME_DARK_CLASS: &str = "theme--dark";

/// `RIGHT_SIDEBAR_WIDTH` (`components/App.viewport.ts:62`), px.
pub const RIGHT_SIDEBAR_WIDTH: f64 = 302.0;

/// The class `theme` adds to the container, if any.
pub fn theme_class(theme: Theme) -> Option<&'static str> {
    match theme {
        Theme::Light => None,
        Theme::Dark => Some(THEME_DARK_CLASS),
    }
}

/// The custom property declarations of the first rule `key` names in
/// `css` (a selector, or `@media … selector` inside a query), as
/// `(name, value)` in source order; a value spanning lines is joined with
/// single spaces.
fn declarations(css: &str, key: &str) -> Vec<(String, String)> {
    let mut stack: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    let mut found = false;
    let mut pending: Option<String> = None;
    for raw in css.lines() {
        let line = raw.trim();
        let inside = stack.join(" ") == key;
        if let Some(mut text) = pending.take() {
            text.push(' ');
            text.push_str(line);
            if line.ends_with(';') {
                if inside {
                    out.push(split(&text));
                }
            } else {
                pending = Some(text);
            }
            continue;
        }
        if let Some(selector) = line.strip_suffix('{') {
            stack.push(selector.trim());
            continue;
        }
        if line == "}" {
            if inside {
                found = true;
            }
            stack.pop();
            if found {
                break;
            }
            continue;
        }
        if !inside || !line.starts_with("--") {
            continue;
        }
        if line.ends_with(';') {
            out.push(split(line));
        } else {
            pending = Some(line.to_string());
        }
    }
    out
}

fn split(text: &str) -> (String, String) {
    let text = text.strip_suffix(';').unwrap_or(text);
    let (name, value) = text.split_once(':').unwrap_or((text, ""));
    (name.trim().to_string(), value.trim().to_string())
}

/// The tokens `.excalidraw` declares (`theme.scss:5-165`), in source order.
pub fn light_tokens() -> Vec<(String, String)> {
    declarations(PRIMITIVES_CSS, ".excalidraw")
}

/// The tokens `.excalidraw.theme--dark` redeclares (`theme.scss:184-278`).
pub fn dark_tokens() -> Vec<(String, String)> {
    declarations(PRIMITIVES_CSS, ".excalidraw.theme--dark")
}

/// The tokens `.excalidraw--mobile` redeclares (`theme.scss:167-169`).
pub fn mobile_tokens() -> Vec<(String, String)> {
    declarations(PRIMITIVES_CSS, ".excalidraw--mobile.excalidraw")
}

/// The tokens screens at least 1921 device pixels wide redeclare
/// (`theme.scss:171-176`).
pub fn large_screen_tokens() -> Vec<(String, String)> {
    declarations(
        PRIMITIVES_CSS,
        "@media screen and (min-device-width: 1921px) .excalidraw",
    )
}

/// What each token of `theme` is on a container without the host's rules:
/// the light declarations with the dark ones over them, a name declared
/// twice taking the later value, in the order names first appear.
pub fn tokens(theme: Theme) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let dark = match theme {
        Theme::Light => Vec::new(),
        Theme::Dark => dark_tokens(),
    };
    for (name, value) in light_tokens().into_iter().chain(dark) {
        match out.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = value,
            None => out.push((name, value)),
        }
    }
    out
}

/// `ZEN_MODE_TRANSITION_DURATION` (`common/src/constants.ts:361`), ms.
pub const ZEN_MODE_TRANSITION_DURATION: f64 = 250.0;

/// The tokens `App.tsx:2449-2454` writes into the container's inline
/// style, by name. `--ui-pointerEvents` is `POINTER_EVENTS.enabled`
/// (`constants.ts:53-55`): `shouldBlockPointerEvents` (`App.tsx:2398-2407`)
/// is false wherever `setPointerCapture` exists.
pub fn container_tokens() -> Vec<(&'static str, String)> {
    vec![
        (
            "--right-sidebar-width",
            format!(
                "{}px",
                excali_core::json::number_to_string(RIGHT_SIDEBAR_WIDTH)
            ),
        ),
        ("--ui-pointerEvents", "all".into()),
        (
            "--zen-mode-transition-duration",
            format!(
                "{}ms",
                excali_core::json::number_to_string(ZEN_MODE_TRANSITION_DURATION)
            ),
        ),
    ]
}

/// Writes [`container_tokens`] into `container`'s inline style.
pub fn apply_container_tokens(container: &HtmlElement) -> Result<(), JsValue> {
    let style = container.style();
    for (name, value) in container_tokens() {
        style.set_property(name, &value)?;
    }
    Ok(())
}

/// Puts `container` (the `.excalidraw` element) in `theme`: toggles
/// `theme--dark` (`App.tsx:4434-4437`).
pub fn apply_theme(container: &Element, theme: Theme) -> Result<(), JsValue> {
    container
        .class_list()
        .toggle_with_force(THEME_DARK_CLASS, theme == Theme::Dark)?;
    Ok(())
}
