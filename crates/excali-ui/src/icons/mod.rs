//! The icon set of `packages/excalidraw/components/icons.tsx` (MIT): every
//! icon it exports as the static SVG string React renders from upstream's
//! own `createIcon` (`icons.tsx:27-51`), with the size preset it was built
//! with and whether it mirrors in right-to-left languages (`mirror`,
//! `icons.tsx:22-25`, the `rtl-mirror` class). `generated.rs` is written by
//! `tools/goldens/icons.mjs` at the pinned commit; the items keep upstream's
//! export names (`icons::HelpIcon`, `icons::helpIcon`).

use excali_scene::shape::Theme;

use crate::dom::Element;

// Written by tools/goldens/icons.mjs, which --check compares byte for byte.
#[rustfmt::skip]
mod generated;
// The generated items are in scope here: lowercase upstream names such as
// `done` or `clone` shadow bindings, so the parser lives in its own module.
mod markup;

pub use generated::*;

/// The rule that flips an icon created with `mirror: true` (the
/// `rtl-mirror` class) in a right-to-left document: `css/styles.scss:679-683`
/// compiled by `tools/goldens/icons.mjs`.
pub const ICONS_CSS: &str = include_str!("icons.css");

/// The option presets icons are built with (directly or spread with
/// overrides).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// `tablerIconProps` (`icons.tsx:53-61`): 24×24, stroke 2,
    /// `currentColor`, round caps and joins.
    Tabler,
    /// `modifiedTablerIconProps` (`icons.tsx:63-70`): 20×20, `currentColor`,
    /// round caps and joins.
    ModifiedTabler,
    /// `arrowheadPreviewIconProps` (`icons.tsx:72-75`): 40×20.
    ArrowheadPreview,
}

impl Preset {
    /// The preset's view box width and height.
    pub fn size(self) -> (u32, u32) {
        match self {
            Preset::Tabler => (24, 24),
            Preset::ModifiedTabler => (20, 20),
            Preset::ArrowheadPreview => (40, 20),
        }
    }
}

/// An icon's markup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Markup {
    /// A `createIcon(...)` element (or `emptyIcon`, a sized `div`).
    Static(&'static str),
    /// A `React.memo(({ theme }) => createIcon(...))` component, rendered
    /// for each theme.
    Themed {
        light: &'static str,
        dark: &'static str,
    },
    /// Path data other components draw (`bucketFillIconSvgPaths`,
    /// `eyeDropperIconSvgPaths`).
    Paths(&'static [&'static str]),
}

/// One `icons.tsx` export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Icon {
    /// The export's name.
    pub name: &'static str,
    pub markup: Markup,
    /// Created with `mirror: true`: flipped in right-to-left languages.
    pub mirror: bool,
    pub preset: Option<Preset>,
}

impl Icon {
    /// The icon's markup in `theme`; `None` for a path list.
    pub fn svg(&self, theme: Theme) -> Option<&'static str> {
        match self.markup {
            Markup::Static(markup) => Some(markup),
            Markup::Themed { light, dark } => Some(match theme {
                Theme::Light => light,
                Theme::Dark => dark,
            }),
            Markup::Paths(_) => None,
        }
    }

    /// The icon in `theme` as a DOM builder tree; `None` for a path list.
    pub fn element(&self, theme: Theme) -> Option<Element> {
        self.svg(theme).map(markup::parse)
    }
}

/// An element tree from markup React wrote for a static component
/// (elements with double-quoted attributes and end tags, no text), as the
/// icons are read.
pub(crate) fn parse_markup(markup: &str) -> Element {
    markup::parse(markup)
}

/// The icon exported as `name`.
pub fn icon(name: &str) -> Option<&'static Icon> {
    ICONS.iter().copied().find(|i| i.name == name)
}
