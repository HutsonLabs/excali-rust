//! The element model: every element type, its fields and the enumerations
//! they use.
//!
//! Upstream counterpart: `packages/element/src/types.ts` at the pinned commit
//! (see `site/content/research/data-model.md`, sections 1.1 to 1.4, and
//! `site/content/architecture/file-format.md`, "Rust model"). An [`Element`]
//! is the shared [`ElementBase`] (`_ExcalidrawElementBase`, `types.ts:40-87`)
//! plus an [`ElementKind`], an enum tagged by the JSON `type` string that
//! holds the per-type fields (`types.ts:89-437`), plus any keys the model
//! does not know about ([`Element::extra`]).
//!
//! Every `number` in upstream's types is an `f64` here, because a JS number
//! is one; points are `[f64; 2]` (`LocalPoint`, `packages/math/src/types.ts`).
//! Font family ids are integers ([`FontFamily`]).
//!
//! [`ElementBase`] and [`ElementKind`] implement serde with upstream's JSON
//! key names, so one element document deserialises into both. A key that is
//! optional upstream (`customData?`, text `labelPosition?`, the elbow-arrow
//! keys that plain arrows lack) is an `Option` that is skipped when `None`;
//! where such a key may also be `null` it is an `Option<Option<T>>`
//! (`None` absent, `Some(None)` null). [`Element`]'s own serde, the
//! whole-element codec, is built on these types and keeps unknown keys and
//! upstream's key order.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::fmt;

use crate::constants::{
    COLOR_TRANSPARENT, DEFAULT_ELEMENT_PROPS, DEFAULT_FONT_SIZE, DEFAULT_STROKE_STREAMLINE,
};
use crate::json;
use crate::layout::{Canonical, Layout};

// ---------------------------------------------------------------------------
// Scalars and ids

/// A point relative to the element's `x`/`y` (`LocalPoint`,
/// `packages/math/src/types.ts:52`). In JSON, a two-element array.
pub type LocalPoint = [f64; 2];

/// A group id (`GroupId = string`, `types.ts:24`).
pub type GroupId = String;

/// An angle in radians (`Radians`, `packages/math/src/types.ts:9`).
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Radians(pub f64);

/// A fractional index key (`FractionalIndex`, `types.ts:33`): base-62 digits
/// compared as plain strings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FractionalIndex(pub String);

/// Id of a binary file in the scene's `files` map (`FileId`, `types.ts:439`):
/// the SHA-1 hex of the file bytes, or a 40-character nanoid.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileId(pub String);

// ---------------------------------------------------------------------------
// Element types

/// The fourteen element type strings (`types.ts:223-234`,
/// `typeChecks.ts:255-284`), in the order the research page lists them.
///
/// Legacy `"draw"` is not a type of the model: restore turns it into
/// `"line"` (`restore.ts:612-637`). `Selection` is never persisted: restore
/// drops it (`restore.ts:969-971`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ElementType {
    Selection,
    Rectangle,
    StickyNote,
    Diamond,
    Ellipse,
    Embeddable,
    Iframe,
    Image,
    Frame,
    MagicFrame,
    Text,
    Line,
    Arrow,
    Freedraw,
}

impl ElementType {
    /// Every element type.
    pub const ALL: [ElementType; 14] = [
        ElementType::Selection,
        ElementType::Rectangle,
        ElementType::StickyNote,
        ElementType::Diamond,
        ElementType::Ellipse,
        ElementType::Embeddable,
        ElementType::Iframe,
        ElementType::Image,
        ElementType::Frame,
        ElementType::MagicFrame,
        ElementType::Text,
        ElementType::Line,
        ElementType::Arrow,
        ElementType::Freedraw,
    ];

    /// The JSON `type` string.
    pub const fn as_str(self) -> &'static str {
        match self {
            ElementType::Selection => "selection",
            ElementType::Rectangle => "rectangle",
            ElementType::StickyNote => "stickynote",
            ElementType::Diamond => "diamond",
            ElementType::Ellipse => "ellipse",
            ElementType::Embeddable => "embeddable",
            ElementType::Iframe => "iframe",
            ElementType::Image => "image",
            ElementType::Frame => "frame",
            ElementType::MagicFrame => "magicframe",
            ElementType::Text => "text",
            ElementType::Line => "line",
            ElementType::Arrow => "arrow",
            ElementType::Freedraw => "freedraw",
        }
    }

    /// The type for a JSON `type` string; `None` for legacy `"draw"` and
    /// unknown strings (both are restore's business).
    pub fn parse(s: &str) -> Option<ElementType> {
        ElementType::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// `line` or `arrow` (`isLinearElementType`, `typeChecks.ts:159-165`).
    pub const fn is_linear(self) -> bool {
        matches!(self, ElementType::Line | ElementType::Arrow)
    }

    /// Its roundness can be edited (`canChangeRoundness`,
    /// `comparisons.ts:57-64`). Ellipses, arrows, freedraw, text and frames
    /// cannot, whatever their `roundness` field holds.
    pub const fn can_change_roundness(self) -> bool {
        matches!(
            self,
            ElementType::Rectangle
                | ElementType::Iframe
                | ElementType::Embeddable
                | ElementType::Line
                | ElementType::Diamond
                | ElementType::StickyNote
                | ElementType::Image
        )
    }

    /// Can bind to other elements: `arrow` only (`isBindingElementType`,
    /// `typeChecks.ts:178-182`).
    pub const fn is_binding(self) -> bool {
        matches!(self, ElementType::Arrow)
    }

    /// Can be a binding target (`ExcalidrawBindableElement`,
    /// `types.ts:293-303`). A text element is bindable only when it has no
    /// container; [`Element::is_bindable`] checks that.
    pub const fn is_bindable(self) -> bool {
        matches!(
            self,
            ElementType::Rectangle
                | ElementType::StickyNote
                | ElementType::Diamond
                | ElementType::Ellipse
                | ElementType::Embeddable
                | ElementType::Iframe
                | ElementType::Image
                | ElementType::Frame
                | ElementType::MagicFrame
                | ElementType::Text
        )
    }

    /// Can contain a bound text label (`ExcalidrawTextContainer`,
    /// `types.ts:305-310`; `VALID_CONTAINER_TYPES`, `textElement.ts:508-514`).
    pub const fn is_text_container(self) -> bool {
        matches!(
            self,
            ElementType::Rectangle
                | ElementType::StickyNote
                | ElementType::Diamond
                | ElementType::Ellipse
                | ElementType::Arrow
        )
    }

    /// `frame` or `magicframe` (`ExcalidrawFrameLikeElement`, `types.ts:188-190`).
    pub const fn is_frame_like(self) -> bool {
        matches!(self, ElementType::Frame | ElementType::MagicFrame)
    }

    /// `embeddable` or `iframe` (`ExcalidrawIframeLikeElement`,
    /// `types.ts:138-140`).
    pub const fn is_iframe_like(self) -> bool {
        matches!(self, ElementType::Embeddable | ElementType::Iframe)
    }

    /// Flowchart node shapes (`ExcalidrawFlowchartNodeElement`,
    /// `types.ts:201-205`; `isFlowchartNodeElement`).
    pub const fn is_flowchart_node(self) -> bool {
        matches!(
            self,
            ElementType::Rectangle
                | ElementType::StickyNote
                | ElementType::Diamond
                | ElementType::Ellipse
        )
    }

    /// Rounded with a fixed pixel radius (`isUsingAdaptiveRadius`,
    /// `typeChecks.ts:322-326`).
    pub const fn uses_adaptive_radius(self) -> bool {
        matches!(
            self,
            ElementType::Rectangle
                | ElementType::Embeddable
                | ElementType::Iframe
                | ElementType::Image
        )
    }

    /// Rounded with a radius proportional to size
    /// (`isUsingProportionalRadius`, `typeChecks.ts:328-332`).
    pub const fn uses_proportional_radius(self) -> bool {
        matches!(
            self,
            ElementType::Line | ElementType::Arrow | ElementType::Diamond | ElementType::StickyNote
        )
    }

    /// The roundness a newly rounded element of this type gets
    /// (`getDefaultRoundnessTypeForElement`, `typeChecks.ts:357-373`).
    pub const fn default_roundness(self) -> Option<Roundness> {
        if self.uses_proportional_radius() {
            Some(Roundness::new(RoundnessType::ProportionalRadius))
        } else if self.uses_adaptive_radius() {
            Some(Roundness::new(RoundnessType::AdaptiveRadius))
        } else {
            None
        }
    }

    /// Not allowed in library items (`LIBRARY_DISABLED_TYPES`,
    /// `packages/common/src/constants.ts:542-546`).
    pub const fn is_library_disabled(self) -> bool {
        matches!(
            self,
            ElementType::Iframe | ElementType::Embeddable | ElementType::Image
        )
    }
}

impl fmt::Display for ElementType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Base-field enumerations

/// `FillStyle`, `types.ts:19`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FillStyle {
    Hachure,
    CrossHatch,
    Solid,
    Zigzag,
}

/// `StrokeStyle`, `types.ts:28`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrokeStyle {
    Solid,
    Dashed,
    Dotted,
}

/// Roundness algorithm (`ROUNDNESS`, `packages/common/src/constants.ts:447-464`),
/// written as its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoundnessType {
    /// 1: legacy rounding for rectangles; works like proportional radius.
    Legacy,
    /// 2: radius proportional to size; lines, arrows, diamonds, sticky notes.
    ProportionalRadius,
    /// 3: fixed pixel radius; rectangles, embeddables, iframes, images.
    AdaptiveRadius,
}

impl RoundnessType {
    /// The number upstream writes.
    pub const fn value(self) -> u8 {
        match self {
            RoundnessType::Legacy => 1,
            RoundnessType::ProportionalRadius => 2,
            RoundnessType::AdaptiveRadius => 3,
        }
    }

    /// The roundness type for its number.
    pub const fn from_value(value: u8) -> Option<RoundnessType> {
        match value {
            1 => Some(RoundnessType::Legacy),
            2 => Some(RoundnessType::ProportionalRadius),
            3 => Some(RoundnessType::AdaptiveRadius),
            _ => None,
        }
    }
}

impl Serialize for RoundnessType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.value())
    }
}

impl<'de> Deserialize<'de> for RoundnessType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // A JS number: `3` and `3.0` are the same value.
        let n = f64::deserialize(d)?;
        let t = if n.fract() == 0.0 && (1.0..=3.0).contains(&n) {
            RoundnessType::from_value(n as u8)
        } else {
            None
        };
        t.ok_or_else(|| {
            serde::de::Error::custom(format_args!(
                "invalid roundness type {n}, expected 1, 2 or 3"
            ))
        })
    }
}

/// `roundness: null | { type: RoundnessType; value?: number }`
/// (`types.ts:49`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Roundness {
    #[serde(rename = "type")]
    pub kind: RoundnessType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
}

impl Roundness {
    /// Roundness of the given type without an explicit value.
    pub const fn new(kind: RoundnessType) -> Roundness {
        Roundness { kind, value: None }
    }
}

/// `BoundElement.type`, `types.ts:35-38`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoundElementType {
    Arrow,
    Text,
}

/// Another element bound to this one (`BoundElement`, `types.ts:35-38`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BoundElement {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: BoundElementType,
}

// ---------------------------------------------------------------------------
// Base fields

/// The fields every element has (`_ExcalidrawElementBase`,
/// `types.ts:40-87`), in declaration order, which is also the order they
/// serialise in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementBase {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub stroke_color: String,
    pub background_color: String,
    pub fill_style: FillStyle,
    pub stroke_width: f64,
    pub stroke_style: StrokeStyle,
    pub roundness: Option<Roundness>,
    pub roughness: f64,
    pub opacity: f64,
    pub width: f64,
    pub height: f64,
    pub angle: Radians,
    /// Random integer seeding the rough.js shape so it is stable across
    /// renders.
    pub seed: f64,
    /// Incremented on every change; used to reconcile collaborators.
    pub version: f64,
    /// Random integer regenerated on every change; breaks `version` ties.
    pub version_nonce: f64,
    /// Fractional index; `None` for elements not yet in a scene.
    pub index: Option<FractionalIndex>,
    pub is_deleted: bool,
    /// Groups the element belongs to, deepest first.
    pub group_ids: Vec<GroupId>,
    pub frame_id: Option<String>,
    /// Other elements bound to this one.
    pub bound_elements: Option<Vec<BoundElement>>,
    /// Epoch milliseconds of the last update.
    pub updated: f64,
    /// Client wall-clock creation time in epoch milliseconds; `None` if
    /// unknown.
    pub created: Option<f64>,
    pub link: Option<String>,
    pub locked: bool,
    /// `customData?: Record<string, any>`; `None` when the key is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<Map<String, Value>>,
}

impl ElementBase {
    /// A base as `_newElementBase` builds it (`newElement.ts:87-172`) with
    /// `DEFAULT_ELEMENT_PROPS` (`constants.ts:514-532`): zero size and angle,
    /// no groups, frame, index, roundness, bound elements or link,
    /// `version` 1, `versionNonce` 0, and `updated` = `created` =
    /// `timestamp`. Upstream draws `id` from nanoid, `seed` from
    /// `randomInteger()` and `timestamp` from `Date.now()`; the caller
    /// supplies them here.
    pub fn new(id: impl Into<String>, x: f64, y: f64, seed: f64, timestamp: f64) -> ElementBase {
        let p = DEFAULT_ELEMENT_PROPS;
        ElementBase {
            id: id.into(),
            x,
            y,
            stroke_color: p.stroke_color.to_owned(),
            background_color: p.background_color.to_owned(),
            fill_style: p.fill_style,
            stroke_width: p.stroke_width,
            stroke_style: p.stroke_style,
            roundness: None,
            roughness: p.roughness,
            opacity: p.opacity,
            width: 0.0,
            height: 0.0,
            angle: Radians(0.0),
            seed,
            version: 1.0,
            version_nonce: 0.0,
            index: None,
            is_deleted: false,
            group_ids: Vec::new(),
            frame_id: None,
            bound_elements: None,
            updated: timestamp,
            created: Some(timestamp),
            link: None,
            locked: p.locked,
            custom_data: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Per-type fields

/// Sticky note fields (`ExcalidrawStickyNoteElement`, `types.ts:97-105`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickyNoteFields {
    /// The height the user set; `height` grows above it to fit the label
    /// and never shrinks below it.
    pub base_height: f64,
}

/// Status of an image's file (`types.ts:166`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageStatus {
    Pending,
    Saved,
    Error,
}

/// Crop rectangle of an image (`ImageCrop`, `types.ts:152-159`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageCrop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub natural_width: f64,
    pub natural_height: f64,
}

/// Image fields (`ExcalidrawImageElement`, `types.ts:161-171`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageFields {
    pub file_id: Option<FileId>,
    /// Whether the file is persisted.
    pub status: ImageStatus,
    /// X and Y scale factors in -1..1, used for flipping.
    pub scale: [f64; 2],
    pub crop: Option<ImageCrop>,
}

impl Default for ImageFields {
    /// `newImageElement` defaults (`newElement.ts:673-692`): no file,
    /// `pending`, scale `[1, 1]`, no crop.
    fn default() -> ImageFields {
        ImageFields {
            file_id: None,
            status: ImageStatus::Pending,
            scale: [1.0, 1.0],
            crop: None,
        }
    }
}

/// Frame and magic frame fields (`types.ts:178-186`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameFields {
    pub name: Option<String>,
}

/// A font family id (`FontFamilyValues`, `constants.ts:140-151`), written as
/// its number. Ids without a name (4, or a host's custom font) are kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FontFamily(pub u32);

impl FontFamily {
    pub const VIRGIL: FontFamily = FontFamily(1);
    pub const HELVETICA: FontFamily = FontFamily(2);
    pub const CASCADIA: FontFamily = FontFamily(3);
    // 4 is unused: historically Assistant, or a custom font (Obsidian).
    pub const EXCALIFONT: FontFamily = FontFamily(5);
    pub const NUNITO: FontFamily = FontFamily(6);
    pub const LILITA_ONE: FontFamily = FontFamily(7);
    pub const COMIC_SHANNS: FontFamily = FontFamily(8);
    pub const LIBERATION_SANS: FontFamily = FontFamily(9);
    pub const ASSISTANT: FontFamily = FontFamily(10);
    /// CJK hand-drawn fallback (`FONT_FAMILY_FALLBACKS`, `constants.ts:163-167`).
    pub const XIAOLAI: FontFamily = FontFamily(100);
    /// Generic fallbacks (`FONT_FAMILY_GENERIC_FALLBACKS`, `constants.ts:158-161`).
    pub const SANS_SERIF: FontFamily = FontFamily(998);
    pub const MONOSPACE: FontFamily = FontFamily(999);
    /// Windows emoji fallback.
    pub const SEGOE_UI_EMOJI: FontFamily = FontFamily(1000);
    /// `DEFAULT_FONT_FAMILY`, `constants.ts:268`.
    pub const DEFAULT: FontFamily = FontFamily::EXCALIFONT;

    /// The families of `FONT_FAMILY`, the ones an element may use.
    pub const ELEMENT_FAMILIES: [FontFamily; 9] = [
        FontFamily::VIRGIL,
        FontFamily::HELVETICA,
        FontFamily::CASCADIA,
        FontFamily::EXCALIFONT,
        FontFamily::NUNITO,
        FontFamily::LILITA_ONE,
        FontFamily::COMIC_SHANNS,
        FontFamily::LIBERATION_SANS,
        FontFamily::ASSISTANT,
    ];

    const NAMES: [(FontFamily, &'static str); 13] = [
        (FontFamily::VIRGIL, "Virgil"),
        (FontFamily::HELVETICA, "Helvetica"),
        (FontFamily::CASCADIA, "Cascadia"),
        (FontFamily::EXCALIFONT, "Excalifont"),
        (FontFamily::NUNITO, "Nunito"),
        (FontFamily::LILITA_ONE, "Lilita One"),
        (FontFamily::COMIC_SHANNS, "Comic Shanns"),
        (FontFamily::LIBERATION_SANS, "Liberation Sans"),
        (FontFamily::ASSISTANT, "Assistant"),
        (FontFamily::XIAOLAI, "Xiaolai"),
        (FontFamily::SANS_SERIF, "sans-serif"),
        (FontFamily::MONOSPACE, "monospace"),
        (FontFamily::SEGOE_UI_EMOJI, "Segoe UI Emoji"),
    ];

    /// The family's name as upstream keys it, for element and fallback ids.
    pub fn name(self) -> Option<&'static str> {
        FontFamily::NAMES
            .iter()
            .find(|(f, _)| *f == self)
            .map(|(_, n)| *n)
    }

    /// The id for a family name, for element and fallback families.
    pub fn from_name(name: &str) -> Option<FontFamily> {
        FontFamily::NAMES
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(f, _)| *f)
    }
}

impl Default for FontFamily {
    fn default() -> FontFamily {
        FontFamily::DEFAULT
    }
}

/// `TEXT_ALIGN`, `constants.ts:431-435`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// `VERTICAL_ALIGN`, `constants.ts:425-429`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// Text fields (`ExcalidrawTextElement`, `types.ts:253-291`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFields {
    pub font_size: f64,
    pub font_family: FontFamily,
    /// The font size the user picked; only sticky note labels have one.
    pub base_font_size: Option<f64>,
    /// The text as rendered, wrapped.
    pub text: String,
    pub text_align: TextAlign,
    pub vertical_align: VerticalAlign,
    pub container_id: Option<String>,
    /// The text as typed, unwrapped.
    pub original_text: String,
    /// `true`: the width fits the text; `false`: the text wraps to the width.
    pub auto_resize: bool,
    /// Unitless line height.
    pub line_height: f64,
    /// `labelPosition?: number | null`: position of an arrow label as an
    /// arc-length ratio in 0..1. `None` absent, `Some(None)` null.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub label_position: Option<Option<f64>>,
}

impl TextFields {
    /// Text fields as `newTextElement` sets them (`newElement.ts:366-383`)
    /// for the given text, family and line height: default size, alignment,
    /// no container, `originalText` = `text`, `autoResize` true,
    /// `labelPosition` and `baseFontSize` null. Upstream also normalises
    /// line endings and measures the text for the element's size; those
    /// belong to text layout, not the model.
    pub fn new(text: impl Into<String>, font_family: FontFamily, line_height: f64) -> TextFields {
        let text = text.into();
        TextFields {
            font_size: DEFAULT_FONT_SIZE,
            font_family,
            base_font_size: None,
            original_text: text.clone(),
            text,
            text_align: TextAlign::Left,
            vertical_align: VerticalAlign::Top,
            container_id: None,
            auto_resize: true,
            line_height,
            label_position: Some(None),
        }
    }
}

/// Arrowheads (`Arrowhead`, `types.ts:342-365`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arrowhead {
    Arrow,
    Bar,
    Circle,
    CircleOutline,
    Triangle,
    TriangleOutline,
    Diamond,
    DiamondOutline,
    CardinalityOne,
    CardinalityMany,
    CardinalityOneOrMany,
    CardinalityExactlyOne,
    CardinalityZeroOrOne,
    CardinalityZeroOrMany,
}

impl Arrowhead {
    /// Every current arrowhead.
    pub const ALL: [Arrowhead; 14] = [
        Arrowhead::Arrow,
        Arrowhead::Bar,
        Arrowhead::Circle,
        Arrowhead::CircleOutline,
        Arrowhead::Triangle,
        Arrowhead::TriangleOutline,
        Arrowhead::Diamond,
        Arrowhead::DiamondOutline,
        Arrowhead::CardinalityOne,
        Arrowhead::CardinalityMany,
        Arrowhead::CardinalityOneOrMany,
        Arrowhead::CardinalityExactlyOne,
        Arrowhead::CardinalityZeroOrOne,
        Arrowhead::CardinalityZeroOrMany,
    ];

    /// The JSON string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Arrowhead::Arrow => "arrow",
            Arrowhead::Bar => "bar",
            Arrowhead::Circle => "circle",
            Arrowhead::CircleOutline => "circle_outline",
            Arrowhead::Triangle => "triangle",
            Arrowhead::TriangleOutline => "triangle_outline",
            Arrowhead::Diamond => "diamond",
            Arrowhead::DiamondOutline => "diamond_outline",
            Arrowhead::CardinalityOne => "cardinality_one",
            Arrowhead::CardinalityMany => "cardinality_many",
            Arrowhead::CardinalityOneOrMany => "cardinality_one_or_many",
            Arrowhead::CardinalityExactlyOne => "cardinality_exactly_one",
            Arrowhead::CardinalityZeroOrOne => "cardinality_zero_or_one",
            Arrowhead::CardinalityZeroOrMany => "cardinality_zero_or_many",
        }
    }

    /// A current or legacy arrowhead name as `normalizeArrowhead` maps it
    /// (`packages/element/src/arrowheads.ts:3-21`): `dot` is `circle`,
    /// `crowfoot_*` are `cardinality_*`. `None` for unknown names.
    pub fn normalize(name: &str) -> Option<Arrowhead> {
        match name {
            "dot" => Some(Arrowhead::Circle),
            "crowfoot_one" => Some(Arrowhead::CardinalityOne),
            "crowfoot_many" => Some(Arrowhead::CardinalityMany),
            "crowfoot_one_or_many" => Some(Arrowhead::CardinalityOneOrMany),
            _ => Arrowhead::ALL.into_iter().find(|a| a.as_str() == name),
        }
    }
}

/// `BindMode`, `types.ts:318`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindMode {
    /// The arrow may go inside the shape up to the fixed point.
    Inside,
    /// The arrow stays outside the shape.
    Orbit,
    Skip,
}

/// An arrow end bound to an element (`FixedPointBinding`, `types.ts:320-333`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedPointBinding {
    pub element_id: String,
    /// The bound point as ratios of the target's width and height.
    pub fixed_point: [f64; 2],
    pub mode: BindMode,
}

/// Fields shared by lines and arrows (`ExcalidrawLinearElement`,
/// `types.ts:369-377`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearFields {
    pub points: Vec<LocalPoint>,
    pub start_binding: Option<FixedPointBinding>,
    pub end_binding: Option<FixedPointBinding>,
    pub start_arrowhead: Option<Arrowhead>,
    pub end_arrowhead: Option<Arrowhead>,
}

impl LinearFields {
    /// Linear fields as `newLinearElement` sets them
    /// (`newElement.ts:604-631`): no bindings, no arrowheads.
    pub fn new(points: Vec<LocalPoint>) -> LinearFields {
        LinearFields {
            points,
            start_binding: None,
            end_binding: None,
            start_arrowhead: None,
            end_arrowhead: None,
        }
    }
}

/// Line fields (`ExcalidrawLineElement`, `types.ts:379-383`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineFields {
    #[serde(flatten)]
    pub linear: LinearFields,
    /// `false` when absent: restore leaves it out for a legacy `draw`
    /// element (`restore.ts:645-650` sets it only when the type read was
    /// `line`), and upstream reads `undefined` as not closed. An absent key
    /// is written back absent until the field changes.
    #[serde(default)]
    pub polygon: bool,
}

/// A segment of an elbow arrow the user fixed (`FixedSegment`,
/// `types.ts:385-389`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixedSegment {
    pub start: LocalPoint,
    pub end: LocalPoint,
    /// Index of the segment's end point in `points`.
    pub index: f64,
}

/// Arrow fields (`ExcalidrawArrowElement`, `types.ts:391-395`, and
/// `ExcalidrawElbowArrowElement`, 397-421).
///
/// The elbow keys are written for elbow arrows and absent on others, so
/// each is `None` when absent and `Some(None)` when `null`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrowFields {
    #[serde(flatten)]
    pub linear: LinearFields,
    /// `false` when absent: files from before elbow arrows have no key, and
    /// restore copies `element.elbowed` as it is (`restore.ts:697`), so the
    /// restored arrow has none either; upstream reads `undefined` as not
    /// elbowed. An absent key is written back absent until the field
    /// changes.
    #[serde(default)]
    pub elbowed: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub fixed_segments: Option<Option<Vec<FixedSegment>>>,
    /// Use the third point as the second, hiding the first segment.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub start_is_special: Option<Option<bool>>,
    /// Use the third point from the end as the second-last, hiding the last
    /// segment.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub end_is_special: Option<Option<bool>>,
}

impl ArrowFields {
    /// Arrow fields as `newArrowElement` sets them (`newElement.ts:633-671`):
    /// an elbow arrow starts with no fixed segments and both `*IsSpecial`
    /// false; a plain arrow has none of those keys.
    pub fn new(linear: LinearFields, elbowed: bool) -> ArrowFields {
        let elbow = |v| elbowed.then_some(Some(v));
        ArrowFields {
            linear,
            elbowed,
            fixed_segments: elbowed.then_some(Some(Vec::new())),
            start_is_special: elbow(false),
            end_is_special: elbow(false),
        }
    }
}

/// Stroke variability of a freedraw (`StrokeVariability`, `types.ts:423`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrokeVariability {
    Variable,
    Constant,
}

/// Freedraw stroke options (`StrokeOptions`, `types.ts:425-428`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrokeOptions {
    pub variability: StrokeVariability,
    pub streamline: f64,
}

impl Default for StrokeOptions {
    /// `{variability: "variable", streamline: DEFAULT_STROKE_STREAMLINE}`
    /// (`newElement.ts:597-600`, `constants.ts:622`).
    fn default() -> StrokeOptions {
        StrokeOptions {
            variability: StrokeVariability::Variable,
            streamline: DEFAULT_STROKE_STREAMLINE,
        }
    }
}

/// Freedraw fields (`ExcalidrawFreeDrawElement`, `types.ts:430-437`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreedrawFields {
    pub points: Vec<LocalPoint>,
    pub pressures: Vec<f64>,
    pub simulate_pressure: bool,
    pub stroke_options: StrokeOptions,
}

impl FreedrawFields {
    /// Freedraw fields as `newFreeDrawElement` sets them
    /// (`newElement.ts:583-602`): no pressures, default stroke options.
    pub fn new(points: Vec<LocalPoint>, simulate_pressure: bool) -> FreedrawFields {
        FreedrawFields {
            points,
            pressures: Vec::new(),
            simulate_pressure,
            stroke_options: StrokeOptions::default(),
        }
    }
}

/// Generation state of an AI iframe (`MagicGenerationData`,
/// `types.ts:120-129`), stored in the iframe's `customData.generationData`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum MagicGenerationData {
    Pending,
    Done {
        html: String,
    },
    Error {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        code: String,
    },
}

/// Stroke width preset names (`StrokeWidthKey`, `constants.ts:472`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrokeWidthKey {
    Thin,
    Medium,
    Bold,
}

// ---------------------------------------------------------------------------
// The element

/// The per-type part of an element, tagged by the JSON `type` string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ElementKind {
    /// Never persisted.
    Selection,
    Rectangle,
    StickyNote(StickyNoteFields),
    Diamond,
    Ellipse,
    /// The embedded URL is the base `link`.
    Embeddable,
    /// Generation data, when any, is in the base `customData`; see
    /// [`Element::magic_generation_data`].
    Iframe,
    Image(ImageFields),
    Frame(FrameFields),
    MagicFrame(FrameFields),
    Text(TextFields),
    Line(LineFields),
    Arrow(ArrowFields),
    Freedraw(FreedrawFields),
}

impl ElementKind {
    /// The element type.
    pub const fn element_type(&self) -> ElementType {
        match self {
            ElementKind::Selection => ElementType::Selection,
            ElementKind::Rectangle => ElementType::Rectangle,
            ElementKind::StickyNote(_) => ElementType::StickyNote,
            ElementKind::Diamond => ElementType::Diamond,
            ElementKind::Ellipse => ElementType::Ellipse,
            ElementKind::Embeddable => ElementType::Embeddable,
            ElementKind::Iframe => ElementType::Iframe,
            ElementKind::Image(_) => ElementType::Image,
            ElementKind::Frame(_) => ElementType::Frame,
            ElementKind::MagicFrame(_) => ElementType::MagicFrame,
            ElementKind::Text(_) => ElementType::Text,
            ElementKind::Line(_) => ElementType::Line,
            ElementKind::Arrow(_) => ElementType::Arrow,
            ElementKind::Freedraw(_) => ElementType::Freedraw,
        }
    }

    /// The line or arrow fields.
    pub fn linear(&self) -> Option<&LinearFields> {
        match self {
            ElementKind::Line(l) => Some(&l.linear),
            ElementKind::Arrow(a) => Some(&a.linear),
            _ => None,
        }
    }

    /// The line or arrow fields, mutably.
    pub fn linear_mut(&mut self) -> Option<&mut LinearFields> {
        match self {
            ElementKind::Line(l) => Some(&mut l.linear),
            ElementKind::Arrow(a) => Some(&mut a.linear),
            _ => None,
        }
    }

    /// The points of a line, arrow or freedraw.
    pub fn points(&self) -> Option<&[LocalPoint]> {
        match self {
            ElementKind::Freedraw(f) => Some(&f.points),
            _ => self.linear().map(|l| l.points.as_slice()),
        }
    }
}

/// Arrow subtypes (`typeChecks.ts:130-157`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArrowSubtype {
    /// `elbowed` is true.
    Elbow,
    /// Not elbowed, `roundness` null.
    Sharp,
    /// Not elbowed, `roundness` set.
    Curved,
}

/// An element: shared fields, per-type fields, and keys the model does not
/// know, which are written back verbatim.
///
/// Serde writes an element as one JSON object the way upstream would write
/// the same JS object with `JSON.stringify`:
///
/// - an element built in Rust has its keys in the order upstream's
///   constructors create them (`newElement.ts:87-692`: `id`, `type`, the
///   base fields, `customData`, then the per-type fields), followed by any
///   [`extra`](Element::extra) keys in insertion order;
/// - an element read from JSON keeps the order it was read in, unknown keys
///   included, wherever they were; a key added later is appended, as a JS
///   property assignment appends it (`mutateElement.ts:80-100`);
/// - a known value the typed model reads in a normalised form (for example
///   `customData: null`, or an unknown key inside `boundElements`) is
///   written back as read until the field is changed.
/// - a string holding a lone UTF-16 surrogate (`"\ud83d"`) reads as U+FFFD
///   in the typed fields and in `extra`, and is written back as the escape
///   by [`crate::document::Document::to_json`] until the value changes;
///   U+FDD0 and every other character built in Rust is written as it is.
///
/// An `extra` key the element's type models (say `"x"`) is not written:
/// the typed field wins. Equality compares content, not key order.
#[derive(Debug, Clone)]
pub struct Element {
    pub base: ElementBase,
    pub kind: ElementKind,
    /// Keys the element's type does not model, as read.
    pub extra: Map<String, Value>,
    layout: Layout,
}

impl PartialEq for Element {
    fn eq(&self, other: &Element) -> bool {
        self.base == other.base && self.kind == other.kind && self.extra == other.extra
    }
}

impl Element {
    /// An element with no unknown keys.
    pub fn new(base: ElementBase, kind: ElementKind) -> Element {
        Element {
            base,
            kind,
            extra: Map::new(),
            layout: Layout::default(),
        }
    }

    /// Read an element from a JSON object: typed fields, unknown keys and
    /// the object's layout. Fails if the `type` is unknown or a field the
    /// type requires is missing or has the wrong type; restoring untyped
    /// input (defaults, legacy fields) is the restore module's job.
    pub fn from_map(raw: Map<String, Value>) -> Result<Element, serde_json::Error> {
        Element::from_encoded(&json::escape_map(&raw))
    }

    /// [`Element::from_map`] for an object in the sentinel form of
    /// [`crate::json`] (from [`json::parse`]). The typed fields and `extra`
    /// get the decoded values (a lone surrogate reads as U+FFFD); the
    /// layout keeps the raw ones.
    pub(crate) fn from_encoded(raw: &Map<String, Value>) -> Result<Element, serde_json::Error> {
        let decoded = Value::Object(json::decode_map(raw));
        let base = ElementBase::deserialize(&decoded)?;
        let kind = ElementKind::deserialize(&decoded)?;
        let typed = json::escape_map(&typed_map(&base, &kind));
        let (layout, extra) = Layout::read(raw, &typed, canonical_keys(kind.element_type()));
        Ok(Element {
            base,
            kind,
            extra,
            layout,
        })
    }

    /// The JSON object serde writes for this element. A lone surrogate read
    /// from a file is U+FFFD here; only [`crate::document::Document::to_json`]
    /// writes it back as its escape. Keys are in JS property order, as
    /// `JSON.stringify` writes them.
    pub fn to_map(&self) -> Map<String, Value> {
        json::ordered_like_js(json::decode_map(&self.to_encoded()))
    }

    /// The object to write, in the sentinel form of [`crate::json`].
    pub(crate) fn to_encoded(&self) -> Map<String, Value> {
        let typed = json::escape_map(&typed_map(&self.base, &self.kind));
        self.layout
            .write(&typed, &self.extra, canonical_keys(self.element_type()))
    }

    /// An image element as `newImageElement` builds it
    /// (`newElement.ts:673-692`): the base with `strokeColor` forced to
    /// `"transparent"`, whatever the base carried, and the image fields.
    pub fn new_image(mut base: ElementBase, fields: ImageFields) -> Element {
        base.stroke_color = COLOR_TRANSPARENT.to_owned();
        Element::new(base, ElementKind::Image(fields))
    }

    /// The element type.
    pub const fn element_type(&self) -> ElementType {
        self.kind.element_type()
    }

    /// Can be a binding target (`isBindableElement`,
    /// `typeChecks.ts:184-202`): a bindable type, and for text, only
    /// without a container (`None` or `""`, which is falsy upstream).
    pub fn is_bindable(&self) -> bool {
        match &self.kind {
            // `!element.containerId`: an empty string is falsy upstream.
            ElementKind::Text(t) => t.container_id.as_deref().is_none_or(str::is_empty),
            kind => kind.element_type().is_bindable(),
        }
    }

    /// The arrow subtype; `None` for other elements.
    pub fn arrow_subtype(&self) -> Option<ArrowSubtype> {
        let ElementKind::Arrow(arrow) = &self.kind else {
            return None;
        };
        Some(if arrow.elbowed {
            ArrowSubtype::Elbow
        } else if self.base.roundness.is_none() {
            ArrowSubtype::Sharp
        } else {
            ArrowSubtype::Curved
        })
    }

    /// An iframe's `customData.generationData`
    /// (`ExcalidrawIframeElement`, `types.ts:131-136`); `None` for other
    /// elements, or when absent or not a valid `MagicGenerationData`.
    pub fn magic_generation_data(&self) -> Option<MagicGenerationData> {
        if !matches!(self.kind, ElementKind::Iframe) {
            return None;
        }
        let data = self.base.custom_data.as_ref()?.get(GENERATION_DATA_KEY)?;
        MagicGenerationData::deserialize(data).ok()
    }

    /// Set an iframe's `customData.generationData`, keeping other
    /// `customData` keys.
    pub fn set_magic_generation_data(&mut self, data: MagicGenerationData) {
        let value = match serde_json::to_value(data) {
            Ok(v) => v,
            // Serialising an enum of strings to a Value cannot fail.
            Err(_) => return,
        };
        self.base
            .custom_data
            .get_or_insert_with(Map::new)
            .insert(GENERATION_DATA_KEY.to_owned(), value);
    }
}

const GENERATION_DATA_KEY: &str = "generationData";

impl Serialize for Element {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_map().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Element {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Element, D::Error> {
        let raw = Map::<String, Value>::deserialize(d)?;
        Element::from_map(raw).map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Key order

/// The keys `_newElementBase` creates, in its order
/// (`newElement.ts:141-169`). `customData` is created even when undefined
/// (which `JSON.stringify` skips), so when set it sits here, before the
/// per-type keys.
const BASE_KEYS: &[&str] = &[
    "id",
    "type",
    "x",
    "y",
    "width",
    "height",
    "angle",
    "strokeColor",
    "backgroundColor",
    "fillStyle",
    "strokeWidth",
    "strokeStyle",
    "roughness",
    "opacity",
    "groupIds",
    "frameId",
    "index",
    "roundness",
    "seed",
    "version",
    "versionNonce",
    "isDeleted",
    "boundElements",
    "updated",
    "created",
    "link",
    "locked",
    "customData",
];

/// `newStickyNoteElement`, `newElement.ts:230-243`.
const STICKY_NOTE_KEYS: &[&str] = &["baseHeight"];
/// `newImageElement`, `newElement.ts:682-691`.
const IMAGE_KEYS: &[&str] = &["status", "fileId", "scale", "crop"];
/// `newFrameElement` and `newMagicFrameElement`, `newElement.ts:263-295`.
const FRAME_KEYS: &[&str] = &["name"];
/// `newTextElement`, `newElement.ts:366-383` (`x`, `y`, `width` and
/// `height` are reassigned there but keep their base positions).
const TEXT_KEYS: &[&str] = &[
    "text",
    "fontSize",
    "baseFontSize",
    "fontFamily",
    "textAlign",
    "verticalAlign",
    "containerId",
    "originalText",
    "autoResize",
    "lineHeight",
    "labelPosition",
];
/// `newLinearElement`, `newElement.ts:611-626`.
const LINE_KEYS: &[&str] = &[
    "points",
    "startBinding",
    "endBinding",
    "startArrowhead",
    "endArrowhead",
    "polygon",
];
/// `newArrowElement`, `newElement.ts:645-670`.
const ARROW_KEYS: &[&str] = &[
    "points",
    "startBinding",
    "endBinding",
    "startArrowhead",
    "endArrowhead",
    "elbowed",
    "fixedSegments",
    "startIsSpecial",
    "endIsSpecial",
];
/// `newFreeDrawElement`, `newElement.ts:592-601`.
const FREEDRAW_KEYS: &[&str] = &["points", "pressures", "simulatePressure", "strokeOptions"];

/// Every key an element of this type may have, in the order upstream's
/// constructor creates them.
fn canonical_keys(ty: ElementType) -> Canonical<'static> {
    match ty {
        ElementType::Selection
        | ElementType::Rectangle
        | ElementType::Diamond
        | ElementType::Ellipse
        | ElementType::Embeddable
        | ElementType::Iframe => &[BASE_KEYS],
        ElementType::StickyNote => &[BASE_KEYS, STICKY_NOTE_KEYS],
        ElementType::Image => &[BASE_KEYS, IMAGE_KEYS],
        ElementType::Frame | ElementType::MagicFrame => &[BASE_KEYS, FRAME_KEYS],
        ElementType::Text => &[BASE_KEYS, TEXT_KEYS],
        ElementType::Line => &[BASE_KEYS, LINE_KEYS],
        ElementType::Arrow => &[BASE_KEYS, ARROW_KEYS],
        ElementType::Freedraw => &[BASE_KEYS, FREEDRAW_KEYS],
    }
}

/// The keys and values the typed model writes for an element.
fn typed_map(base: &ElementBase, kind: &ElementKind) -> Map<String, Value> {
    let mut map = object_of(base);
    map.extend(object_of(kind));
    map
}

/// A struct as a JSON object. Cannot fail for the model's types: every map
/// key is a string, and a non-finite number becomes `null`, as
/// `JSON.stringify` writes it.
pub(crate) fn object_of<T: Serialize>(value: &T) -> Map<String, Value> {
    match serde_json::to_value(value) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

// ---------------------------------------------------------------------------
// serde helpers

/// `Option<Option<T>>` for a key that may be absent (`None`), `null`
/// (`Some(None)`) or set. Use with `default` and
/// `skip_serializing_if = "Option::is_none"`.
mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T: Serialize, S: Serializer>(
        value: &Option<Option<T>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(inner) => inner.serialize(s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Option<T>>, D::Error> {
        Option::<T>::deserialize(d).map(Some)
    }
}
