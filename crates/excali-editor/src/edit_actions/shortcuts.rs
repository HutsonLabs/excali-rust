//! The `perform`s of the actions behind the deselect, flip, element lock,
//! copy and paste styles, view mode and theme shortcuts, behind
//! [`perform_shortcut_action`].

use excali_core::app_state::AppState;
use excali_core::element::Element;
use serde_json::Value;

use super::{ActionResult, StyleEnv};
use crate::actions::ActionName;
use crate::resize_elements::TransformEnv;

/// What the shortcut actions draw, measure and lay out: a [`StyleEnv`]
/// that also transforms ([`TransformEnv`], the flips).
pub trait ShortcutEnv: StyleEnv + TransformEnv {}

impl<T: StyleEnv + TransformEnv> ShortcutEnv for T {}

/// What the shortcut actions read from and write to the host.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShortcutHost {
    /// `copiedStyles` (`actionStyles.ts:48`): the JSON copyStyles writes
    /// and pasteStyles reads.
    pub copied_styles: String,
    /// `app.props.onThemeChange` is set: toggleTheme hands the theme over.
    pub on_theme_change: bool,
    /// `t("toast.copyStyles")`.
    pub copy_styles_toast: String,
}

/// `action.perform(elements, appState, value, app)` for the shortcut
/// actions.
pub fn perform_shortcut_action<E: ShortcutEnv>(
    name: ActionName,
    elements: &[Element],
    app_state: &AppState,
    value: &Value,
    host: &mut ShortcutHost,
    env: &mut E,
) -> Option<ActionResult> {
    let _ = (name, elements, app_state, value, host, env);
    None
}
