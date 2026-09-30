//! DOM chrome (toolbar, panels, dialogs) that applies editor effects.
//!
//! Upstream counterpart: `packages/excalidraw/components/*`; for
//! [`primitives`] the UI kit (`Island`, `Stack`, `Button`, `IconButton`,
//! `RadioGroup`, `Range`, `TextField`, `Popover`, `Modal`, `Dialog`,
//! `Tooltip`) built with [`dom`]; for [`layers`]
//! the canvases (`components/canvases/*`) and the helpers they render
//! through (`renderer/helpers.ts`); for [`fonts`] the scene font loading of
//! `packages/excalidraw/fonts/Fonts.ts`; for [`keyboard`] the key events
//! as `App.onKeyDown` reads them (`common/src/utils.ts`); for [`text_editor`]
//! the DOM half of the text editor overlay (`wysiwyg/textWysiwyg.tsx`); for
//! [`theme`] the light and dark tokens of `css/theme.scss` and the
//! container's `theme--dark` class; for
//! [`icons`] the icon set of `components/icons.tsx`; for
//! [`styles_panel`] the full, compact and mobile styles panels
//! (`components/Actions.tsx`, `LayerUI.tsx`); for [`editor_interface`]
//! the form factor rules (`common/src/editorInterface.ts`); for [`toolbar`] the desktop shapes toolbar and its
//! extra-tools dropdown (`components/Toolbar.tsx`, `Tools.tsx`); for
//! [`mobile_menu`] the phone layout's top and bottom bars and toolbar
//! (`components/MobileMenu.tsx`, `MobileToolbar.tsx`, `ToolPopover.tsx`);
//! for [`main_menu`] the hamburger menu (`components/main-menu/*`,
//! `components/dropdownMenu/*`); for [`context_menu`] the canvas and
//! element context menus (`components/ContextMenu.tsx`); for [`footer`]
//! the footer's zoom, undo/redo, help and exit-zen controls
//! (`components/footer/Footer.tsx`); for [`help_dialog`] the help dialog
//! and its shortcut islands (`components/HelpDialog.tsx`); for
//! [`command_palette`] the command palette
//! (`components/CommandPalette/CommandPalette.tsx`); for [`library_sidebar`] the default
//! sidebar with the library (`components/DefaultSidebar.tsx`,
//! `components/Sidebar/*`, `components/LibraryMenu*.tsx`,
//! `components/LibraryUnit.tsx`); for [`search_menu`] the sidebar's search
//! tab (`components/SearchMenu.tsx`); for [`font_picker`] the font picker
//! (`components/FontPicker/*`); for [`hints`] the hint viewer and the
//! cursor hint (`components/HintViewer.tsx`, `components/CursorHint.tsx`);
//! for [`i18n`] the locale loader (`i18n.ts`, `locales/*.json`);
//! for [`welcome_screen`] the welcome screen
//! (`components/welcome-screen/*`); for [`top_picks_dnd`]
//! the pickers' top-picks drag and drop (`components/TopPicksDnD/*`); for
//! [`accessibility`] the accessible names upstream's icon-only triggers
//! lack (ADR-012); for [`stats`] the stats panel
//! (`components/Stats/*`).
//!
//! Targets: wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-canvas2d`, `excali-editor`.

pub mod accessibility;
pub mod action_panels;
pub mod color_picker;
pub mod command_palette;
pub mod context_menu;
pub mod convert_popup;
pub mod dom;
pub mod editor_interface;
pub mod font_picker;
pub mod fonts;
pub mod footer;
pub mod help_dialog;
pub mod hints;
pub mod i18n;
pub mod icons;
pub mod keyboard;
pub mod layers;
pub mod library_sidebar;
pub mod main_menu;
pub mod mobile_menu;
pub mod primitives;
pub mod search_menu;
pub mod stats;
pub mod styles_panel;
pub mod text_editor;
pub mod theme;
pub mod toolbar;
pub mod top_picks_dnd;
pub mod welcome_screen;
