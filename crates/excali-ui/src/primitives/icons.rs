//! The icons the primitives use, from the icon set ([`crate::icons`],
//! rendered from `components/icons.tsx` at the pin).

use excali_scene::shape::Theme;

use crate::dom::Element;
use crate::icons::{self, Icon};

fn element(icon: &Icon) -> Element {
    icon.element(Theme::Light)
        .expect("a createIcon element, the same in both themes")
}

/// `CloseIcon` (`icons.tsx:958-976`).
pub fn close_icon() -> Element {
    element(&icons::CloseIcon)
}

/// `eyeIcon` (`icons.tsx:2112-2119`): shows a redacted value.
pub fn eye_icon() -> Element {
    element(&icons::eyeIcon)
}

/// `eyeClosedIcon` (`icons.tsx:2121-2129`): redacts it again.
pub fn eye_closed_icon() -> Element {
    element(&icons::eyeClosedIcon)
}
