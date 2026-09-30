//! The actions registry as data (`packages/excalidraw/actions/*`).
//!
//! Upstream, at the pinned commit:
//!
//! - `actions/types.ts:45-144`: the 99 [`ActionName`]s; 95 are registered
//!   (93 through `register()`, undo and redo through `createUndoAction` /
//!   `createRedoAction`), and `toggleFullScreen`, `createContainerFromText`,
//!   `commandPalette` and `elementStats` have no action;
//! - every `register({...})` under `actions/`: [`ActionSpec`] keeps the
//!   label, keywords, icon, `keyPriority`, `viewMode`, `navigation`,
//!   `trackEvent`, whether there is a `PanelComponent`, and the `keyTest`,
//!   `predicate`, `checked`, dynamic `label` and `icon` callbacks, ported;
//! - `actions/manager.tsx`: [`ActionManager`] (`handleKeyDown`,
//!   `executeAction`'s gate, `renderAction`'s gate, `isActionEnabled`) and
//!   [`track_action`];
//! - `actions/shortcuts.ts`, `shortcut.ts`, `common/src/keys.ts`:
//!   [`get_shortcut_from_shortcut_name`], [`get_shortcut_key`],
//!   [`match_key`];
//! - the menus and panels built from the registry: the context menus
//!   (`App.tsx:13835-13936`, `components/ContextMenu.tsx`), the command
//!   palette's action commands (`components/CommandPalette/CommandPalette.tsx`),
//!   the full styles panel (`components/Actions.tsx:63-217`) with the
//!   predicates that gate it (`components/shapeActionPredicates.ts`,
//!   [`get_shape_action_predicates`]) and the
//!   default main menu (`components/LayerUI.tsx:111-136`,
//!   `components/main-menu/DefaultItems.tsx`).
//!
//! See `site/content/research/ui-design-system.md` sections 3 and 5.
//!
//! `perform` is not here: each action's effect lives with the feature it
//! drives (zoom in [`crate::viewport`], undo and redo in
//! [`crate::session`], the tools in [`crate::tools`], ...). Labels are
//! locale keys; translating them is the UI's job.

mod context;
mod keys;
mod manager;
mod menus;
mod names;
mod registry;
mod shape_predicates;

pub use context::{ActionContext, ActionEnv, AppProps, CanvasActions, FormFactor};
pub use keys::{
    get_shortcut_from_shortcut_name, get_shortcut_key, match_key, KeyEvent, KeyLabels,
    SHORTCUT_NAMES,
};
pub use manager::{track_action, ActionManager, ActionSource, KeyDownOutcome, TrackedEvent};
pub use menus::{
    build_context_menu, commands_from_actions, default_main_menu, full_styles_panel,
    get_context_menu_items, palette_change_background_available, palette_change_stroke_available,
    palette_command_available, render_styles_panel, ContextMenuEntry, ContextMenuItem,
    ContextMenuKind, ContextMenuRow, MainMenuEntry, MainMenuItem, MainMenuRow,
    PaletteActionCommand, PaletteCategory, PaletteCommandSource, PanelControl, PanelFieldset,
    PanelGate,
};
pub use names::ActionName;
pub use registry::{
    registered_actions, ActionIcon, ActionLabel, ActionSpec, CheckedFn, KeyTestFn, PredicateFn,
    TrackEvent,
};
pub use shape_predicates::{
    can_change_background_color, can_change_roundness, can_change_stroke_color,
    can_have_arrowheads, get_shape_action_predicates, get_target_elements, has_background,
    has_fill_style, has_freedraw_mode, has_roughness, has_stroke_color, has_stroke_style,
    has_stroke_width, shape_action_predicates, show_selected_shape_actions, tool_is_arrow,
    ShapeActionPredicates,
};
