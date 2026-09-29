//! The form factor rules of `common/src/editorInterface.ts`: the
//! breakpoints, phone / tablet / desktop from the editor's size, and the
//! styles panel mode each form factor gets (a tablet's is compact). See
//! `site/content/research/ui-design-system.md` section 1.4.

/// `MQ_MAX_MOBILE` (`editorInterface.ts:19`).
pub const MQ_MAX_MOBILE: f64 = 599.0;
/// `MQ_MAX_WIDTH_LANDSCAPE` (:21).
pub const MQ_MAX_WIDTH_LANDSCAPE: f64 = 1000.0;
/// `MQ_MAX_HEIGHT_LANDSCAPE` (:22).
pub const MQ_MAX_HEIGHT_LANDSCAPE: f64 = 500.0;
/// `MQ_MIN_TABLET` (:25): the lower bound, excluding phones.
pub const MQ_MIN_TABLET: f64 = MQ_MAX_MOBILE + 1.0;
/// `MQ_MAX_TABLET` (:26): an iPad Air.
pub const MQ_MAX_TABLET: f64 = 1180.0;
/// `MQ_MIN_WIDTH_DESKTOP` (:29), not used for form factor detection.
pub const MQ_MIN_WIDTH_DESKTOP: f64 = 1440.0;
/// `MQ_RIGHT_SIDEBAR_MIN_WIDTH` (:32).
pub const MQ_RIGHT_SIDEBAR_MIN_WIDTH: f64 = 1229.0;

/// `DESKTOP_UI_MODE_STORAGE_KEY` (:16): the localStorage key of the
/// desktop UI mode preference.
pub const DESKTOP_UI_MODE_STORAGE_KEY: &str = "excalidraw.desktopUIMode";

/// `EditorInterface["formFactor"]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormFactor {
    Phone,
    Tablet,
    Desktop,
}

impl FormFactor {
    pub fn as_str(self) -> &'static str {
        match self {
            FormFactor::Phone => "phone",
            FormFactor::Tablet => "tablet",
            FormFactor::Desktop => "desktop",
        }
    }
}

/// `EditorInterface["desktopUIMode"]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopUiMode {
    Compact,
    Full,
}

impl DesktopUiMode {
    pub fn as_str(self) -> &'static str {
        match self {
            DesktopUiMode::Compact => "compact",
            DesktopUiMode::Full => "full",
        }
    }

    /// "compact" or "full"; `None` for anything else.
    pub fn parse(value: &str) -> Option<DesktopUiMode> {
        match value {
            "compact" => Some(DesktopUiMode::Compact),
            "full" => Some(DesktopUiMode::Full),
            _ => None,
        }
    }
}

/// `StylesPanelMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StylesPanelMode {
    Compact,
    Full,
    Mobile,
}

impl StylesPanelMode {
    pub fn as_str(self) -> &'static str {
        match self {
            StylesPanelMode::Compact => "compact",
            StylesPanelMode::Full => "full",
            StylesPanelMode::Mobile => "mobile",
        }
    }
}

/// `isMobileBreakpoint` (:64-69): narrow, or a short landscape screen.
pub fn is_mobile_breakpoint(width: f64, height: f64) -> bool {
    width <= MQ_MAX_MOBILE || (height < MQ_MAX_HEIGHT_LANDSCAPE && width < MQ_MAX_WIDTH_LANDSCAPE)
}

/// `isTabletBreakpoint` (:71-79): the shorter side at least
/// [`MQ_MIN_TABLET`] and the longer at most [`MQ_MAX_TABLET`], in either
/// orientation.
pub fn is_tablet_breakpoint(width: f64, height: f64) -> bool {
    let min_side = width.min(height);
    let max_side = width.max(height);
    min_side >= MQ_MIN_TABLET && max_side <= MQ_MAX_TABLET
}

/// `getFormFactor` (:137-150): phone first, then tablet, else desktop.
pub fn get_form_factor(width: f64, height: f64) -> FormFactor {
    if is_mobile_breakpoint(width, height) {
        FormFactor::Phone
    } else if is_tablet_breakpoint(width, height) {
        FormFactor::Tablet
    } else {
        FormFactor::Desktop
    }
}

/// `deriveStylesPanelMode` (:152-164): mobile on a phone, compact on a
/// tablet, the desktop UI mode otherwise.
pub fn derive_styles_panel_mode(
    form_factor: FormFactor,
    desktop_ui_mode: DesktopUiMode,
) -> StylesPanelMode {
    match (form_factor, desktop_ui_mode) {
        (FormFactor::Phone, _) => StylesPanelMode::Mobile,
        (FormFactor::Tablet, _) | (FormFactor::Desktop, DesktopUiMode::Compact) => {
            StylesPanelMode::Compact
        }
        (FormFactor::Desktop, DesktopUiMode::Full) => StylesPanelMode::Full,
    }
}

/// `loadDesktopUIModePreference` (:186-202) for the value stored under
/// [`DESKTOP_UI_MODE_STORAGE_KEY`]: only "compact" and "full" count.
pub fn load_desktop_ui_mode_preference(stored: Option<&str>) -> Option<DesktopUiMode> {
    stored.and_then(DesktopUiMode::parse)
}
