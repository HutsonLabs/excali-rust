//! Element constants: default style, presets for size, width and
//! roughness, and rounding radii.
//!
//! Upstream counterpart: `packages/common/src/constants.ts` and
//! `packages/common/src/colors.ts` at the pinned commit (see
//! `site/content/research/data-model.md`, sections 1.4 and 2). Font family
//! ids are on [`FontFamily`](crate::element::FontFamily).

use crate::element::{
    ElementType, FillStyle, StrokeStyle, StrokeWidthKey, TextAlign, VerticalAlign,
};

/// `COLOR_PALETTE.black`, `colors.ts:195`: the default stroke.
pub const COLOR_BLACK: &str = "#1e1e1e";
/// `COLOR_PALETTE.transparent`, `colors.ts:194`: the default background.
pub const COLOR_TRANSPARENT: &str = "transparent";
/// `COLOR_PALETTE.white`, `colors.ts:196`.
pub const COLOR_WHITE: &str = "#ffffff";

/// Font size presets (`FONT_SIZES`, `constants.ts:122-127`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSizes {
    pub sm: f64,
    pub md: f64,
    pub lg: f64,
    pub xl: f64,
}

pub const FONT_SIZES: FontSizes = FontSizes {
    sm: 16.0,
    md: 20.0,
    lg: 28.0,
    xl: 36.0,
};

/// `MIN_FONT_SIZE`, `constants.ts:222`.
pub const MIN_FONT_SIZE: f64 = 1.0;
/// `DEFAULT_FONT_SIZE`, `constants.ts:223`.
pub const DEFAULT_FONT_SIZE: f64 = 20.0;
/// `DEFAULT_TEXT_ALIGN`, `constants.ts:274`.
pub const DEFAULT_TEXT_ALIGN: TextAlign = TextAlign::Left;
/// `DEFAULT_VERTICAL_ALIGN`, `constants.ts:275`.
pub const DEFAULT_VERTICAL_ALIGN: VerticalAlign = VerticalAlign::Top;

/// Radius as a fraction of the largest side, for legacy and proportional
/// rounding (`constants.ts:443`).
pub const DEFAULT_PROPORTIONAL_RADIUS: f64 = 0.25;
/// Fixed radius in pixels for adaptive rounding (`constants.ts:445`).
pub const DEFAULT_ADAPTIVE_RADIUS: f64 = 32.0;

/// Roughness presets (`ROUGHNESS`, `constants.ts:466-470`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Roughness {
    pub architect: f64,
    pub artist: f64,
    pub cartoonist: f64,
}

pub const ROUGHNESS: Roughness = Roughness {
    architect: 0.0,
    artist: 1.0,
    cartoonist: 2.0,
};

/// Stroke width presets (`constants.ts:480-501`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeWidths {
    pub thin: f64,
    pub medium: f64,
    pub bold: f64,
    /// Not offered in the UI.
    pub extra_bold: f64,
}

impl StrokeWidths {
    /// The width for a preset.
    pub const fn get(&self, key: StrokeWidthKey) -> f64 {
        match key {
            StrokeWidthKey::Thin => self.thin,
            StrokeWidthKey::Medium => self.medium,
            StrokeWidthKey::Bold => self.bold,
        }
    }
}

/// `STROKE_WIDTH`, `constants.ts:480-487`.
pub const STROKE_WIDTH: StrokeWidths = StrokeWidths {
    thin: 1.0,
    medium: 2.0,
    bold: 4.0,
    extra_bold: 8.0,
};

/// `FREEDRAW_STROKE_WIDTH`, `constants.ts:489-501`: half of `STROKE_WIDTH`.
pub const FREEDRAW_STROKE_WIDTH: StrokeWidths = StrokeWidths {
    thin: 0.5,
    medium: 1.0,
    bold: 2.0,
    extra_bold: 4.0,
};

/// `getStrokeWidthByKey`, `constants.ts:503-510`.
pub const fn stroke_width_by_key(element_type: ElementType, key: StrokeWidthKey) -> f64 {
    match element_type {
        ElementType::Freedraw => FREEDRAW_STROKE_WIDTH.get(key),
        _ => STROKE_WIDTH.get(key),
    }
}

/// `DEFAULT_ELEMENT_STROKE_WIDTH_KEY`, `constants.ts:512`.
pub const DEFAULT_ELEMENT_STROKE_WIDTH_KEY: StrokeWidthKey = StrokeWidthKey::Medium;

/// Default element style (`DEFAULT_ELEMENT_PROPS`, `constants.ts:514-532`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementProps {
    pub stroke_color: &'static str,
    pub background_color: &'static str,
    pub fill_style: FillStyle,
    pub stroke_width: f64,
    pub stroke_style: StrokeStyle,
    pub roughness: f64,
    pub opacity: f64,
    pub locked: bool,
}

pub const DEFAULT_ELEMENT_PROPS: ElementProps = ElementProps {
    stroke_color: COLOR_BLACK,
    background_color: COLOR_TRANSPARENT,
    fill_style: FillStyle::Solid,
    stroke_width: STROKE_WIDTH.get(DEFAULT_ELEMENT_STROKE_WIDTH_KEY),
    stroke_style: StrokeStyle::Solid,
    roughness: ROUGHNESS.artist,
    opacity: 100.0,
    locked: false,
};

/// `DEFAULT_STROKE_STREAMLINE`, `constants.ts:622`.
pub const DEFAULT_STROKE_STREAMLINE: f64 = 0.5;

/// `MIME_TYPES.excalidraw` (`STRING_MIME_TYPES.excalidraw`,
/// `constants.ts:313`): the MIME type of a `.excalidraw` file, and the
/// keyword of the PNG `tEXt` chunk that embeds a scene (`image.ts:36`).
pub const MIME_TYPE_EXCALIDRAW: &str = "application/vnd.excalidraw+json";

/// `EXPORT_DATA_TYPES.excalidraw`, `constants.ts:345`: the `type` of a
/// `.excalidraw` file.
pub const EXPORT_DATA_TYPE_EXCALIDRAW: &str = "excalidraw";
/// `EXPORT_DATA_TYPES.excalidrawLibrary`, `constants.ts:347`.
pub const EXPORT_DATA_TYPE_EXCALIDRAW_LIBRARY: &str = "excalidrawlib";
/// `VERSIONS.excalidraw`, `constants.ts:417`: the `version` of a
/// `.excalidraw` file.
pub const VERSION_EXCALIDRAW: f64 = 2.0;
/// `VERSIONS.excalidrawLibrary`, `constants.ts:418`.
pub const VERSION_EXCALIDRAW_LIBRARY: f64 = 2.0;

/// The default sticky-note background (`DEFAULT_STICKY_NOTE_BG`,
/// `colors.ts:268`).
pub const DEFAULT_STICKY_NOTE_BG: &str = "#ffdf6b";
/// Slots in the colour-picker top-picks strip (`COLOR_TOP_PICKS_SLOTS`,
/// `colors.ts:236`).
pub const COLOR_TOP_PICKS_SLOTS: usize = 5;
/// Slots in the font-picker top-picks strip (`FONT_TOP_PICKS_SLOTS`,
/// `constants.ts:273`).
pub const FONT_TOP_PICKS_SLOTS: usize = 3;

/// `DEFAULT_GRID_SIZE`, `constants.ts:293`.
pub const DEFAULT_GRID_SIZE: f64 = 20.0;
/// `DEFAULT_GRID_STEP`, `constants.ts:294`.
pub const DEFAULT_GRID_STEP: f64 = 5.0;

/// `MIN_ZOOM`, `constants.ts:363`.
pub const MIN_ZOOM: f64 = 0.1;
/// `MAX_ZOOM`, `constants.ts:364`.
pub const MAX_ZOOM: f64 = 30.0;
/// `DEFAULT_ZOOM.value`, `constants.ts:366-368`.
pub const DEFAULT_ZOOM: f64 = 1.0;

/// `EXPORT_SCALES`, `constants.ts:401`.
pub const EXPORT_SCALES: [f64; 3] = [1.0, 2.0, 3.0];

/// `DEFAULT_SIDEBAR.name`, `constants.ts:537-540`.
pub const DEFAULT_SIDEBAR_NAME: &str = "default";

/// `STATS_PANELS.generalStats`, `constants.ts:584`.
pub const STATS_PANEL_GENERAL_STATS: u32 = 1;
/// `STATS_PANELS.elementProperties`, `constants.ts:584`.
pub const STATS_PANEL_ELEMENT_PROPERTIES: u32 = 2;
