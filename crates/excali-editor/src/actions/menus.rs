//! The menus and panels generated from the registry: the canvas and
//! element context menus (`App.tsx:13835-13936`, `ContextMenu.tsx`), the
//! command palette's commands built from actions (`CommandPalette.tsx`),
//! the full styles panel (`Actions.tsx:63-217`) and the default main menu
//! (`LayerUI.tsx:111-136`, `main-menu/DefaultItems.tsx`).

use super::context::{ActionContext, FormFactor};
use super::keys::{get_shortcut_from_shortcut_name, KeyLabels};
use super::manager::ActionManager;
use super::names::ActionName;

// ---------------------------------------------------------------------------
// Context menu
// ---------------------------------------------------------------------------

/// Which context menu (`getContextMenuItems(type)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuKind {
    Canvas,
    Element,
}

/// A context menu item before filtering (`ContextMenuItem`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuItem {
    /// `CONTEXT_MENU_SEPARATOR`.
    Separator,
    Action(ActionName),
}

/// A rendered context menu row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuEntry {
    /// `<hr class="context-menu-item-separator">`.
    Separator,
    Item(ContextMenuRow),
}

/// A clickable context menu row (`ContextMenu.tsx:88-117`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenuRow {
    /// Also the row's `data-testid`.
    pub name: ActionName,
    /// The label's locale key.
    pub label: &'static str,
    /// `getShortcutFromShortcutName(name)`.
    pub shortcut: String,
    /// `checkmark`: `action.checked(appState)`.
    pub checked: bool,
    /// `dangerous`: the delete row.
    pub dangerous: bool,
}

/// `App.getContextMenuItems(type)` (`App.tsx:13835-13936`).
pub fn get_context_menu_items(
    kind: ContextMenuKind,
    view_mode_enabled: bool,
    form_factor: FormFactor,
) -> Vec<ContextMenuItem> {
    use ActionName as N;
    use ContextMenuItem::{Action as A, Separator as S};
    let exports = [A(N::CopyAsPng), A(N::CopyAsSvg)];
    match kind {
        ContextMenuKind::Canvas if view_mode_enabled => exports
            .into_iter()
            .chain([A(N::GridMode), A(N::ZenMode), A(N::ViewMode), A(N::Stats)])
            .collect(),
        ContextMenuKind::Canvas => vec![
            A(N::Paste),
            S,
            A(N::CopyAsPng),
            A(N::CopyAsSvg),
            A(N::CopyText),
            S,
            A(N::SelectAll),
            A(N::UnlockAllElements),
            S,
            A(N::GridMode),
            A(N::ObjectsSnapMode),
            A(N::ArrowBinding),
            A(N::MidpointSnapping),
            A(N::ZenMode),
            A(N::ViewMode),
            A(N::Stats),
        ],
        ContextMenuKind::Element if view_mode_enabled => {
            let mut items = vec![A(N::Copy)];
            items.extend(exports);
            items.push(A(N::CopyText));
            items
        }
        ContextMenuKind::Element => {
            let mut items = vec![
                S,
                A(N::Cut),
                A(N::Copy),
                A(N::Paste),
                S,
                A(N::SelectAllElementsInFrame),
                A(N::RemoveAllElementsFromFrame),
                A(N::WrapSelectionInFrame),
                S,
                A(N::CropEditor),
                S,
            ];
            items.extend(exports);
            items.extend([
                A(N::CopyText),
                S,
                A(N::CopyStyles),
                A(N::PasteStyles),
                S,
                A(N::Group),
                A(N::AutoResize),
                A(N::UnbindText),
                A(N::BindText),
                A(N::WrapTextInContainer),
                A(N::Ungroup),
                S,
                A(N::AddToLibrary),
            ]);
            if form_factor == FormFactor::Desktop {
                items.extend([
                    S,
                    A(N::SendBackward),
                    A(N::BringForward),
                    A(N::SendToBack),
                    A(N::BringToFront),
                ]);
            }
            items.extend([
                S,
                A(N::FlipHorizontal),
                A(N::FlipVertical),
                S,
                A(N::ToggleLinearEditor),
                S,
                A(N::Hyperlink),
                A(N::CopyElementLink),
                S,
                A(N::DuplicateSelection),
                A(N::ToggleElementLock),
                S,
                A(N::DeleteSelectedElements),
            ]);
            items
        }
    }
}

/// `ContextMenu` (`ContextMenu.tsx:37-120`): drop the items whose
/// predicate fails, drop a separator that would lead or follow another,
/// and resolve labels, shortcuts and checkmarks. `ctx.elements` should be
/// the non-deleted elements, as `useExcalidrawElements()` gives.
pub fn build_context_menu(
    items: &[ContextMenuItem],
    ctx: &ActionContext<'_>,
    labels: &KeyLabels<'_>,
) -> Vec<ContextMenuEntry> {
    let filtered: Vec<ContextMenuItem> = items
        .iter()
        .copied()
        .filter(|item| match item {
            ContextMenuItem::Separator => true,
            ContextMenuItem::Action(a) => a.spec().predicate.is_none_or(|p| p(ctx)),
        })
        .collect();
    let mut entries = Vec::new();
    for (i, item) in filtered.iter().enumerate() {
        match item {
            ContextMenuItem::Separator => {
                if i == 0 || filtered[i - 1] == ContextMenuItem::Separator {
                    continue;
                }
                entries.push(ContextMenuEntry::Separator);
            }
            ContextMenuItem::Action(name) => {
                let spec = name.spec();
                entries.push(ContextMenuEntry::Item(ContextMenuRow {
                    name: *name,
                    label: spec.label_key(ctx).unwrap_or(""),
                    shortcut: get_shortcut_from_shortcut_name(
                        name.as_str(),
                        0,
                        ctx.env.is_darwin,
                        labels,
                    ),
                    checked: spec.checked.is_some_and(|c| c(ctx.app_state)),
                    dangerous: *name == ActionName::DeleteSelectedElements,
                }));
            }
        }
    }
    entries
}

// ---------------------------------------------------------------------------
// Command palette
// ---------------------------------------------------------------------------

/// `DEFAULT_CATEGORIES` (`CommandPalette.tsx:87-95`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCategory {
    App,
    Export,
    Tools,
    Editor,
    Elements,
    Links,
    Library,
}

impl PaletteCategory {
    /// The category's name.
    pub fn as_str(self) -> &'static str {
        match self {
            PaletteCategory::App => "App",
            PaletteCategory::Export => "Export",
            PaletteCategory::Tools => "Tools",
            PaletteCategory::Editor => "Editor",
            PaletteCategory::Elements => "Elements",
            PaletteCategory::Links => "Links",
            PaletteCategory::Library => "Library",
        }
    }

    /// `getCategoryOrder` (`CommandPalette.tsx:97-114`).
    pub fn order(self) -> u32 {
        match self {
            PaletteCategory::App => 1,
            PaletteCategory::Export => 2,
            PaletteCategory::Editor => 3,
            PaletteCategory::Tools => 4,
            PaletteCategory::Elements => 5,
            PaletteCategory::Links => 6,
            PaletteCategory::Library => 10,
        }
    }
}

/// What a palette command built from actions does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommandSource {
    /// `actionManager.executeAction(action, "commandPalette")`.
    Action(ActionName),
    /// Opens the clear-canvas confirm dialog, labelled as `clearCanvas`.
    ClearCanvas,
    /// Opens the image export dialog.
    ImageExport,
}

/// A command palette command built from the registry
/// (`CommandPalette.tsx:290-424`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteActionCommand {
    pub source: PaletteCommandSource,
    pub category: PaletteCategory,
    /// Elements commands whose action has no predicate need a selection
    /// (`CommandPalette.tsx:341-355`).
    pub selection_fallback: bool,
}

impl PaletteActionCommand {
    /// The label's locale key.
    pub fn label_key(&self, ctx: &ActionContext<'_>) -> &'static str {
        match self.source {
            PaletteCommandSource::Action(a) => a.spec().label_key(ctx).unwrap_or(""),
            PaletteCommandSource::ClearCanvas => {
                ActionName::ClearCanvas.spec().label_key(ctx).unwrap_or("")
            }
            PaletteCommandSource::ImageExport => "buttons.exportImage",
        }
    }

    /// The shortcut hint.
    pub fn shortcut(&self, is_darwin: bool, labels: &KeyLabels<'_>) -> String {
        let name = match self.source {
            PaletteCommandSource::Action(a) => a.as_str(),
            PaletteCommandSource::ClearCanvas => "clearCanvas",
            PaletteCommandSource::ImageExport => "imageExport",
        };
        get_shortcut_from_shortcut_name(name, 0, is_darwin, labels)
    }

    /// The search keywords.
    pub fn keywords(&self) -> &'static [&'static str] {
        match self.source {
            PaletteCommandSource::Action(a) => a.spec().keywords,
            PaletteCommandSource::ClearCanvas => &["delete", "destroy"],
            PaletteCommandSource::ImageExport => &[
                "export",
                "image",
                "png",
                "jpeg",
                "svg",
                "clipboard",
                "picture",
            ],
        }
    }

    /// `viewMode`: `Some(false)` hides the command in view mode.
    pub fn view_mode(&self) -> Option<bool> {
        match self.source {
            PaletteCommandSource::Action(a) => a.spec().view_mode,
            PaletteCommandSource::ClearCanvas => Some(false),
            PaletteCommandSource::ImageExport => None,
        }
    }
}

/// The palette's `commandsFromActions` followed by the theme toggle, the
/// first of `additionalCommands` (`CommandPalette.tsx:310-424`).
pub fn commands_from_actions() -> Vec<PaletteActionCommand> {
    use ActionName as N;
    const ELEMENTS: [ActionName; 31] = [
        N::Group,
        N::Ungroup,
        N::Cut,
        N::Copy,
        N::DeleteSelectedElements,
        N::WrapSelectionInFrame,
        N::CopyStyles,
        N::PasteStyles,
        N::BringToFront,
        N::BringForward,
        N::SendBackward,
        N::SendToBack,
        N::AlignTop,
        N::AlignBottom,
        N::AlignLeft,
        N::AlignRight,
        N::AlignVerticallyCentered,
        N::AlignHorizontallyCentered,
        N::DuplicateSelection,
        N::FlipHorizontal,
        N::FlipVertical,
        N::ZoomToFitSelection,
        N::ZoomToFitSelectionInViewport,
        N::IncreaseFontSize,
        N::DecreaseFontSize,
        N::ToggleLinearEditor,
        N::CropEditor,
        N::TogglePolygon,
        N::Hyperlink,
        N::CopyElementLink,
        N::LinkToElement,
    ];
    const EDITOR: [ActionName; 15] = [
        N::Undo,
        N::Redo,
        N::ZoomIn,
        N::ZoomOut,
        N::ResetZoom,
        N::ZoomToFit,
        N::ZenMode,
        N::ViewMode,
        N::GridMode,
        N::ObjectsSnapMode,
        N::ToggleShortcuts,
        N::SelectAll,
        N::ToggleElementLock,
        N::UnlockAllElements,
        N::Stats,
    ];
    const EXPORT: [ActionName; 4] = [
        N::SaveToActiveFile,
        N::SaveFileToDisk,
        N::CopyAsPng,
        N::CopyAsSvg,
    ];
    let action = |a: ActionName, category, selection_fallback| PaletteActionCommand {
        source: PaletteCommandSource::Action(a),
        category,
        selection_fallback,
    };
    let mut commands: Vec<PaletteActionCommand> = ELEMENTS
        .into_iter()
        .map(|a| action(a, PaletteCategory::Elements, a.spec().predicate.is_none()))
        .collect();
    commands.extend(
        EDITOR
            .into_iter()
            .map(|a| action(a, PaletteCategory::Editor, false)),
    );
    commands.push(PaletteActionCommand {
        source: PaletteCommandSource::ClearCanvas,
        category: PaletteCategory::Editor,
        selection_fallback: false,
    });
    commands.push(PaletteActionCommand {
        source: PaletteCommandSource::ImageExport,
        category: PaletteCategory::Export,
        selection_fallback: false,
    });
    commands.extend(
        EXPORT
            .into_iter()
            .map(|a| action(a, PaletteCategory::Export, false)),
    );
    commands.push(action(N::ToggleTheme, PaletteCategory::App, false));
    commands
}

/// The predicate of the palette's "Change stroke color" command
/// (`CommandPalette.tsx:461-477`): a selection whose stroke can change
/// (`canChangeStrokeColor`).
pub fn palette_change_stroke_available(ctx: &ActionContext<'_>) -> bool {
    let selected: Vec<_> = ctx.selected(false).into_iter().cloned().collect();
    let active_tool = ctx
        .field("activeTool", "type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    !selected.is_empty() && super::can_change_stroke_color(active_tool, &selected)
}

/// The predicate of the palette's "Change background color" command
/// (`CommandPalette.tsx:480-499`): a selection whose background can change
/// (`canChangeBackgroundColor`).
pub fn palette_change_background_available(ctx: &ActionContext<'_>) -> bool {
    let selected: Vec<_> = ctx.selected(false).into_iter().cloned().collect();
    !selected.is_empty() && super::can_change_background_color(ctx, &selected)
}

/// `isCommandAvailable(command)` (`CommandPalette.tsx:672-689`).
/// `ctx.elements` should be the non-deleted elements.
pub fn palette_command_available(command: &PaletteActionCommand, ctx: &ActionContext<'_>) -> bool {
    if command.view_mode() == Some(false) && ctx.flag("viewModeEnabled") {
        return false;
    }
    match command.source {
        PaletteCommandSource::Action(a) => match a.spec().predicate {
            Some(p) => p(ctx),
            None if command.selection_fallback => !ctx.selected(false).is_empty(),
            None => true,
        },
        PaletteCommandSource::ClearCanvas | PaletteCommandSource::ImageExport => true,
    }
}

// ---------------------------------------------------------------------------
// Styles panel, full mode
// ---------------------------------------------------------------------------

/// The `getShapeActionPredicates` flags (`shapeActionPredicates.ts`) that
/// gate the full panel's controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelGate {
    StrokeColor,
    BackgroundColor,
    Fill,
    StrokeWidth,
    StrokeStyle,
    FreedrawMode,
    Sloppiness,
    Roundness,
    ArrowType,
    Text,
    TextAlign,
    VerticalAlign,
    Arrowheads,
    Opacity,
    Layers,
    Align,
    Distribute,
    ShowExtraActions,
    Link,
    CropEditor,
    LineEditor,
}

/// The fieldset a control sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelFieldset {
    None,
    /// The font family's own `<fieldset>`.
    FontFamily,
    /// `labels.layers` (`LayersFieldset`).
    Layers,
    /// `labels.align`, first row.
    AlignHorizontal,
    /// `labels.align`, second row.
    AlignVertical,
    /// `labels.actions`.
    Actions,
}

/// A control of the styles panel: its action, rendered through
/// `renderAction`, when all its gates hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelControl {
    pub action: ActionName,
    pub gates: &'static [PanelGate],
    pub fieldset: PanelFieldset,
}

/// `SelectedShapeActions` (`Actions.tsx:129-217`), in order; `bucket_fill`
/// is the bucket fill tool being active (:150-158), `rtl` the document
/// direction (the align row mirrors, :93-107).
pub fn full_styles_panel(bucket_fill: bool, rtl: bool) -> Vec<PanelControl> {
    use ActionName as N;
    use PanelGate as G;
    const fn c(
        action: ActionName,
        gates: &'static [PanelGate],
        fieldset: PanelFieldset,
    ) -> PanelControl {
        PanelControl {
            action,
            gates,
            fieldset,
        }
    }
    if bucket_fill {
        return vec![
            c(N::ChangeBucketFillBackgroundColor, &[], PanelFieldset::None),
            c(N::ChangeFillStyle, &[], PanelFieldset::None),
            c(N::ChangeOpacity, &[], PanelFieldset::None),
        ];
    }
    let none = PanelFieldset::None;
    let mut panel = vec![
        c(N::ChangeStrokeColor, &[G::StrokeColor], none),
        c(N::ChangeBackgroundColor, &[G::BackgroundColor], none),
        c(N::ChangeFillStyle, &[G::Fill], none),
        c(N::ChangeStrokeWidth, &[G::StrokeWidth], none),
        c(N::ChangeStrokeStyle, &[G::StrokeStyle], none),
        c(N::ChangeFreedrawMode, &[G::FreedrawMode], none),
        c(N::ChangeSloppiness, &[G::Sloppiness], none),
        c(N::ChangeRoundness, &[G::Roundness], none),
        c(N::ChangeArrowType, &[G::ArrowType], none),
        c(N::ChangeFontFamily, &[G::Text], PanelFieldset::FontFamily),
        c(N::ChangeFontSize, &[G::Text], none),
        c(N::ChangeTextAlign, &[G::Text, G::TextAlign], none),
        c(N::ChangeVerticalAlign, &[G::VerticalAlign], none),
        c(N::ChangeArrowhead, &[G::Arrowheads], none),
        c(N::ChangeOpacity, &[G::Opacity], none),
    ];
    let layers = PanelFieldset::Layers;
    panel.extend([
        c(N::SendToBack, &[G::Layers], layers),
        c(N::SendBackward, &[G::Layers], layers),
        c(N::BringForward, &[G::Layers], layers),
        c(N::BringToFront, &[G::Layers], layers),
    ]);
    let row = PanelFieldset::AlignHorizontal;
    let (first, last) = if rtl {
        (N::AlignRight, N::AlignLeft)
    } else {
        (N::AlignLeft, N::AlignRight)
    };
    panel.extend([
        c(first, &[G::Align], row),
        c(N::AlignHorizontallyCentered, &[G::Align], row),
        c(last, &[G::Align], row),
        c(N::DistributeHorizontally, &[G::Align, G::Distribute], row),
    ]);
    let row = PanelFieldset::AlignVertical;
    panel.extend([
        c(N::AlignTop, &[G::Align], row),
        c(N::AlignVerticallyCentered, &[G::Align], row),
        c(N::AlignBottom, &[G::Align], row),
        c(N::DistributeVertically, &[G::Align, G::Distribute], row),
    ]);
    let actions = PanelFieldset::Actions;
    panel.extend([
        c(N::DuplicateSelection, &[G::ShowExtraActions], actions),
        c(N::DeleteSelectedElements, &[G::ShowExtraActions], actions),
        c(N::Group, &[G::ShowExtraActions], actions),
        c(N::Ungroup, &[G::ShowExtraActions], actions),
        c(N::Hyperlink, &[G::ShowExtraActions, G::Link], actions),
        c(
            N::CropEditor,
            &[G::ShowExtraActions, G::CropEditor],
            actions,
        ),
        c(
            N::ToggleLinearEditor,
            &[G::ShowExtraActions, G::LineEditor],
            actions,
        ),
    ]);
    panel
}

/// The panel's controls that render: every gate holds (`gate` answers
/// `getShapeActionPredicates`) and `renderAction` renders the action.
pub fn render_styles_panel(
    panel: &[PanelControl],
    gate: impl Fn(PanelGate) -> bool,
    manager: &ActionManager,
    ctx: &ActionContext<'_>,
) -> Vec<ActionName> {
    panel
        .iter()
        .filter(|c| c.gates.iter().all(|g| gate(*g)) && manager.can_render(c.action, ctx))
        .map(|c| c.action)
        .collect()
}

// ---------------------------------------------------------------------------
// Main menu
// ---------------------------------------------------------------------------

/// `MainMenu.DefaultItems` used by the default menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainMenuItem {
    LoadScene,
    SaveToActiveFile,
    Export,
    SaveAsImage,
    SearchMenu,
    Help,
    ClearCanvas,
    Socials,
    ToggleTheme,
    ChangeCanvasBackground,
}

/// A main menu row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainMenuRow {
    pub item: MainMenuItem,
    /// The action it runs or renders, if any.
    pub action: Option<ActionName>,
    /// The label's locale key.
    pub label: &'static str,
    pub shortcut: String,
}

/// An entry of the main menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainMenuEntry {
    Item(MainMenuRow),
    Separator,
    Group {
        title: &'static str,
        items: Vec<MainMenuRow>,
    },
}

/// `DefaultMainMenu` (`LayerUI.tsx:111-136`) with each item's own
/// visibility (`DefaultItems.tsx`).
pub fn default_main_menu(
    manager: &ActionManager,
    ctx: &ActionContext<'_>,
    labels: &KeyLabels<'_>,
) -> Vec<MainMenuEntry> {
    let darwin = ctx.env.is_darwin;
    let shortcut = |name: &str| get_shortcut_from_shortcut_name(name, 0, darwin, labels);
    let row = |item, action, label, shortcut| MainMenuRow {
        item,
        action,
        label,
        shortcut,
    };
    let canvas = &ctx.props.canvas_actions;
    let enabled = |a: ActionName| manager.is_action_enabled(a, ctx);
    let mut entries = Vec::new();
    let mut push = |r: MainMenuRow| entries.push(MainMenuEntry::Item(r));
    if enabled(ActionName::LoadScene) {
        push(row(
            MainMenuItem::LoadScene,
            Some(ActionName::LoadScene),
            "buttons.load",
            shortcut("loadScene"),
        ));
    }
    if enabled(ActionName::SaveToActiveFile) {
        push(row(
            MainMenuItem::SaveToActiveFile,
            Some(ActionName::SaveToActiveFile),
            "buttons.save",
            shortcut("saveScene"),
        ));
    }
    if canvas.export.is_some() {
        push(row(
            MainMenuItem::Export,
            None,
            "buttons.export",
            String::new(),
        ));
    }
    if canvas.save_as_image {
        push(row(
            MainMenuItem::SaveAsImage,
            None,
            "buttons.exportImage",
            shortcut("imageExport"),
        ));
    }
    push(row(
        MainMenuItem::SearchMenu,
        Some(ActionName::SearchMenu),
        "search.title",
        shortcut("searchMenu"),
    ));
    push(row(
        MainMenuItem::Help,
        Some(ActionName::ToggleShortcuts),
        "helpDialog.title",
        "?".to_owned(),
    ));
    if enabled(ActionName::ClearCanvas) {
        push(row(
            MainMenuItem::ClearCanvas,
            Some(ActionName::ClearCanvas),
            "buttons.clearReset",
            String::new(),
        ));
    }
    entries.push(MainMenuEntry::Separator);
    entries.push(MainMenuEntry::Group {
        title: "Excalidraw links",
        items: vec![row(MainMenuItem::Socials, None, "", String::new())],
    });
    entries.push(MainMenuEntry::Separator);
    if enabled(ActionName::ToggleTheme) {
        let label = ActionName::ToggleTheme.spec().label_key(ctx).unwrap_or("");
        entries.push(MainMenuEntry::Item(row(
            MainMenuItem::ToggleTheme,
            Some(ActionName::ToggleTheme),
            label,
            shortcut("toggleTheme"),
        )));
    }
    if !ctx.flag("viewModeEnabled") && canvas.change_view_background_color {
        entries.push(MainMenuEntry::Item(row(
            MainMenuItem::ChangeCanvasBackground,
            Some(ActionName::ChangeViewBackgroundColor),
            "labels.canvasBackground",
            String::new(),
        )));
    }
    entries
}
