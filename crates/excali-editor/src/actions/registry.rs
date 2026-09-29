//! Every action as data: what each `register({...})` call under
//! `packages/excalidraw/actions/` declares apart from `perform` and the
//! `PanelComponent` body (which live with the features they drive), with
//! the `keyTest`, `predicate`, `label`, `icon` and `checked` callbacks
//! ported.

use excali_core::app_state::AppState;
use excali_core::element::{ElementKind, ElementType};
use serde_json::Value;

use super::context::{
    can_create_link_from_elements, frame_and_children_selected_together, has_bound_text_element,
    is_bound_to_container, is_frame_like, is_text_bindable_container, is_type, ActionContext,
    FormFactor,
};
use super::keys::{match_key, KeyEvent};
use super::names::ActionName;
use crate::js_value::truthy;
use crate::viewport::{ZoomAction, ZoomKeyEvent};

/// `keyTest(event, appState, elements, app)`.
pub type KeyTestFn = for<'a, 'b> fn(&KeyEvent<'a>, &ActionContext<'b>) -> bool;
/// `predicate(elements, appState, appProps, app)`.
pub type PredicateFn = for<'a> fn(&ActionContext<'a>) -> bool;
/// `checked(appState)`.
pub type CheckedFn = fn(&AppState) -> bool;

/// `Action.label`: a locale key (or a literal upstream never translated),
/// or a function of the state returning one.
#[derive(Debug, Clone, Copy)]
pub enum ActionLabel {
    Key(&'static str),
    Dynamic(for<'a> fn(&ActionContext<'a>) -> &'static str),
}

/// `Action.icon`, by the `icons.tsx` export it renders.
#[derive(Debug, Clone, Copy)]
pub enum ActionIcon {
    None,
    Static(&'static str),
    Dynamic(for<'a> fn(&ActionContext<'a>) -> Option<&'static str>),
}

/// `Action.trackEvent` when not `false`.
#[derive(Debug, Clone, Copy)]
pub struct TrackEvent {
    pub category: &'static str,
    /// Defaults to the action's name.
    pub action: Option<&'static str>,
    /// Tracks only when this holds for the state before the action.
    pub predicate: Option<fn(&AppState) -> bool>,
}

/// One action of the registry.
#[derive(Debug, Clone, Copy)]
pub struct ActionSpec {
    pub name: ActionName,
    /// The file under `packages/excalidraw/actions/` and the line of its
    /// `name:`; `None` for the four union members nothing registers.
    pub source: Option<(&'static str, u32)>,
    pub label: Option<ActionLabel>,
    pub keywords: &'static [&'static str],
    pub icon: ActionIcon,
    /// `keyPriority` (0 when unset).
    pub key_priority: i32,
    /// `viewMode`: `Some(true)` runs in view mode; unset and `false` differ
    /// only to the command palette.
    pub view_mode: Option<bool>,
    /// `navigation`: stays available in a non-interactive editor that
    /// allows navigation.
    pub navigation: bool,
    pub track_event: Option<TrackEvent>,
    /// Has a `PanelComponent` (`renderAction` renders it).
    pub has_panel: bool,
    pub key_test: Option<KeyTestFn>,
    pub predicate: Option<PredicateFn>,
    pub checked: Option<CheckedFn>,
}

impl ActionSpec {
    /// Registered with the action manager (`register()` or, for undo and
    /// redo, `createUndoAction` / `createRedoAction`).
    pub fn is_registered(&self) -> bool {
        self.source.is_some()
    }

    /// The label's locale key under this state.
    pub fn label_key(&self, ctx: &ActionContext<'_>) -> Option<&'static str> {
        match self.label? {
            ActionLabel::Key(k) => Some(k),
            ActionLabel::Dynamic(f) => Some(f(ctx)),
        }
    }

    /// The icon's `icons.tsx` export under this state.
    pub fn icon_name(&self, ctx: &ActionContext<'_>) -> Option<&'static str> {
        match self.icon {
            ActionIcon::None => None,
            ActionIcon::Static(i) => Some(i),
            ActionIcon::Dynamic(f) => f(ctx),
        }
    }
}

impl ActionName {
    /// The action's registry entry.
    pub fn spec(self) -> &'static ActionSpec {
        &SPECS[self as usize]
    }
}

/// The registered actions, in `ActionName` order.
pub fn registered_actions() -> impl Iterator<Item = &'static ActionSpec> {
    SPECS.iter().filter(|s| s.is_registered())
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

const fn unregistered(name: ActionName) -> ActionSpec {
    ActionSpec {
        name,
        source: None,
        label: None,
        keywords: &[],
        icon: ActionIcon::None,
        key_priority: 0,
        view_mode: None,
        navigation: false,
        track_event: None,
        has_panel: false,
        key_test: None,
        predicate: None,
        checked: None,
    }
}

const fn action(
    name: ActionName,
    file: &'static str,
    line: u32,
    label: &'static str,
) -> ActionSpec {
    ActionSpec {
        source: Some((file, line)),
        label: Some(ActionLabel::Key(label)),
        ..unregistered(name)
    }
}

const fn track(category: &'static str) -> Option<TrackEvent> {
    Some(TrackEvent {
        category,
        action: None,
        predicate: None,
    })
}

const fn track_as(category: &'static str, action: &'static str) -> Option<TrackEvent> {
    Some(TrackEvent {
        category,
        action: Some(action),
        predicate: None,
    })
}

const fn track_if(category: &'static str, predicate: fn(&AppState) -> bool) -> Option<TrackEvent> {
    Some(TrackEvent {
        category,
        action: None,
        predicate: Some(predicate),
    })
}

const ZINDEX_DOWN: &[&str] = &["move down", "zindex", "layer"];
const ZINDEX_UP: &[&str] = &["move up", "zindex", "layer"];

use ActionName as N;

/// The registry, indexed by `ActionName as usize`.
static SPECS: [ActionSpec; 99] = [
    ActionSpec {
        icon: ActionIcon::Static("DuplicateIcon"),
        track_event: track("element"),
        ..action(N::Copy, "actionClipboard.tsx", 24, "labels.copy")
    },
    ActionSpec {
        icon: ActionIcon::Static("cutIcon"),
        track_event: track("element"),
        key_test: Some(kt_cut),
        ..action(N::Cut, "actionClipboard.tsx", 113, "labels.cut")
    },
    ActionSpec {
        track_event: track("element"),
        ..action(N::Paste, "actionClipboard.tsx", 56, "labels.paste")
    },
    ActionSpec {
        keywords: &["png", "clipboard", "copy"],
        icon: ActionIcon::Static("pngIcon"),
        track_event: track("element"),
        predicate: Some(p_copy_as_png),
        key_test: Some(kt_copy_as_png),
        ..action(N::CopyAsPng, "actionClipboard.tsx", 193, "labels.copyAsPng")
    },
    ActionSpec {
        keywords: &["svg", "clipboard", "copy"],
        icon: ActionIcon::Static("svgIcon"),
        track_event: track("element"),
        predicate: Some(p_copy_as_svg),
        ..action(N::CopyAsSvg, "actionClipboard.tsx", 125, "labels.copyAsSvg")
    },
    ActionSpec {
        keywords: &["text", "clipboard", "copy"],
        track_event: track("element"),
        predicate: Some(p_copy_text),
        ..action(N::CopyText, "actionClipboard.tsx", 255, "labels.copyText")
    },
    ActionSpec {
        keywords: ZINDEX_DOWN,
        icon: ActionIcon::Static("SendBackwardIcon"),
        track_event: track("element"),
        key_priority: 40,
        key_test: Some(kt_send_backward),
        has_panel: true,
        ..action(
            N::SendBackward,
            "actionZindex.tsx",
            24,
            "labels.sendBackward",
        )
    },
    ActionSpec {
        keywords: ZINDEX_UP,
        icon: ActionIcon::Static("BringForwardIcon"),
        track_event: track("element"),
        key_priority: 40,
        key_test: Some(kt_bring_forward),
        has_panel: true,
        ..action(
            N::BringForward,
            "actionZindex.tsx",
            54,
            "labels.bringForward",
        )
    },
    ActionSpec {
        keywords: ZINDEX_DOWN,
        icon: ActionIcon::Static("SendToBackIcon"),
        track_event: track("element"),
        key_test: Some(kt_send_to_back),
        has_panel: true,
        ..action(N::SendToBack, "actionZindex.tsx", 84, "labels.sendToBack")
    },
    ActionSpec {
        keywords: ZINDEX_UP,
        icon: ActionIcon::Static("BringToFrontIcon"),
        track_event: track("element"),
        key_test: Some(kt_bring_to_front),
        has_panel: true,
        ..action(
            N::BringToFront,
            "actionZindex.tsx",
            121,
            "labels.bringToFront",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("paintIcon"),
        track_event: track("element"),
        key_test: Some(kt_copy_styles),
        ..action(N::CopyStyles, "actionStyles.ts", 52, "labels.copyStyles")
    },
    ActionSpec {
        icon: ActionIcon::Static("selectAllIcon"),
        track_event: track("canvas"),
        view_mode: Some(false),
        key_test: Some(kt_select_all),
        ..action(N::SelectAll, "actionSelectAll.ts", 22, "labels.selectAll")
    },
    ActionSpec {
        icon: ActionIcon::Static("paintIcon"),
        track_event: track("element"),
        key_test: Some(kt_paste_styles),
        ..action(N::PasteStyles, "actionStyles.ts", 83, "labels.pasteStyles")
    },
    ActionSpec {
        keywords: &["snap"],
        icon: ActionIcon::Static("gridIcon"),
        view_mode: Some(true),
        track_event: track_if("canvas", grid_mode_enabled),
        checked: Some(grid_mode_enabled),
        predicate: Some(p_grid_prop_unset),
        key_test: Some(kt_grid_mode),
        ..action(
            N::GridMode,
            "actionToggleGridMode.tsx",
            10,
            "labels.toggleGrid",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("coffeeIcon"),
        view_mode: Some(true),
        track_event: track_if("canvas", zen_mode_disabled),
        checked: Some(zen_mode_enabled),
        predicate: Some(p_zen_mode),
        key_test: Some(kt_zen_mode),
        ..action(N::ZenMode, "actionToggleZenMode.tsx", 10, "buttons.zenMode")
    },
    ActionSpec {
        icon: ActionIcon::Static("magnetIcon"),
        view_mode: Some(false),
        track_event: track_if("canvas", objects_snap_disabled),
        checked: Some(objects_snap_enabled),
        predicate: Some(p_objects_snap_prop_unset),
        key_test: Some(kt_objects_snap_mode),
        ..action(
            N::ObjectsSnapMode,
            "actionToggleObjectsSnapMode.tsx",
            10,
            "buttons.objectsSnapMode",
        )
    },
    ActionSpec {
        view_mode: Some(false),
        track_event: track_if("canvas", binding_disabled),
        checked: Some(binding_enabled),
        ..action(
            N::ArrowBinding,
            "actionToggleArrowBinding.tsx",
            6,
            "labels.arrowBinding",
        )
    },
    ActionSpec {
        view_mode: Some(false),
        track_event: track_if("canvas", midpoint_snapping_disabled),
        checked: Some(midpoint_snapping_enabled),
        ..action(
            N::MidpointSnapping,
            "actionToggleMidpointSnapping.tsx",
            6,
            "labels.midpointSnapping",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("abacusIcon"),
        view_mode: Some(true),
        track_event: track("menu"),
        keywords: &["edit", "attributes", "customize"],
        checked: Some(stats_open),
        key_test: Some(kt_stats),
        ..action(N::Stats, "actionToggleStats.tsx", 10, "stats.fullTitle")
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeStrokeColor,
            "actionProperties.tsx",
            361,
            "labels.stroke",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeBackgroundColor,
            "actionProperties.tsx",
            440,
            "labels.changeBackground",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeBucketFillBackgroundColor,
            "actionProperties.tsx",
            556,
            "labels.changeBackground",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeFillStyle,
            "actionProperties.tsx",
            607,
            "labels.fill",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeStrokeWidth,
            "actionProperties.tsx",
            709,
            "labels.strokeWidth",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeSloppiness,
            "actionProperties.tsx",
            767,
            "labels.sloppiness",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeFreedrawMode,
            "actionProperties.tsx",
            821,
            "labels.pressure",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeStrokeStyle,
            "actionProperties.tsx",
            904,
            "labels.strokeStyle",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeArrowhead,
            "actionProperties.tsx",
            1948,
            "Change arrowheads",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeArrowType,
            "actionProperties.tsx",
            2058,
            "Change arrow types",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeArrowProperties,
            "actionProperties.tsx",
            2039,
            "Change arrow properties",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeOpacity,
            "actionProperties.tsx",
            957,
            "labels.opacity",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeFontSize,
            "actionProperties.tsx",
            1001,
            "labels.fontSize",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("UndoIcon"),
        track_event: track("history"),
        view_mode: Some(false),
        key_test: Some(kt_undo),
        has_panel: true,
        ..action(N::Undo, "actionHistory.tsx", 70, "buttons.undo")
    },
    ActionSpec {
        icon: ActionIcon::Static("RedoIcon"),
        track_event: track("history"),
        view_mode: Some(false),
        key_test: Some(kt_redo),
        has_panel: true,
        ..action(N::Redo, "actionHistory.tsx", 109, "buttons.redo")
    },
    ActionSpec {
        key_test: Some(kt_finalize),
        has_panel: true,
        ..action(N::Finalize, "actionFinalize.tsx", 54, "")
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeProjectName,
            "actionExport.tsx",
            38,
            "labels.fileTitle",
        )
    },
    ActionSpec {
        track_event: track_as("export", "toggleBackground"),
        has_panel: true,
        ..action(
            N::ChangeExportBackground,
            "actionExport.tsx",
            72,
            "imageExportDialog.label.withBackground",
        )
    },
    ActionSpec {
        track_event: track_as("export", "embedScene"),
        has_panel: true,
        ..action(
            N::ChangeExportEmbedScene,
            "actionExport.tsx",
            94,
            "imageExportDialog.tooltip.embedScene",
        )
    },
    ActionSpec {
        track_event: track_as("export", "scale"),
        ..action(
            N::ChangeExportScale,
            "actionExport.tsx",
            58,
            "imageExportDialog.scale",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("ExportIcon"),
        track_event: track("export"),
        predicate: Some(p_save_to_active_file),
        key_test: Some(kt_save_to_active_file),
        ..action(N::SaveToActiveFile, "actionExport.tsx", 254, "buttons.save")
    },
    ActionSpec {
        icon: ActionIcon::Static("ExportIcon"),
        view_mode: Some(true),
        track_event: track("export"),
        key_test: Some(kt_save_file_to_disk),
        has_panel: true,
        ..action(
            N::SaveFileToDisk,
            "actionExport.tsx",
            329,
            "exportDialog.disk_title",
        )
    },
    ActionSpec {
        track_event: track("export"),
        predicate: Some(p_load_scene),
        key_test: Some(kt_load_scene),
        ..action(N::LoadScene, "actionExport.tsx", 394, "buttons.load")
    },
    ActionSpec {
        icon: ActionIcon::Static("DuplicateIcon"),
        track_event: track("element"),
        key_test: Some(kt_duplicate_selection),
        has_panel: true,
        ..action(
            N::DuplicateSelection,
            "actionDuplicateSelection.tsx",
            35,
            "labels.duplicateSelection",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("TrashIcon"),
        track_event: track_as("element", "delete"),
        key_test: Some(kt_delete_selected),
        has_panel: true,
        ..action(
            N::DeleteSelectedElements,
            "actionDeleteSelected.tsx",
            209,
            "labels.delete",
        )
    },
    ActionSpec {
        predicate: Some(p_change_view_background_color),
        has_panel: true,
        ..action(
            N::ChangeViewBackgroundColor,
            "actionCanvas.tsx",
            48,
            "labels.canvasBackground",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("TrashIcon"),
        track_event: track("canvas"),
        predicate: Some(p_clear_canvas),
        ..action(N::ClearCanvas, "actionCanvas.tsx", 85, "labels.clearCanvas")
    },
    ActionSpec {
        icon: ActionIcon::Static("ZoomInIcon"),
        key_test: Some(kt_zoom_in),
        has_panel: true,
        ..zoom(N::ZoomIn, 130, "buttons.zoomIn")
    },
    ActionSpec {
        icon: ActionIcon::Static("ZoomOutIcon"),
        key_test: Some(kt_zoom_out),
        has_panel: true,
        ..zoom(N::ZoomOut, 182, "buttons.zoomOut")
    },
    ActionSpec {
        icon: ActionIcon::Static("ZoomResetIcon"),
        key_test: Some(kt_reset_zoom),
        has_panel: true,
        ..zoom(N::ResetZoom, 234, "buttons.resetZoom")
    },
    ActionSpec {
        icon: ActionIcon::Static("zoomAreaIcon"),
        key_test: Some(kt_zoom_to_fit),
        ..zoom(N::ZoomToFit, 393, "helpDialog.zoomToFit")
    },
    ActionSpec {
        icon: ActionIcon::Static("zoomAreaIcon"),
        key_test: Some(kt_zoom_to_fit_selection),
        ..zoom(N::ZoomToFitSelection, 351, "helpDialog.zoomToSelection")
    },
    ActionSpec {
        icon: ActionIcon::Static("zoomAreaIcon"),
        key_test: Some(kt_zoom_to_fit_selection_in_viewport),
        ..zoom(
            N::ZoomToFitSelectionInViewport,
            308,
            "labels.zoomToFitViewport",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeFontFamily,
            "actionProperties.tsx",
            1164,
            "labels.fontFamily",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeTextAlign,
            "actionProperties.tsx",
            1548,
            "Change text alignment",
        )
    },
    ActionSpec {
        track_event: track("element"),
        has_panel: true,
        ..action(
            N::ChangeVerticalAlign,
            "actionProperties.tsx",
            1649,
            "Change vertical alignment",
        )
    },
    unregistered(N::ToggleFullScreen),
    ActionSpec {
        icon: ActionIcon::Static("HelpIconThin"),
        view_mode: Some(true),
        track_event: track_as("menu", "toggleHelpDialog"),
        key_test: Some(kt_toggle_shortcuts),
        ..action(
            N::ToggleShortcuts,
            "actionMenu.tsx",
            10,
            "welcomeScreen.defaults.helpHint",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("GroupIcon"),
        track_event: track("element"),
        predicate: Some(p_group),
        key_test: Some(kt_group),
        has_panel: true,
        ..action(N::Group, "actionGroup.tsx", 87, "labels.group")
    },
    ActionSpec {
        icon: ActionIcon::Static("UngroupIcon"),
        track_event: track("element"),
        predicate: Some(p_ungroup),
        key_test: Some(kt_ungroup),
        has_panel: true,
        ..action(N::Ungroup, "actionGroup.tsx", 218, "labels.ungroup")
    },
    ActionSpec {
        view_mode: Some(true),
        track_event: track("collab"),
        has_panel: true,
        ..action(
            N::GoToCollaborator,
            "actionNavigate.tsx",
            22,
            "Go to a collaborator",
        )
    },
    ActionSpec {
        track_event: track("element"),
        ..action(
            N::AddToLibrary,
            "actionAddToLibrary.ts",
            11,
            "labels.addToLibrary",
        )
    },
    ActionSpec {
        has_panel: true,
        ..action(
            N::ChangeRoundness,
            "actionProperties.tsx",
            1749,
            "Change edge roundness",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("AlignTopIcon"),
        key_test: Some(kt_align_top),
        ..align(N::AlignTop, 80, "labels.alignTop")
    },
    ActionSpec {
        icon: ActionIcon::Static("AlignBottomIcon"),
        key_test: Some(kt_align_bottom),
        ..align(N::AlignBottom, 114, "labels.alignBottom")
    },
    ActionSpec {
        icon: ActionIcon::Static("AlignLeftIcon"),
        key_test: Some(kt_align_left),
        ..align(N::AlignLeft, 148, "labels.alignLeft")
    },
    ActionSpec {
        icon: ActionIcon::Static("AlignRightIcon"),
        key_test: Some(kt_align_right),
        ..align(N::AlignRight, 182, "labels.alignRight")
    },
    ActionSpec {
        icon: ActionIcon::Static("CenterVerticallyIcon"),
        ..align(N::AlignVerticallyCentered, 216, "labels.centerVertically")
    },
    ActionSpec {
        icon: ActionIcon::Static("CenterHorizontallyIcon"),
        ..align(
            N::AlignHorizontallyCentered,
            246,
            "labels.centerHorizontally",
        )
    },
    ActionSpec {
        track_event: track("element"),
        key_test: Some(kt_distribute_horizontally),
        has_panel: true,
        ..action(
            N::DistributeHorizontally,
            "actionDistribute.tsx",
            74,
            "labels.distributeHorizontally",
        )
    },
    ActionSpec {
        track_event: track("element"),
        key_test: Some(kt_distribute_vertically),
        has_panel: true,
        ..action(
            N::DistributeVertically,
            "actionDistribute.tsx",
            105,
            "labels.distributeVertically",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("flipHorizontal"),
        track_event: track("element"),
        key_test: Some(kt_flip_horizontal),
        ..action(
            N::FlipHorizontal,
            "actionFlip.ts",
            30,
            "labels.flipHorizontal",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("flipVertical"),
        track_event: track("element"),
        key_test: Some(kt_flip_vertical),
        ..action(N::FlipVertical, "actionFlip.ts", 55, "labels.flipVertical")
    },
    ActionSpec {
        key_test: Some(kt_deselect),
        ..action(N::Deselect, "actionDeselect.ts", 65, "")
    },
    ActionSpec {
        icon: ActionIcon::Static("eyeIcon"),
        view_mode: Some(true),
        track_event: track_if("canvas", view_mode_disabled),
        checked: Some(view_mode_enabled),
        predicate: Some(p_view_mode),
        key_test: Some(kt_view_mode),
        ..action(
            N::ViewMode,
            "actionToggleViewMode.tsx",
            10,
            "labels.viewMode",
        )
    },
    ActionSpec {
        track_event: track_as("export", "toggleTheme"),
        has_panel: true,
        ..action(
            N::ExportWithDarkMode,
            "actionExport.tsx",
            434,
            "imageExportDialog.label.darkMode",
        )
    },
    ActionSpec {
        label: Some(ActionLabel::Dynamic(label_toggle_theme)),
        keywords: &["toggle", "dark", "light", "mode", "theme"],
        icon: ActionIcon::Dynamic(icon_toggle_theme),
        view_mode: Some(true),
        track_event: track("canvas"),
        key_test: Some(kt_toggle_theme),
        predicate: Some(p_toggle_theme),
        ..action(N::ToggleTheme, "actionCanvas.tsx", 429, "")
    },
    ActionSpec {
        icon: ActionIcon::Static("fontSizeIcon"),
        key_test: Some(kt_increase_font_size),
        ..action(
            N::IncreaseFontSize,
            "actionProperties.tsx",
            1121,
            "labels.increaseFontSize",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("fontSizeIcon"),
        key_test: Some(kt_decrease_font_size),
        ..action(
            N::DecreaseFontSize,
            "actionProperties.tsx",
            1096,
            "labels.decreaseFontSize",
        )
    },
    ActionSpec {
        track_event: track("element"),
        predicate: Some(p_unbind_text),
        ..action(
            N::UnbindText,
            "actionBoundText.tsx",
            61,
            "labels.unbindText",
        )
    },
    ActionSpec {
        label: Some(ActionLabel::Dynamic(label_hyperlink)),
        icon: ActionIcon::Static("LinkIcon"),
        track_event: track_as("hyperlink", "click"),
        key_test: Some(kt_hyperlink),
        predicate: Some(p_hyperlink),
        has_panel: true,
        ..action(N::Hyperlink, "actionLink.tsx", 20, "")
    },
    ActionSpec {
        track_event: track("element"),
        predicate: Some(p_bind_text),
        ..action(N::BindText, "actionBoundText.tsx", 125, "labels.bindText")
    },
    ActionSpec {
        icon: ActionIcon::Static("UnlockedIcon"),
        track_event: track("canvas"),
        view_mode: Some(false),
        predicate: Some(p_unlock_all_elements),
        ..action(
            N::UnlockAllElements,
            "actionElementLock.ts",
            161,
            "labels.elementLock.unlockAll",
        )
    },
    ActionSpec {
        label: Some(ActionLabel::Dynamic(label_toggle_element_lock)),
        icon: ActionIcon::Dynamic(icon_toggle_element_lock),
        track_event: track("element"),
        predicate: Some(p_toggle_element_lock),
        key_test: Some(kt_toggle_element_lock),
        ..action(N::ToggleElementLock, "actionElementLock.ts", 26, "")
    },
    ActionSpec {
        label: Some(ActionLabel::Dynamic(label_toggle_linear_editor)),
        keywords: &["line"],
        track_event: track("element"),
        predicate: Some(p_toggle_linear_editor),
        has_panel: true,
        ..action(N::ToggleLinearEditor, "actionLinearEditor.tsx", 28, "")
    },
    ActionSpec {
        track_event: track("canvas"),
        predicate: Some(p_single_frame_selected),
        ..action(
            N::SelectAllElementsInFrame,
            "actionFrame.ts",
            37,
            "labels.selectAllElementsInFrame",
        )
    },
    ActionSpec {
        track_event: track("history"),
        predicate: Some(p_single_frame_selected),
        ..action(
            N::RemoveAllElementsFromFrame,
            "actionFrame.ts",
            74,
            "labels.removeAllElementsFromFrame",
        )
    },
    ActionSpec {
        view_mode: Some(true),
        track_event: track("canvas"),
        checked: Some(frame_rendering_enabled),
        ..action(
            N::UpdateFrameRendering,
            "actionFrame.ts",
            105,
            "labels.updateFrameRendering",
        )
    },
    unregistered(N::CreateContainerFromText),
    ActionSpec {
        track_event: track("element"),
        predicate: Some(p_wrap_text_in_container),
        ..action(
            N::WrapTextInContainer,
            "actionBoundText.tsx",
            259,
            "labels.createContainerFromText",
        )
    },
    unregistered(N::CommandPalette),
    ActionSpec {
        track_event: track("element"),
        predicate: Some(p_auto_resize),
        ..action(
            N::AutoResize,
            "actionTextAutoResize.ts",
            23,
            "labels.autoResize",
        )
    },
    unregistered(N::ElementStats),
    ActionSpec {
        keywords: &["search", "find"],
        icon: ActionIcon::Static("searchIcon"),
        view_mode: Some(true),
        track_event: Some(TrackEvent {
            category: "search_menu",
            action: Some("toggle"),
            predicate: Some(grid_mode_enabled),
        }),
        checked: Some(grid_mode_enabled),
        predicate: Some(p_grid_prop_unset),
        key_test: Some(kt_search_menu),
        ..action(
            N::SearchMenu,
            "actionToggleSearchMenu.ts",
            15,
            "search.title",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("copyIcon"),
        track_event: track("element"),
        predicate: Some(p_copy_element_link),
        ..action(
            N::CopyElementLink,
            "actionElementLink.ts",
            17,
            "labels.copyElementLink",
        )
    },
    ActionSpec {
        icon: ActionIcon::Static("elementLinkIcon"),
        predicate: Some(p_link_to_element),
        ..action(
            N::LinkToElement,
            "actionElementLink.ts",
            74,
            "labels.linkToElement",
        )
    },
    ActionSpec {
        keywords: &["image", "crop"],
        icon: ActionIcon::Static("cropIcon"),
        view_mode: Some(true),
        track_event: track("menu"),
        predicate: Some(p_crop_editor),
        has_panel: true,
        ..action(
            N::CropEditor,
            "actionCropEditor.tsx",
            14,
            "helpDialog.cropStart",
        )
    },
    ActionSpec {
        track_event: track("element"),
        predicate: Some(p_wrap_selection_in_frame),
        ..action(
            N::WrapSelectionInFrame,
            "actionFrame.ts",
            126,
            "labels.wrapSelectionInFrame",
        )
    },
    ActionSpec {
        keywords: &["change", "switch", "swap"],
        view_mode: Some(true),
        track_event: track_as("shape_switch", "toggle"),
        checked: Some(grid_mode_enabled),
        predicate: Some(p_toggle_shape_switch),
        ..action(
            N::ToggleShapeSwitch,
            "actionToggleShapeSwitch.tsx",
            14,
            "labels.shapeSwitch",
        )
    },
    ActionSpec {
        label: Some(ActionLabel::Dynamic(label_toggle_polygon)),
        keywords: &["loop"],
        icon: ActionIcon::Static("polygonIcon"),
        track_event: track("element"),
        predicate: Some(p_toggle_polygon),
        has_panel: true,
        ..action(N::TogglePolygon, "actionLinearEditor.tsx", 107, "")
    },
];

/// The zoom actions' shared fields (`actionCanvas.tsx`).
const fn zoom(name: ActionName, line: u32, label: &'static str) -> ActionSpec {
    ActionSpec {
        view_mode: Some(true),
        navigation: true,
        track_event: track("canvas"),
        predicate: Some(p_navigation_enabled),
        ..action(name, "actionCanvas.tsx", line, label)
    }
}

/// The align actions' shared fields (`actionAlign.tsx`).
const fn align(name: ActionName, line: u32, label: &'static str) -> ActionSpec {
    ActionSpec {
        track_event: track("element"),
        predicate: Some(p_align),
        has_panel: true,
        ..action(name, "actionAlign.tsx", line, label)
    }
}

// ---------------------------------------------------------------------------
// App state reads (checked, trackEvent.predicate)
// ---------------------------------------------------------------------------

fn flag(app_state: &AppState, key: &str) -> bool {
    truthy(app_state.get(key))
}

fn grid_mode_enabled(s: &AppState) -> bool {
    flag(s, "gridModeEnabled")
}
fn zen_mode_enabled(s: &AppState) -> bool {
    flag(s, "zenModeEnabled")
}
fn zen_mode_disabled(s: &AppState) -> bool {
    !zen_mode_enabled(s)
}
fn view_mode_enabled(s: &AppState) -> bool {
    flag(s, "viewModeEnabled")
}
fn view_mode_disabled(s: &AppState) -> bool {
    !view_mode_enabled(s)
}
fn objects_snap_enabled(s: &AppState) -> bool {
    flag(s, "objectsSnapModeEnabled")
}
fn objects_snap_disabled(s: &AppState) -> bool {
    !objects_snap_enabled(s)
}
fn binding_enabled(s: &AppState) -> bool {
    s.get("bindingPreference").and_then(Value::as_str) == Some("enabled")
}
fn binding_disabled(s: &AppState) -> bool {
    s.get("bindingPreference").and_then(Value::as_str) == Some("disabled")
}
fn midpoint_snapping_enabled(s: &AppState) -> bool {
    flag(s, "isMidpointSnappingEnabled")
}
fn midpoint_snapping_disabled(s: &AppState) -> bool {
    !midpoint_snapping_enabled(s)
}
fn stats_open(s: &AppState) -> bool {
    truthy(s.get("stats").and_then(|v| v.get("open")))
}
fn frame_rendering_enabled(s: &AppState) -> bool {
    truthy(s.get("frameRendering").and_then(|v| v.get("enabled")))
}

// ---------------------------------------------------------------------------
// keyTest
// ---------------------------------------------------------------------------

fn cmd(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    ev.ctrl_or_cmd(ctx.env.is_darwin)
}

fn zoom_key(action: ZoomAction, ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    action.key_test(&ZoomKeyEvent {
        code: ev.code,
        shift_key: ev.shift_key,
        alt_key: ev.alt_key,
        ctrl_or_cmd: cmd(ev, ctx),
    })
}

fn kt_zoom_in(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    zoom_key(ZoomAction::ZoomIn, ev, ctx)
}
fn kt_zoom_out(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    zoom_key(ZoomAction::ZoomOut, ev, ctx)
}
fn kt_reset_zoom(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    zoom_key(ZoomAction::ResetZoom, ev, ctx)
}
fn kt_zoom_to_fit(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    zoom_key(ZoomAction::ZoomToFit, ev, ctx)
}
fn kt_zoom_to_fit_selection(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    zoom_key(ZoomAction::ZoomToFitSelection, ev, ctx)
}
fn kt_zoom_to_fit_selection_in_viewport(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    zoom_key(ZoomAction::ZoomToFitSelectionInViewport, ev, ctx)
}

/// `actionAlign.tsx:96-97` and siblings.
fn align_key(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>, key: &str) -> bool {
    cmd(ev, ctx) && ev.shift_key && ev.key == key
}
fn kt_align_top(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    align_key(ev, ctx, "ArrowUp")
}
fn kt_align_bottom(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    align_key(ev, ctx, "ArrowDown")
}
fn kt_align_left(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    align_key(ev, ctx, "ArrowLeft")
}
fn kt_align_right(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    align_key(ev, ctx, "ArrowRight")
}

/// `actionCanvas.tsx:457-461`.
fn kt_toggle_theme(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    !cmd(ev, ctx) && ev.alt_key && ev.shift_key && ev.code == "KeyD"
}

/// `actionClipboard.tsx:121`.
fn kt_cut(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && ev.key == "x"
}

/// `actionClipboard.tsx:250`.
fn kt_copy_as_png(ev: &KeyEvent<'_>, _: &ActionContext<'_>) -> bool {
    ev.code == "KeyC" && ev.alt_key && ev.shift_key
}

/// `actionDeleteSelected.tsx:305-307`.
fn kt_delete_selected(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    (ev.key == "Backspace" || ev.key == "Delete") && !cmd(ev, ctx)
}

/// `actionDeselect.ts:130-149`.
fn kt_deselect(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    if ev.key != "Escape" || ev.target_is_writable {
        return false;
    }
    let active_tool = ctx.field("activeTool", "type");
    let preferred = ctx.field("preferredSelectionTool", "type");
    !ctx.flag("newElement")
        && ctx.is_null("multiElement")
        && !truthy(ctx.field("selectedLinearElement", "isEditing"))
        && (!ctx.is_null("activeEmbeddable")
            || active_tool != preferred
            || ctx.flag("editingGroupId")
            || ctx.flag("selectedLinearElement")
            || ctx.is_some_element_selected())
}

/// `actionDistribute.tsx:87-88`.
fn kt_distribute_horizontally(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    !cmd(ev, ctx) && ev.alt_key && ev.code == "KeyH"
}
/// `actionDistribute.tsx:118-119`.
fn kt_distribute_vertically(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    !cmd(ev, ctx) && ev.alt_key && ev.code == "KeyV"
}

/// `actionDuplicateSelection.tsx:117`.
fn kt_duplicate_selection(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && ev.key == "d"
}

/// `actionElementLock.ts:147-157`.
fn kt_toggle_element_lock(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    ev.key.to_lowercase() == "l" && cmd(ev, ctx) && ev.shift_key && !ctx.selected(false).is_empty()
}

/// `actionExport.tsx:324-325`.
fn kt_save_to_active_file(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    ev.key == "s" && cmd(ev, ctx) && !ev.shift_key
}
/// `actionExport.tsx:375-378`.
fn kt_save_file_to_disk(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    ev.key.to_lowercase() == "s" && ev.shift_key && cmd(ev, ctx)
}
/// `actionExport.tsx:428`.
fn kt_load_scene(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && ev.key == "o"
}

/// `actionFinalize.tsx:421-424`.
fn kt_finalize(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    (ev.key == "Escape" && truthy(ctx.field("selectedLinearElement", "isEditing")))
        || ((ev.key == "Escape" || ev.key == "Enter")
            && !matches!(ctx.get("multiElement"), Some(Value::Null)))
}

/// `actionFlip.ts:51`.
fn kt_flip_horizontal(ev: &KeyEvent<'_>, _: &ActionContext<'_>) -> bool {
    ev.shift_key && ev.code == "KeyH"
}
/// `actionFlip.ts:76-77`.
fn kt_flip_vertical(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    ev.shift_key && ev.code == "KeyV" && !cmd(ev, ctx)
}

/// `actionGroup.tsx:202-203`.
fn kt_group(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    !ev.shift_key && cmd(ev, ctx) && ev.key == "g"
}
/// `actionGroup.tsx:306-309`.
fn kt_ungroup(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    ev.shift_key && cmd(ev, ctx) && ev.key == "G"
}

/// `actionHistory.tsx:79-80`.
fn kt_undo(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && match_key(ev, "z") && !ev.shift_key
}
/// `actionHistory.tsx:118-120`.
fn kt_redo(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    (cmd(ev, ctx) && ev.shift_key && match_key(ev, "z"))
        || (cmd(ev, ctx) && !ev.shift_key && match_key(ev, "y"))
}

/// `actionLink.tsx:40`.
fn kt_hyperlink(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && ev.key == "k"
}

/// `actionMenu.tsx:34`.
fn kt_toggle_shortcuts(ev: &KeyEvent<'_>, _: &ActionContext<'_>) -> bool {
    ev.key == "?"
}

/// `actionProperties.tsx:1110-1117` (`,` for macOS).
fn kt_decrease_font_size(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && ev.shift_key && (ev.key == "<" || ev.key == ",")
}
/// `actionProperties.tsx:1133-1140` (`.` for macOS).
fn kt_increase_font_size(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && ev.shift_key && (ev.key == ">" || ev.key == ".")
}

/// `actionSelectAll.ts:69`.
fn kt_select_all(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && ev.key == "a"
}

/// `actionStyles.ts:78-79`.
fn kt_copy_styles(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && ev.alt_key && ev.code == "KeyC"
}
/// `actionStyles.ts:229-230`.
fn kt_paste_styles(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && ev.alt_key && ev.code == "KeyV"
}

/// `actionToggleGridMode.tsx:33`.
fn kt_grid_mode(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && ev.code == "Quote"
}
/// `actionToggleObjectsSnapMode.tsx:32-33`.
fn kt_objects_snap_mode(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    !cmd(ev, ctx) && ev.alt_key && ev.code == "KeyS"
}
/// `actionToggleSearchMenu.ts:57`.
fn kt_search_menu(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && ev.key == "f"
}
/// `actionToggleStats.tsx:26-27`.
fn kt_stats(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    !cmd(ev, ctx) && ev.alt_key && ev.code == "Slash"
}
/// `actionToggleViewMode.tsx:34-35`.
fn kt_view_mode(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    !cmd(ev, ctx) && ev.alt_key && ev.code == "KeyR"
}
/// `actionToggleZenMode.tsx:34-35`.
fn kt_zen_mode(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    !cmd(ev, ctx) && ev.alt_key && ev.code == "KeyZ"
}

/// `actionZindex.tsx:37-40`.
fn kt_send_backward(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && !ev.shift_key && ev.code == "BracketLeft"
}
/// `actionZindex.tsx:67-70`.
fn kt_bring_forward(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    cmd(ev, ctx) && !ev.shift_key && ev.code == "BracketRight"
}
/// `actionZindex.tsx:96-103`: Cmd+Alt on a Mac, Ctrl+Shift elsewhere.
fn kt_send_to_back(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    z_extreme(ev, ctx, "BracketLeft")
}
/// `actionZindex.tsx:134-141`.
fn kt_bring_to_front(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> bool {
    z_extreme(ev, ctx, "BracketRight")
}
fn z_extreme(ev: &KeyEvent<'_>, ctx: &ActionContext<'_>, code: &str) -> bool {
    let second = if ctx.env.is_darwin {
        ev.alt_key
    } else {
        ev.shift_key
    };
    cmd(ev, ctx) && second && ev.code == code
}

// ---------------------------------------------------------------------------
// predicate
// ---------------------------------------------------------------------------

/// `app.isNavigationEnabled()` (the zoom actions).
fn p_navigation_enabled(ctx: &ActionContext<'_>) -> bool {
    ctx.env.navigation_enabled
}

/// `alignActionsPredicate` (`actionAlign.tsx:36-52`): more than one unit
/// selected and no frame.
fn p_align(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    ctx.selected_units(&selected) > 1 && !selected.iter().any(|e| is_frame_like(e))
}

/// `actionBoundText.tsx:64-68`.
fn p_unbind_text(ctx: &ActionContext<'_>) -> bool {
    ctx.selected(false)
        .iter()
        .any(|e| has_bound_text_element(e))
}

/// `actionBoundText.tsx:128-154`: a text and a container without a label.
fn p_bind_text(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    if selected.len() != 2 {
        return false;
    }
    let text = selected.iter().any(|e| is_type(e, ElementType::Text));
    let container = selected
        .iter()
        .find(|e| is_text_bindable_container(e))
        .copied();
    text && container.is_some_and(|c| ctx.bound_text_of(c).is_none())
}

/// `actionBoundText.tsx:262-268`.
fn p_wrap_text_in_container(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    !selected.is_empty()
        && selected
            .iter()
            .any(|e| is_type(e, ElementType::Text) && !is_bound_to_container(e))
}

/// `actionCanvas.tsx:51-56`.
fn p_change_view_background_color(ctx: &ActionContext<'_>) -> bool {
    ctx.props.canvas_actions.change_view_background_color && !ctx.flag("viewModeEnabled")
}

/// `actionCanvas.tsx:89-95`.
fn p_clear_canvas(ctx: &ActionContext<'_>) -> bool {
    ctx.props.canvas_actions.clear_canvas
        && !ctx.flag("viewModeEnabled")
        && ctx.open_dialog_name() != Some("elementLinkSelector")
}

/// `actionCanvas.tsx:462-464`.
fn p_toggle_theme(ctx: &ActionContext<'_>) -> bool {
    ctx.props.canvas_actions.toggle_theme == Some(true)
}

/// `actionClipboard.tsx:186-188`.
fn p_copy_as_svg(ctx: &ActionContext<'_>) -> bool {
    ctx.env.clipboard_write_text && !ctx.elements.is_empty()
}

/// `actionClipboard.tsx:247-249`.
fn p_copy_as_png(ctx: &ActionContext<'_>) -> bool {
    ctx.env.clipboard_blob && !ctx.elements.is_empty()
}

/// `actionClipboard.tsx:273-283`.
fn p_copy_text(ctx: &ActionContext<'_>) -> bool {
    ctx.env.clipboard_write_text
        && ctx
            .selected(true)
            .iter()
            .any(|e| is_type(e, ElementType::Text))
}

/// `actionCropEditor.tsx:35-45`.
fn p_crop_editor(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    !ctx.flag("croppingElementId")
        && selected.len() == 1
        && is_type(selected[0], ElementType::Image)
}

/// `actionElementLink.ts:69-70`.
fn p_copy_element_link(ctx: &ActionContext<'_>) -> bool {
    can_create_link_from_elements(&ctx.selected(false))
}

/// `actionElementLink.ts:103-111`.
fn p_link_to_element(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    ctx.open_dialog_name() != Some("elementLinkSelector")
        && selected.len() == 1
        && can_create_link_from_elements(&selected)
}

/// `actionElementLock.ts:42-48`.
fn p_toggle_element_lock(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    !selected.is_empty()
        && !selected
            .iter()
            .any(|e| e.base.locked && e.base.frame_id.as_deref().is_some_and(|f| !f.is_empty()))
}

/// `actionElementLock.ts:165-171`.
fn p_unlock_all_elements(ctx: &ActionContext<'_>) -> bool {
    ctx.selected(false).is_empty() && ctx.elements.iter().any(|e| e.base.locked)
}

/// `actionExport.tsx:258-264`.
fn p_save_to_active_file(ctx: &ActionContext<'_>) -> bool {
    ctx.props.canvas_actions.save_to_active_file
        && ctx.flag("fileHandle")
        && !ctx.flag("viewModeEnabled")
}

/// `actionExport.tsx:397-401`.
fn p_load_scene(ctx: &ActionContext<'_>) -> bool {
    ctx.props.canvas_actions.load_scene && !ctx.flag("viewModeEnabled")
}

/// `isSingleFrameSelected` (`actionFrame.ts:25-35`).
fn p_single_frame_selected(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    selected.len() == 1 && is_frame_like(selected[0])
}

/// `actionFrame.ts:129-136`.
fn p_wrap_selection_in_frame(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    !selected.is_empty() && !selected.iter().any(|e| is_frame_like(e))
}

/// `enableActionGroup` (`actionGroup.tsx:73-84`).
fn p_group(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    selected.len() >= 2
        && !ctx.all_in_same_group(&selected)
        && !frame_and_children_selected_together(&selected)
}

/// `actionGroup.tsx:310`.
fn p_ungroup(ctx: &ActionContext<'_>) -> bool {
    !ctx.selected_group_ids().is_empty()
}

/// `actionLinearEditor.tsx:43-54`.
fn p_toggle_linear_editor(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    !truthy(ctx.field("selectedLinearElement", "isEditing"))
        && selected.len() == 1
        && selected[0].element_type().is_linear()
        && !matches!(&selected[0].kind, ElementKind::Arrow(a) if a.elbowed)
}

/// `actionLinearEditor.tsx:127-138`.
fn p_toggle_polygon(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    !selected.is_empty()
        && selected
            .iter()
            .all(|e| matches!(&e.kind, ElementKind::Line(l) if l.linear.points.len() >= 4))
}

/// `actionLink.tsx:41-44`.
fn p_hyperlink(ctx: &ActionContext<'_>) -> bool {
    ctx.selected(false).len() == 1
}

/// `actionTextAutoResize.ts:27-34`.
fn p_auto_resize(ctx: &ActionContext<'_>) -> bool {
    let selected = ctx.selected(false);
    selected.len() == 1 && matches!(&selected[0].kind, ElementKind::Text(t) if !t.auto_resize)
}

/// `actionToggleGridMode.tsx:30-32` and `actionToggleSearchMenu.ts:54-56`.
fn p_grid_prop_unset(ctx: &ActionContext<'_>) -> bool {
    ctx.props.grid_mode_enabled.is_none()
}

/// `actionToggleObjectsSnapMode.tsx:29-31`.
fn p_objects_snap_prop_unset(ctx: &ActionContext<'_>) -> bool {
    ctx.props.objects_snap_mode_enabled.is_none()
}

/// `actionToggleViewMode.tsx:28-33`.
fn p_view_mode(ctx: &ActionContext<'_>) -> bool {
    ctx.props.view_mode_enabled.is_none() && ctx.env.interaction_enabled
}

/// `actionToggleZenMode.tsx:28-33`.
fn p_zen_mode(ctx: &ActionContext<'_>) -> bool {
    ctx.env.form_factor != FormFactor::Phone && ctx.props.zen_mode_enabled.is_none()
}

/// `getConversionTypeFromElements(elements) !== null`
/// (`ConvertElementTypePopup.tsx:641-672`), over every element passed in.
fn p_toggle_shape_switch(ctx: &ActionContext<'_>) -> bool {
    ctx.elements.iter().any(|e| {
        matches!(
            e.element_type(),
            ElementType::Rectangle | ElementType::Diamond | ElementType::Ellipse
        ) || match &e.kind {
            ElementKind::Line(_) => true,
            ElementKind::Arrow(a) => {
                a.linear.start_binding.is_none()
                    && a.linear.end_binding.is_none()
                    && !has_bound_text_element(e)
            }
            _ => false,
        }
    })
}

// ---------------------------------------------------------------------------
// Dynamic labels and icons
// ---------------------------------------------------------------------------

/// `actionCanvas.tsx:430-434`.
fn label_toggle_theme(ctx: &ActionContext<'_>) -> &'static str {
    if ctx.get("theme").and_then(Value::as_str) == Some("dark") {
        "buttons.lightMode"
    } else {
        "buttons.darkMode"
    }
}

/// `actionCanvas.tsx:436-437`.
fn icon_toggle_theme(ctx: &ActionContext<'_>) -> Option<&'static str> {
    Some(
        if ctx.get("theme").and_then(Value::as_str) == Some("light") {
            "MoonIcon"
        } else {
            "SunIcon"
        },
    )
}

/// `shouldLock` (`actionElementLock.ts:22-23`).
fn should_lock(ctx: &ActionContext<'_>) -> bool {
    ctx.selected(false).iter().all(|e| !e.base.locked)
}

/// `actionElementLock.ts:27-36`.
fn label_toggle_element_lock(ctx: &ActionContext<'_>) -> &'static str {
    if should_lock(ctx) {
        "labels.elementLock.lock"
    } else {
        "labels.elementLock.unlock"
    }
}

/// `actionElementLock.ts:37-40`.
fn icon_toggle_element_lock(ctx: &ActionContext<'_>) -> Option<&'static str> {
    Some(if should_lock(ctx) {
        "LockedIcon"
    } else {
        "UnlockedIcon"
    })
}

/// `actionLinearEditor.tsx:30-38`.
fn label_toggle_linear_editor(ctx: &ActionContext<'_>) -> &'static str {
    match ctx.selected(false).first() {
        Some(e) if is_type(e, ElementType::Arrow) => "labels.lineEditor.editArrow",
        _ => "labels.lineEditor.edit",
    }
}

/// `actionLinearEditor.tsx:111-123`.
fn label_toggle_polygon(ctx: &ActionContext<'_>) -> &'static str {
    let all_polygons = ctx
        .selected(false)
        .iter()
        .all(|e| matches!(&e.kind, ElementKind::Line(l) if l.polygon));
    if all_polygons {
        "labels.polygon.breakPolygon"
    } else {
        "labels.polygon.convertToPolygon"
    }
}

/// `getContextMenuLabel` (`Hyperlink.tsx:371-382`).
fn label_hyperlink(ctx: &ActionContext<'_>) -> &'static str {
    let selected = ctx.selected(false);
    match selected.first() {
        Some(e) if is_type(e, ElementType::Embeddable) => "labels.link.editEmbed",
        Some(e) if e.base.link.as_deref().is_some_and(|l| !l.is_empty()) => "labels.link.edit",
        _ => "labels.link.create",
    }
}
