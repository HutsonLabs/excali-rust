//! The element model pins upstream's element types at the pinned commit
//! 438d89861f53d8a90ad566113ecac1b83761098f.
//!
//! Sources (paths relative to `.tools/upstream`):
//!
//! - `packages/element/src/types.ts:18-437`: `_ExcalidrawElementBase`
//!   (40-87), every per-type interface (89-437) and the supporting types;
//! - `packages/element/src/typeChecks.ts:130-202, 255-284, 322-373`: type
//!   strings and groupings; `packages/element/src/textElement.ts:508-514`:
//!   text containers; `packages/common/src/constants.ts:542-546`: library
//!   disabled types;
//! - `packages/element/src/newElement.ts:87-172, 583-692`: construction
//!   defaults; `packages/common/src/constants.ts:122-167, 223-275, 425-532,
//!   622`: enumeration constants; `packages/common/src/colors.ts:193-196`;
//! - `packages/element/src/arrowheads.ts:3-21`: legacy arrowhead names;
//! - the JSON documents under "Upstream snapshots" are the restored elements
//!   in `packages/excalidraw/tests/data/__snapshots__/restore.test.ts.snap`
//!   (vendored under `fixtures/upstream`), with `Any<Number>` replaced by a
//!   number, `customData: undefined` dropped (JSON has no `undefined`) and
//!   the snapshot serializer's `"0.50000"`-style floats written as numbers.

use excali_core::constants::{
    self, DEFAULT_ELEMENT_PROPS, FONT_SIZES, FREEDRAW_STROKE_WIDTH, ROUGHNESS, STROKE_WIDTH,
};
use excali_core::element::{
    ArrowFields, ArrowSubtype, Arrowhead, BindMode, BoundElement, BoundElementType, Element,
    ElementBase, ElementKind, ElementType, FileId, FillStyle, FixedPointBinding, FixedSegment,
    FontFamily, FractionalIndex, FrameFields, FreedrawFields, ImageCrop, ImageFields, ImageStatus,
    LineFields, LinearFields, MagicGenerationData, Radians, Roundness, RoundnessType,
    StickyNoteFields, StrokeOptions, StrokeStyle, StrokeVariability, StrokeWidthKey, TextAlign,
    TextFields, VerticalAlign,
};
use serde_json::{json, Map, Value};

// ---------------------------------------------------------------------------
// Helpers

/// A base as `_newElementBase` builds it (`newElement.ts:87-172`) with the
/// timestamp and seed fixed.
fn base(id: &str) -> ElementBase {
    ElementBase::new(id, 10.0, 20.0, 12345.0, 1.0)
}

/// Rewrite every number as an f64 so that `100` and `100.0` compare equal:
/// JSON numbers are JS numbers, so upstream cannot tell them apart either.
fn numbers_as_f64(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().expect("finite number")),
        Value::Array(items) => Value::Array(items.iter().map(numbers_as_f64).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), numbers_as_f64(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Read one element document into the typed base and kind, write both back
/// and merge them. Every key of the document must come back with the same
/// value: that pins each field's JSON name and type.
fn typed_round_trip(doc: &Value) -> (ElementBase, ElementKind) {
    let base: ElementBase =
        serde_json::from_value(doc.clone()).unwrap_or_else(|e| panic!("base fields of {doc}: {e}"));
    let kind: ElementKind =
        serde_json::from_value(doc.clone()).unwrap_or_else(|e| panic!("type fields of {doc}: {e}"));
    let mut merged: Map<String, Value> = match serde_json::to_value(&base).unwrap() {
        Value::Object(map) => map,
        other => panic!("base serialised to {other}"),
    };
    match serde_json::to_value(&kind).unwrap() {
        Value::Object(map) => {
            for (k, v) in map {
                assert!(
                    merged.insert(k.clone(), v).is_none(),
                    "key {k} written by both base and kind"
                );
            }
        }
        other => panic!("kind serialised to {other}"),
    }
    assert_eq!(
        numbers_as_f64(&Value::Object(merged)),
        numbers_as_f64(doc),
        "typed round trip of {doc}"
    );
    (base, kind)
}

/// The 26 `_ExcalidrawElementBase` keys, `types.ts:41-85` (plus the optional
/// `customData`, 86), in declaration order.
const BASE_KEYS: [&str; 26] = [
    "id",
    "x",
    "y",
    "strokeColor",
    "backgroundColor",
    "fillStyle",
    "strokeWidth",
    "strokeStyle",
    "roundness",
    "roughness",
    "opacity",
    "width",
    "height",
    "angle",
    "seed",
    "version",
    "versionNonce",
    "index",
    "isDeleted",
    "groupIds",
    "frameId",
    "boundElements",
    "updated",
    "created",
    "link",
    "locked",
];

fn base_json(id: &str, ty: &str) -> Map<String, Value> {
    // elementFixture.ts:7-32 (`elementBase`), with the given id and type.
    let Value::Object(mut map) = json!({
        "id": id,
        "type": ty,
        "x": 414,
        "y": 237,
        "width": 214,
        "height": 214,
        "angle": 0,
        "strokeColor": "#000000",
        "backgroundColor": "#15aabf",
        "fillStyle": "hachure",
        "strokeWidth": 1,
        "strokeStyle": "solid",
        "roughness": 1,
        "opacity": 100,
        "groupIds": [],
        "frameId": null,
        "roundness": null,
        "index": null,
        "seed": 1041657908,
        "version": 120,
        "versionNonce": 1188004276,
        "isDeleted": false,
        "boundElements": null,
        "updated": 1,
        "created": null,
        "link": null,
        "locked": false
    }) else {
        unreachable!()
    };
    map.shift_remove("type");
    map.insert("type".into(), json!(ty));
    map
}

fn with(mut map: Map<String, Value>, extra: Value) -> Value {
    let Value::Object(extra) = extra else {
        panic!("extra must be an object")
    };
    map.extend(extra);
    Value::Object(map)
}

// ---------------------------------------------------------------------------
// Type strings (types.ts:223-234, typeChecks.ts:255-284)

const TYPE_STRINGS: [&str; 14] = [
    "selection",
    "rectangle",
    "stickynote",
    "diamond",
    "ellipse",
    "embeddable",
    "iframe",
    "image",
    "frame",
    "magicframe",
    "text",
    "line",
    "arrow",
    "freedraw",
];

#[test]
fn element_type_covers_exactly_the_fourteen_upstream_type_strings() {
    let all: Vec<&str> = ElementType::ALL.iter().map(|t| t.as_str()).collect();
    assert_eq!(all, TYPE_STRINGS);
    for s in TYPE_STRINGS {
        let ty = ElementType::parse(s).expect(s);
        assert_eq!(ty.as_str(), s);
        assert_eq!(ty.to_string(), s);
    }
}

#[test]
fn legacy_draw_and_unknown_types_are_not_element_types() {
    // "draw" is migrated to "line" by restore (restore.ts:612-637); unknown
    // types make restoreElement return null (restore.ts:747-751). Neither
    // is a type of the model.
    for s in ["draw", "Rectangle", "", "group", "laser", "hand", "eraser"] {
        assert_eq!(ElementType::parse(s), None, "{s}");
    }
}

// ---------------------------------------------------------------------------
// Base fields (types.ts:40-87) and construction defaults (newElement.ts)

#[test]
fn new_base_has_new_element_base_defaults() {
    // newElement.ts:87-172 with DEFAULT_ELEMENT_PROPS (constants.ts:514-532).
    let b = ElementBase::new("abc", 10.0, 20.0, 42.0, 1_700_000_000_000.0);
    assert_eq!(b.id, "abc");
    assert_eq!((b.x, b.y), (10.0, 20.0));
    assert_eq!((b.width, b.height), (0.0, 0.0));
    assert_eq!(b.angle, Radians(0.0));
    assert_eq!(b.stroke_color, "#1e1e1e");
    assert_eq!(b.background_color, "transparent");
    assert_eq!(b.fill_style, FillStyle::Solid);
    assert_eq!(b.stroke_width, 2.0);
    assert_eq!(b.stroke_style, StrokeStyle::Solid);
    assert_eq!(b.roughness, 1.0);
    assert_eq!(b.opacity, 100.0);
    assert_eq!(b.roundness, None);
    assert_eq!(b.seed, 42.0);
    assert_eq!(b.version, 1.0);
    assert_eq!(b.version_nonce, 0.0);
    assert_eq!(b.index, None);
    assert!(!b.is_deleted);
    assert!(b.group_ids.is_empty());
    assert_eq!(b.frame_id, None);
    assert_eq!(b.bound_elements, None);
    assert_eq!(b.updated, 1_700_000_000_000.0);
    assert_eq!(b.created, Some(1_700_000_000_000.0));
    assert_eq!(b.link, None);
    assert!(!b.locked);
    assert_eq!(b.custom_data, None);
}

#[test]
fn base_serialises_exactly_the_documented_keys() {
    let value = serde_json::to_value(base("k")).unwrap();
    let keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, BASE_KEYS);

    let mut with_data = base("k");
    with_data.custom_data = Some(Map::from_iter([("a".to_owned(), json!(1))]));
    let value = serde_json::to_value(&with_data).unwrap();
    assert_eq!(value["customData"], json!({"a": 1}));
    assert_eq!(value.as_object().unwrap().len(), 27);
}

#[test]
fn base_field_types_accept_every_documented_value() {
    let doc = with(
        base_json("full", "rectangle"),
        json!({
            "roundness": {"type": 3, "value": 12.5},
            "index": "a0V",
            "groupIds": ["inner", "outer"],
            "frameId": "frame-1",
            "boundElements": [{"id": "arrow-1", "type": "arrow"}, {"id": "t", "type": "text"}],
            "created": 1_700_000_000_000_u64,
            "link": "https://excalidraw.com",
            "locked": true,
            "isDeleted": true,
            "angle": 0.785,
            "customData": {"nested": {"k": [1, "two", null]}}
        }),
    );
    let (b, kind) = typed_round_trip(&doc);
    assert_eq!(kind, ElementKind::Rectangle);
    assert_eq!(
        b.roundness,
        Some(Roundness {
            kind: RoundnessType::AdaptiveRadius,
            value: Some(12.5)
        })
    );
    assert_eq!(b.index, Some(FractionalIndex("a0V".into())));
    assert_eq!(b.group_ids, ["inner", "outer"]);
    assert_eq!(b.frame_id.as_deref(), Some("frame-1"));
    assert_eq!(
        b.bound_elements,
        Some(vec![
            BoundElement {
                id: "arrow-1".into(),
                kind: BoundElementType::Arrow
            },
            BoundElement {
                id: "t".into(),
                kind: BoundElementType::Text
            },
        ])
    );
    assert_eq!(b.created, Some(1_700_000_000_000.0));
    assert_eq!(b.link.as_deref(), Some("https://excalidraw.com"));
    assert!(b.locked && b.is_deleted);
    assert_eq!(b.angle, Radians(0.785));
    assert_eq!(
        b.custom_data.unwrap()["nested"]["k"],
        json!([1, "two", null])
    );
}

#[test]
fn roundness_without_value_omits_the_key() {
    // `roundness: null | { type: RoundnessType; value?: number }`
    let r = Roundness::new(RoundnessType::ProportionalRadius);
    assert_eq!(serde_json::to_value(r).unwrap(), json!({"type": 2}));
    let r: Roundness = serde_json::from_value(json!({"type": 1})).unwrap();
    assert_eq!(r, Roundness::new(RoundnessType::Legacy));
    assert!(serde_json::from_value::<Roundness>(json!({"type": 4})).is_err());
    assert!(serde_json::from_value::<Roundness>(json!({"type": "3"})).is_err());
}

// ---------------------------------------------------------------------------
// Enumerations (types.ts, constants.ts)

fn assert_strings<T>(cases: &[(T, &str)])
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    for (value, s) in cases {
        assert_eq!(serde_json::to_value(value).unwrap(), json!(s), "{value:?}");
        assert_eq!(&serde_json::from_value::<T>(json!(s)).unwrap(), value);
    }
}

#[test]
fn string_enumerations_use_upstream_spellings() {
    assert_strings(&[
        (FillStyle::Hachure, "hachure"),
        (FillStyle::CrossHatch, "cross-hatch"),
        (FillStyle::Solid, "solid"),
        (FillStyle::Zigzag, "zigzag"),
    ]);
    assert_strings(&[
        (StrokeStyle::Solid, "solid"),
        (StrokeStyle::Dashed, "dashed"),
        (StrokeStyle::Dotted, "dotted"),
    ]);
    assert_strings(&[
        (TextAlign::Left, "left"),
        (TextAlign::Center, "center"),
        (TextAlign::Right, "right"),
    ]);
    assert_strings(&[
        (VerticalAlign::Top, "top"),
        (VerticalAlign::Middle, "middle"),
        (VerticalAlign::Bottom, "bottom"),
    ]);
    assert_strings(&[
        (ImageStatus::Pending, "pending"),
        (ImageStatus::Saved, "saved"),
        (ImageStatus::Error, "error"),
    ]);
    assert_strings(&[
        (BindMode::Inside, "inside"),
        (BindMode::Orbit, "orbit"),
        (BindMode::Skip, "skip"),
    ]);
    assert_strings(&[
        (StrokeVariability::Variable, "variable"),
        (StrokeVariability::Constant, "constant"),
    ]);
    assert_strings(&[
        (BoundElementType::Arrow, "arrow"),
        (BoundElementType::Text, "text"),
    ]);
    assert_strings(&[
        (Arrowhead::Arrow, "arrow"),
        (Arrowhead::Bar, "bar"),
        (Arrowhead::Circle, "circle"),
        (Arrowhead::CircleOutline, "circle_outline"),
        (Arrowhead::Triangle, "triangle"),
        (Arrowhead::TriangleOutline, "triangle_outline"),
        (Arrowhead::Diamond, "diamond"),
        (Arrowhead::DiamondOutline, "diamond_outline"),
        (Arrowhead::CardinalityOne, "cardinality_one"),
        (Arrowhead::CardinalityMany, "cardinality_many"),
        (Arrowhead::CardinalityOneOrMany, "cardinality_one_or_many"),
        (Arrowhead::CardinalityExactlyOne, "cardinality_exactly_one"),
        (Arrowhead::CardinalityZeroOrOne, "cardinality_zero_or_one"),
        (Arrowhead::CardinalityZeroOrMany, "cardinality_zero_or_many"),
    ]);
    assert_eq!(Arrowhead::ALL.len(), 14);
    for s in ["Solid", "cross_hatch", "dot", ""] {
        assert!(
            serde_json::from_value::<FillStyle>(json!(s)).is_err(),
            "{s}"
        );
    }
}

#[test]
fn legacy_arrowheads_normalise_like_upstream() {
    // arrowheads.ts:3-21
    assert_eq!(Arrowhead::normalize("dot"), Some(Arrowhead::Circle));
    assert_eq!(
        Arrowhead::normalize("crowfoot_one"),
        Some(Arrowhead::CardinalityOne)
    );
    assert_eq!(
        Arrowhead::normalize("crowfoot_many"),
        Some(Arrowhead::CardinalityMany)
    );
    assert_eq!(
        Arrowhead::normalize("crowfoot_one_or_many"),
        Some(Arrowhead::CardinalityOneOrMany)
    );
    for a in Arrowhead::ALL {
        assert_eq!(Arrowhead::normalize(a.as_str()), Some(a));
    }
    assert_eq!(Arrowhead::normalize("unknown"), None);
}

#[test]
fn roundness_types_are_the_rounding_constants() {
    // ROUNDNESS, constants.ts:447-464
    assert_eq!(RoundnessType::Legacy.value(), 1);
    assert_eq!(RoundnessType::ProportionalRadius.value(), 2);
    assert_eq!(RoundnessType::AdaptiveRadius.value(), 3);
    assert_eq!(constants::DEFAULT_PROPORTIONAL_RADIUS, 0.25);
    assert_eq!(constants::DEFAULT_ADAPTIVE_RADIUS, 32.0);
    for t in [
        RoundnessType::Legacy,
        RoundnessType::ProportionalRadius,
        RoundnessType::AdaptiveRadius,
    ] {
        assert_eq!(serde_json::to_value(t).unwrap(), json!(t.value()));
        assert_eq!(RoundnessType::from_value(t.value()), Some(t));
    }
    assert_eq!(RoundnessType::from_value(0), None);
}

#[test]
fn font_family_ids_match_upstream() {
    // FONT_FAMILY constants.ts:140-151, fallbacks 158-167, default 268.
    let ids = [
        (FontFamily::VIRGIL, 1, "Virgil"),
        (FontFamily::HELVETICA, 2, "Helvetica"),
        (FontFamily::CASCADIA, 3, "Cascadia"),
        (FontFamily::EXCALIFONT, 5, "Excalifont"),
        (FontFamily::NUNITO, 6, "Nunito"),
        (FontFamily::LILITA_ONE, 7, "Lilita One"),
        (FontFamily::COMIC_SHANNS, 8, "Comic Shanns"),
        (FontFamily::LIBERATION_SANS, 9, "Liberation Sans"),
        (FontFamily::ASSISTANT, 10, "Assistant"),
        (FontFamily::XIAOLAI, 100, "Xiaolai"),
        (FontFamily::SANS_SERIF, 998, "sans-serif"),
        (FontFamily::MONOSPACE, 999, "monospace"),
        (FontFamily::SEGOE_UI_EMOJI, 1000, "Segoe UI Emoji"),
    ];
    for (family, id, name) in ids {
        assert_eq!(family, FontFamily(id));
        assert_eq!(family.name(), Some(name));
        assert_eq!(FontFamily::from_name(name), Some(family));
        assert_eq!(serde_json::to_value(family).unwrap(), json!(id));
    }
    assert_eq!(FontFamily::ELEMENT_FAMILIES.len(), 9);
    assert!(!FontFamily::ELEMENT_FAMILIES.contains(&FontFamily::XIAOLAI));
    // 4 is reserved (historically Assistant or an Obsidian custom font) and
    // must still read and write as a number.
    assert_eq!(FontFamily(4).name(), None);
    assert_eq!(
        serde_json::from_value::<FontFamily>(json!(4)).unwrap(),
        FontFamily(4)
    );
    assert_eq!(FontFamily::DEFAULT, FontFamily::EXCALIFONT);
    assert_eq!(FontFamily::default(), FontFamily::EXCALIFONT);
}

#[test]
fn style_constants_match_upstream() {
    // constants.ts:122-127, 223, 274-275, 466-532, 622; colors.ts:194-195.
    assert_eq!(
        (FONT_SIZES.sm, FONT_SIZES.md, FONT_SIZES.lg, FONT_SIZES.xl),
        (16.0, 20.0, 28.0, 36.0)
    );
    assert_eq!(constants::DEFAULT_FONT_SIZE, 20.0);
    assert_eq!(constants::MIN_FONT_SIZE, 1.0);
    assert_eq!(constants::DEFAULT_TEXT_ALIGN, TextAlign::Left);
    assert_eq!(constants::DEFAULT_VERTICAL_ALIGN, VerticalAlign::Top);
    assert_eq!(
        (ROUGHNESS.architect, ROUGHNESS.artist, ROUGHNESS.cartoonist),
        (0.0, 1.0, 2.0)
    );
    assert_eq!(
        (
            STROKE_WIDTH.thin,
            STROKE_WIDTH.medium,
            STROKE_WIDTH.bold,
            STROKE_WIDTH.extra_bold
        ),
        (1.0, 2.0, 4.0, 8.0)
    );
    assert_eq!(
        (
            FREEDRAW_STROKE_WIDTH.thin,
            FREEDRAW_STROKE_WIDTH.medium,
            FREEDRAW_STROKE_WIDTH.bold,
            FREEDRAW_STROKE_WIDTH.extra_bold
        ),
        (0.5, 1.0, 2.0, 4.0)
    );
    // getStrokeWidthByKey, constants.ts:503-510
    for ty in ElementType::ALL {
        let expected = if ty == ElementType::Freedraw {
            [0.5, 1.0, 2.0]
        } else {
            [1.0, 2.0, 4.0]
        };
        let got = [
            StrokeWidthKey::Thin,
            StrokeWidthKey::Medium,
            StrokeWidthKey::Bold,
        ]
        .map(|k| constants::stroke_width_by_key(ty, k));
        assert_eq!(got, expected, "{ty}");
    }
    assert_eq!(
        constants::DEFAULT_ELEMENT_STROKE_WIDTH_KEY,
        StrokeWidthKey::Medium
    );
    let p = DEFAULT_ELEMENT_PROPS;
    assert_eq!(p.stroke_color, "#1e1e1e");
    assert_eq!(p.background_color, "transparent");
    assert_eq!(p.fill_style, FillStyle::Solid);
    assert_eq!(p.stroke_width, 2.0);
    assert_eq!(p.stroke_style, StrokeStyle::Solid);
    assert_eq!(p.roughness, 1.0);
    assert_eq!(p.opacity, 100.0);
    assert!(!p.locked);
    assert_eq!(constants::DEFAULT_STROKE_STREAMLINE, 0.5);
    assert_eq!(constants::COLOR_BLACK, "#1e1e1e");
    assert_eq!(constants::COLOR_TRANSPARENT, "transparent");
}

// ---------------------------------------------------------------------------
// Constructing every variant

fn every_variant() -> Vec<Element> {
    let linear = LinearFields::new(vec![[0.0, 0.0], [100.0, 50.0]]);
    vec![
        Element::new(base("selection"), ElementKind::Selection),
        Element::new(base("rectangle"), ElementKind::Rectangle),
        Element::new(
            base("stickynote"),
            ElementKind::StickyNote(StickyNoteFields { base_height: 200.0 }),
        ),
        Element::new(base("diamond"), ElementKind::Diamond),
        Element::new(base("ellipse"), ElementKind::Ellipse),
        Element::new(base("embeddable"), ElementKind::Embeddable),
        Element::new(base("iframe"), ElementKind::Iframe),
        Element::new(base("image"), ElementKind::Image(ImageFields::default())),
        Element::new(
            base("frame"),
            ElementKind::Frame(FrameFields { name: None }),
        ),
        Element::new(
            base("magicframe"),
            ElementKind::MagicFrame(FrameFields {
                name: Some("Magic".into()),
            }),
        ),
        Element::new(
            base("text"),
            ElementKind::Text(TextFields::new("hello", FontFamily::EXCALIFONT, 1.25)),
        ),
        Element::new(
            base("line"),
            ElementKind::Line(LineFields {
                linear: linear.clone(),
                polygon: false,
            }),
        ),
        Element::new(
            base("arrow"),
            ElementKind::Arrow(ArrowFields::new(linear, false)),
        ),
        Element::new(
            base("freedraw"),
            ElementKind::Freedraw(FreedrawFields::new(vec![[0.0, 0.0], [1.0, 1.0]], true)),
        ),
    ]
}

#[test]
fn every_variant_constructs_and_reports_its_type_string() {
    let elements = every_variant();
    let types: Vec<&str> = elements.iter().map(|e| e.element_type().as_str()).collect();
    assert_eq!(types, TYPE_STRINGS);
    for e in &elements {
        assert_eq!(e.base.id, e.element_type().as_str());
        assert_eq!(e.kind.element_type(), e.element_type());
        assert!(e.extra.is_empty());
    }
}

#[test]
fn every_variant_serialises_with_its_type_tag_and_fields_and_reads_back() {
    let expected_keys: [&[&str]; 14] = [
        &[],
        &[],
        &["baseHeight"],
        &[],
        &[],
        &[],
        &[],
        &["fileId", "status", "scale", "crop"],
        &["name"],
        &["name"],
        &[
            "fontSize",
            "fontFamily",
            "baseFontSize",
            "text",
            "textAlign",
            "verticalAlign",
            "containerId",
            "originalText",
            "autoResize",
            "lineHeight",
            "labelPosition",
        ],
        &[
            "points",
            "startBinding",
            "endBinding",
            "startArrowhead",
            "endArrowhead",
            "polygon",
        ],
        &[
            "points",
            "startBinding",
            "endBinding",
            "startArrowhead",
            "endArrowhead",
            "elbowed",
        ],
        &["points", "pressures", "simulatePressure", "strokeOptions"],
    ];
    for (e, keys) in every_variant().iter().zip(expected_keys) {
        let value = serde_json::to_value(&e.kind).unwrap();
        let map = value.as_object().unwrap();
        let mut want = vec!["type"];
        want.extend_from_slice(keys);
        let got: Vec<&str> = map.keys().map(String::as_str).collect();
        assert_eq!(got, want, "{}", e.element_type());
        assert_eq!(map["type"], json!(e.element_type().as_str()));
        let back: ElementKind = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(back, e.kind);
    }
}

#[test]
fn per_type_constructors_use_upstream_defaults() {
    // newImageElement, newElement.ts:673-692
    let image = ImageFields::default();
    assert_eq!(image.file_id, None);
    assert_eq!(image.status, ImageStatus::Pending);
    assert_eq!(image.scale, [1.0, 1.0]);
    assert_eq!(image.crop, None);
    // newImageElement forces the base strokeColor to "transparent"; the
    // image constructor applies it over whatever the base carried.
    let image = Element::new_image(base("img"), ImageFields::default());
    assert_eq!(image.element_type(), ElementType::Image);
    assert_eq!(image.base.stroke_color, "transparent");
    assert_eq!(image.base.background_color, "transparent");
    assert_eq!(image.kind, ElementKind::Image(ImageFields::default()));
    assert!(image.extra.is_empty());
    let value = serde_json::to_value(&image.base).unwrap();
    assert_eq!(value["strokeColor"], json!("transparent"));

    // newFreeDrawElement, newElement.ts:583-602
    let free = FreedrawFields::new(vec![], false);
    assert!(free.points.is_empty() && free.pressures.is_empty());
    assert!(!free.simulate_pressure);
    assert_eq!(
        free.stroke_options,
        StrokeOptions {
            variability: StrokeVariability::Variable,
            streamline: 0.5
        }
    );
    assert_eq!(StrokeOptions::default(), free.stroke_options);

    // newLinearElement, newElement.ts:604-631
    let linear = LinearFields::new(vec![[0.0, 0.0]]);
    assert_eq!(linear.points, vec![[0.0, 0.0]]);
    assert_eq!(linear.start_binding, None);
    assert_eq!(linear.end_binding, None);
    assert_eq!(linear.start_arrowhead, None);
    assert_eq!(linear.end_arrowhead, None);

    // newArrowElement, newElement.ts:633-671: a simple arrow has no elbow
    // keys; an elbow arrow starts with fixedSegments [] and both flags false.
    let simple = ArrowFields::new(linear.clone(), false);
    assert!(!simple.elbowed);
    assert_eq!(simple.fixed_segments, None);
    assert_eq!(simple.start_is_special, None);
    assert_eq!(simple.end_is_special, None);
    let elbow = ArrowFields::new(linear, true);
    assert!(elbow.elbowed);
    assert_eq!(elbow.fixed_segments, Some(Some(vec![])));
    assert_eq!(elbow.start_is_special, Some(Some(false)));
    assert_eq!(elbow.end_is_special, Some(Some(false)));
    let value = serde_json::to_value(ElementKind::Arrow(elbow)).unwrap();
    assert_eq!(value["fixedSegments"], json!([]));
    assert_eq!(value["startIsSpecial"], json!(false));
    assert_eq!(value["endIsSpecial"], json!(false));

    // newTextElement, newElement.ts:366-383 (dimensions come from text
    // measurement, which is not part of the model)
    let text = TextFields::new("a\nb", FontFamily::VIRGIL, 1.25);
    assert_eq!(text.text, "a\nb");
    assert_eq!(text.original_text, "a\nb");
    assert_eq!(text.font_size, 20.0);
    assert_eq!(text.font_family, FontFamily::VIRGIL);
    assert_eq!(text.base_font_size, None);
    assert_eq!(text.text_align, TextAlign::Left);
    assert_eq!(text.vertical_align, VerticalAlign::Top);
    assert_eq!(text.container_id, None);
    assert!(text.auto_resize);
    assert_eq!(text.line_height, 1.25);
    assert_eq!(text.label_position, Some(None));
}

// ---------------------------------------------------------------------------
// Upstream snapshots and fixtures, read into the typed model and written back

#[test]
fn restored_rectangle_ellipse_and_diamond_snapshots_round_trip() {
    for (id, ty, index, roundness) in [
        ("1", "rectangle", "a0", 3),
        ("2", "ellipse", "a1", 2),
        ("3", "diamond", "a2", 2),
    ] {
        let doc = json!({
          "angle": 0,
          "backgroundColor": "blue",
          "boundElements": [],
          "created": 1,
          "fillStyle": "cross-hatch",
          "frameId": null,
          "groupIds": ["1", "2", "3"],
          "height": 200,
          "id": id,
          "index": index,
          "isDeleted": false,
          "link": null,
          "locked": false,
          "opacity": 10,
          "roughness": 2,
          "roundness": {"type": roundness},
          "seed": 7,
          "strokeColor": "red",
          "strokeStyle": "dashed",
          "strokeWidth": 2,
          "type": ty,
          "updated": 1,
          "version": 2,
          "versionNonce": 8,
          "width": 100,
          "x": 10,
          "y": 20
        });
        let (b, kind) = typed_round_trip(&doc);
        assert_eq!(kind.element_type().as_str(), ty);
        assert_eq!(b.fill_style, FillStyle::CrossHatch);
        assert_eq!(b.stroke_style, StrokeStyle::Dashed);
        assert_eq!(b.bound_elements, Some(vec![]));
        assert_eq!(b.index, Some(FractionalIndex(index.into())));
        assert_eq!(b.roundness.unwrap().kind.value(), roundness);
    }
}

#[test]
fn restored_arrow_snapshot_round_trips() {
    let doc = json!({
      "angle": 0,
      "backgroundColor": "transparent",
      "boundElements": [],
      "created": 1,
      "elbowed": false,
      "endArrowhead": null,
      "endBinding": null,
      "fillStyle": "solid",
      "frameId": null,
      "groupIds": [],
      "height": 100,
      "id": "id-arrow01",
      "index": "a0",
      "isDeleted": false,
      "link": null,
      "locked": false,
      "opacity": 100,
      "points": [[0, 0], [100, 100]],
      "roughness": 1,
      "roundness": null,
      "seed": 1,
      "startArrowhead": null,
      "startBinding": null,
      "strokeColor": "#1e1e1e",
      "strokeStyle": "solid",
      "strokeWidth": 2,
      "type": "arrow",
      "updated": 1,
      "version": 2,
      "versionNonce": 2,
      "width": 100,
      "x": 0,
      "y": 0
    });
    let (_, kind) = typed_round_trip(&doc);
    let ElementKind::Arrow(arrow) = kind else {
        panic!("not an arrow: {kind:?}")
    };
    assert_eq!(arrow.linear.points, vec![[0.0, 0.0], [100.0, 100.0]]);
    assert!(!arrow.elbowed);
    assert_eq!(arrow.fixed_segments, None);
}

#[test]
fn restored_line_snapshot_round_trips() {
    let doc = json!({
      "angle": 0,
      "backgroundColor": "transparent",
      "boundElements": [],
      "created": 1,
      "endArrowhead": null,
      "endBinding": null,
      "fillStyle": "solid",
      "frameId": null,
      "groupIds": [],
      "height": 100,
      "id": "id-line01",
      "index": "a0",
      "isDeleted": false,
      "link": null,
      "locked": false,
      "opacity": 100,
      "points": [[0, 0], [100, 100]],
      "polygon": false,
      "roughness": 1,
      "roundness": null,
      "seed": 1,
      "startArrowhead": null,
      "startBinding": null,
      "strokeColor": "#1e1e1e",
      "strokeStyle": "solid",
      "strokeWidth": 2,
      "type": "line",
      "updated": 1,
      "version": 2,
      "versionNonce": 3,
      "width": 100,
      "x": 0,
      "y": 0
    });
    let (_, kind) = typed_round_trip(&doc);
    let ElementKind::Line(line) = kind else {
        panic!("not a line: {kind:?}")
    };
    assert!(!line.polygon);
}

#[test]
fn restored_freedraw_snapshot_round_trips() {
    let doc = json!({
      "angle": 0,
      "backgroundColor": "transparent",
      "boundElements": [],
      "created": 1,
      "fillStyle": "solid",
      "frameId": null,
      "groupIds": [],
      "height": 100,
      "id": "id-freedraw01",
      "index": "a0",
      "isDeleted": false,
      "link": null,
      "locked": false,
      "opacity": 100,
      "points": [[0, 0], [10, 10]],
      "pressures": [],
      "roughness": 1,
      "roundness": null,
      "seed": 1,
      "simulatePressure": true,
      "strokeColor": "#1e1e1e",
      "strokeOptions": {"streamline": 0.5, "variability": "variable"},
      "strokeStyle": "solid",
      "strokeWidth": 1,
      "type": "freedraw",
      "updated": 1,
      "version": 2,
      "versionNonce": 4,
      "width": 100,
      "x": 0,
      "y": 0
    });
    let (b, kind) = typed_round_trip(&doc);
    assert_eq!(b.stroke_width, 1.0);
    let ElementKind::Freedraw(free) = kind else {
        panic!("not freedraw: {kind:?}")
    };
    assert!(free.simulate_pressure);
    assert_eq!(free.stroke_options, StrokeOptions::default());
}

#[test]
fn restored_text_snapshot_round_trips() {
    let doc = json!({
      "angle": 0,
      "autoResize": true,
      "backgroundColor": "transparent",
      "baseFontSize": null,
      "boundElements": [],
      "containerId": null,
      "created": 1,
      "fillStyle": "solid",
      "fontFamily": 1,
      "fontSize": 14,
      "frameId": null,
      "groupIds": [],
      "height": 100,
      "id": "id-text01",
      "index": "a0",
      "isDeleted": false,
      "labelPosition": null,
      "lineHeight": 1.25,
      "link": null,
      "locked": false,
      "opacity": 100,
      "originalText": "text",
      "roughness": 1,
      "roundness": null,
      "seed": 1,
      "strokeColor": "#1e1e1e",
      "strokeStyle": "solid",
      "strokeWidth": 2,
      "text": "text",
      "textAlign": "center",
      "type": "text",
      "updated": 1,
      "version": 2,
      "versionNonce": 5,
      "verticalAlign": "middle",
      "width": 100,
      "x": -20,
      "y": -8.75
    });
    let (b, kind) = typed_round_trip(&doc);
    assert_eq!(b.y, -8.75);
    let ElementKind::Text(text) = kind else {
        panic!("not text: {kind:?}")
    };
    assert_eq!(text.font_family, FontFamily::VIRGIL);
    assert_eq!(text.font_size, 14.0);
    assert_eq!(text.text_align, TextAlign::Center);
    assert_eq!(text.vertical_align, VerticalAlign::Middle);
    assert_eq!(text.label_position, Some(None));
}

#[test]
fn text_fixture_without_label_position_keeps_the_key_absent() {
    // elementFixture.ts textFixture: no labelPosition key at all
    // (`labelPosition?: number | null`, types.ts:290).
    let doc = with(
        base_json("vWrqOAfkind2qcm7LDAGZ", "text"),
        json!({
            "fontSize": 20,
            "baseFontSize": null,
            "fontFamily": 5,
            "strokeColor": "#1e1e1e",
            "text": "original text",
            "originalText": "original text",
            "textAlign": "left",
            "verticalAlign": "top",
            "containerId": null,
            "lineHeight": 1.25,
            "autoResize": false
        }),
    );
    let (_, kind) = typed_round_trip(&doc);
    let ElementKind::Text(text) = kind else {
        panic!("not text")
    };
    assert_eq!(text.label_position, None);
    assert!(!text.auto_resize);

    let doc = with(
        base_json("label", "text"),
        json!({
            "fontSize": 16, "baseFontSize": 28, "fontFamily": 6, "text": "a",
            "originalText": "a", "textAlign": "right", "verticalAlign": "bottom",
            "containerId": "arrow-1", "lineHeight": 1.25, "autoResize": true,
            "labelPosition": 0.3
        }),
    );
    let (_, kind) = typed_round_trip(&doc);
    let ElementKind::Text(text) = kind else {
        panic!("not text")
    };
    assert_eq!(text.label_position, Some(Some(0.3)));
    assert_eq!(text.base_font_size, Some(28.0));
    assert_eq!(text.container_id.as_deref(), Some("arrow-1"));
}

#[test]
fn generic_fixtures_round_trip() {
    // elementFixture.ts rectangleFixture, embeddableFixture, ellipseFixture,
    // diamondFixture, rectangleWithLinkFixture.
    for (ty, kind) in [
        ("rectangle", ElementKind::Rectangle),
        ("embeddable", ElementKind::Embeddable),
        ("ellipse", ElementKind::Ellipse),
        ("diamond", ElementKind::Diamond),
        ("selection", ElementKind::Selection),
        ("iframe", ElementKind::Iframe),
    ] {
        let doc = Value::Object(base_json("vWrqOAfkind2qcm7LDAGZ", ty));
        assert_eq!(typed_round_trip(&doc).1, kind);
    }
    let doc = with(
        base_json("vWrqOAfkind2qcm7LDAGZ", "rectangle"),
        json!({"link": "excalidraw.com"}),
    );
    assert_eq!(
        typed_round_trip(&doc).0.link.as_deref(),
        Some("excalidraw.com")
    );
}

#[test]
fn stickynote_image_and_frame_documents_round_trip() {
    let doc = with(
        base_json("note", "stickynote"),
        json!({"backgroundColor": "#ffdf6b", "fillStyle": "solid", "baseHeight": 180}),
    );
    assert_eq!(
        typed_round_trip(&doc).1,
        ElementKind::StickyNote(StickyNoteFields { base_height: 180.0 })
    );

    let doc = with(
        base_json("img", "image"),
        json!({
            "strokeColor": "transparent",
            "fileId": "0123456789abcdef0123456789abcdef01234567",
            "status": "saved",
            "scale": [-1, 1],
            "crop": {"x": 1, "y": 2, "width": 30, "height": 40,
                     "naturalWidth": 300, "naturalHeight": 400}
        }),
    );
    let (_, kind) = typed_round_trip(&doc);
    assert_eq!(
        kind,
        ElementKind::Image(ImageFields {
            file_id: Some(FileId("0123456789abcdef0123456789abcdef01234567".into())),
            status: ImageStatus::Saved,
            scale: [-1.0, 1.0],
            crop: Some(ImageCrop {
                x: 1.0,
                y: 2.0,
                width: 30.0,
                height: 40.0,
                natural_width: 300.0,
                natural_height: 400.0
            }),
        })
    );
    let doc = with(
        base_json("img2", "image"),
        json!({"fileId": null, "status": "pending", "scale": [1, 1], "crop": null}),
    );
    assert_eq!(
        typed_round_trip(&doc).1,
        ElementKind::Image(ImageFields::default())
    );

    for ty in ["frame", "magicframe"] {
        for name in [json!(null), json!("Frame 1")] {
            let doc = with(base_json("f", ty), json!({ "name": name }));
            let (_, kind) = typed_round_trip(&doc);
            let fields = FrameFields {
                name: name.as_str().map(str::to_owned),
            };
            let expected = if ty == "frame" {
                ElementKind::Frame(fields)
            } else {
                ElementKind::MagicFrame(fields)
            };
            assert_eq!(kind, expected);
        }
    }
}

#[test]
fn bound_elbow_arrow_document_round_trips() {
    let doc = with(
        base_json("elbow", "arrow"),
        json!({
            "roundness": null,
            "points": [[0, 0], [50, 0], [50, 80], [120, 80]],
            "startBinding": {"elementId": "r1", "fixedPoint": [1, 0.5], "mode": "orbit"},
            "endBinding": {"elementId": "r2", "fixedPoint": [0, 0.5001], "mode": "inside"},
            "startArrowhead": "cardinality_zero_or_many",
            "endArrowhead": "triangle_outline",
            "elbowed": true,
            "fixedSegments": [{"start": [50, 0], "end": [50, 80], "index": 2}],
            "startIsSpecial": null,
            "endIsSpecial": true
        }),
    );
    let (_, kind) = typed_round_trip(&doc);
    let ElementKind::Arrow(arrow) = kind else {
        panic!("not an arrow")
    };
    assert_eq!(
        arrow.linear.start_binding,
        Some(FixedPointBinding {
            element_id: "r1".into(),
            fixed_point: [1.0, 0.5],
            mode: BindMode::Orbit
        })
    );
    assert_eq!(
        arrow.linear.end_binding.as_ref().map(|b| b.mode),
        Some(BindMode::Inside)
    );
    assert_eq!(
        arrow.linear.start_arrowhead,
        Some(Arrowhead::CardinalityZeroOrMany)
    );
    assert_eq!(arrow.linear.end_arrowhead, Some(Arrowhead::TriangleOutline));
    assert_eq!(
        arrow.fixed_segments,
        Some(Some(vec![FixedSegment {
            start: [50.0, 0.0],
            end: [50.0, 80.0],
            index: 2.0
        }]))
    );
    assert_eq!(arrow.start_is_special, Some(None));
    assert_eq!(arrow.end_is_special, Some(Some(true)));

    // restore writes `fixedSegments: null` for elbow arrows with fewer than
    // four points (restore.ts:656-723): null is kept distinct from absent.
    let doc = with(
        base_json("elbow2", "arrow"),
        json!({
            "points": [[0, 0], [10, 10]], "startBinding": null, "endBinding": null,
            "startArrowhead": null, "endArrowhead": "arrow", "elbowed": true,
            "fixedSegments": null, "startIsSpecial": false, "endIsSpecial": false
        }),
    );
    let (_, kind) = typed_round_trip(&doc);
    let ElementKind::Arrow(arrow) = kind else {
        panic!("not an arrow")
    };
    assert_eq!(arrow.fixed_segments, Some(None));
}

#[test]
fn a_type_field_mismatch_is_rejected() {
    let bad = [
        with(
            base_json("x", "image"),
            json!({"fileId": null, "status": "done", "scale": [1, 1], "crop": null}),
        ),
        with(
            base_json("x", "image"),
            json!({"fileId": null, "status": "saved", "scale": [1], "crop": null}),
        ),
        with(base_json("x", "frame"), json!({"name": 3})),
        with(
            base_json("x", "line"),
            json!({"points": [[0, 0, 0]], "startBinding": null, "endBinding": null, "startArrowhead": null, "endArrowhead": null, "polygon": false}),
        ),
        with(
            base_json("x", "arrow"),
            json!({"points": [], "startBinding": null, "endBinding": null, "startArrowhead": "dot", "endArrowhead": null, "elbowed": false}),
        ),
        with(base_json("x", "draw"), json!({})),
        with(base_json("x", "unknown"), json!({})),
    ];
    for doc in bad {
        assert!(
            serde_json::from_value::<ElementKind>(doc.clone()).is_err(),
            "{doc}"
        );
    }
    let bad_base = [
        with(base_json("x", "rectangle"), json!({"fillStyle": "dots"})),
        with(base_json("x", "rectangle"), json!({"strokeStyle": "wavy"})),
        with(
            base_json("x", "rectangle"),
            json!({"boundElements": [{"id": "a", "type": "line"}]}),
        ),
        with(base_json("x", "rectangle"), json!({"groupIds": "g"})),
        with(base_json("x", "rectangle"), json!({"x": "1"})),
    ];
    for doc in bad_base {
        assert!(
            serde_json::from_value::<ElementBase>(doc.clone()).is_err(),
            "{doc}"
        );
    }
}

// ---------------------------------------------------------------------------
// Iframe customData (types.ts:120-136)

#[test]
fn magic_generation_data_is_read_from_iframe_custom_data() {
    for (data, expected) in [
        (json!({"status": "pending"}), MagicGenerationData::Pending),
        (
            json!({"status": "done", "html": "<p>hi</p>"}),
            MagicGenerationData::Done {
                html: "<p>hi</p>".into(),
            },
        ),
        (
            json!({"status": "error", "code": "ERR_GENERATION_INTERRUPTED"}),
            MagicGenerationData::Error {
                message: None,
                code: "ERR_GENERATION_INTERRUPTED".into(),
            },
        ),
        (
            json!({"status": "error", "message": "boom", "code": "X"}),
            MagicGenerationData::Error {
                message: Some("boom".into()),
                code: "X".into(),
            },
        ),
    ] {
        let mut e = Element::new(base("i"), ElementKind::Iframe);
        assert_eq!(e.magic_generation_data(), None);
        e.set_magic_generation_data(expected.clone());
        assert_eq!(e.base.custom_data.as_ref().unwrap()["generationData"], data);
        assert_eq!(e.magic_generation_data(), Some(expected));
    }
    // Other elements never carry generation data.
    let mut rect = Element::new(base("r"), ElementKind::Rectangle);
    rect.base.custom_data = Some(Map::from_iter([(
        "generationData".to_owned(),
        json!({"status": "pending"}),
    )]));
    assert_eq!(rect.magic_generation_data(), None);
}

// ---------------------------------------------------------------------------
// Type groupings (types.ts:188-216, 293-310; typeChecks.ts; textElement.ts)

fn types_where(pred: impl Fn(ElementType) -> bool) -> Vec<&'static str> {
    ElementType::ALL
        .into_iter()
        .filter(|t| pred(*t))
        .map(ElementType::as_str)
        .collect()
}

#[test]
fn type_groupings_match_upstream() {
    assert_eq!(types_where(ElementType::is_linear), ["line", "arrow"]);
    assert_eq!(types_where(ElementType::is_binding), ["arrow"]);
    // canChangeRoundness, comparisons.ts:57-64.
    assert_eq!(
        types_where(ElementType::can_change_roundness),
        [
            "rectangle",
            "stickynote",
            "diamond",
            "embeddable",
            "iframe",
            "image",
            "line"
        ]
    );
    assert_eq!(
        types_where(ElementType::is_bindable),
        [
            "rectangle",
            "stickynote",
            "diamond",
            "ellipse",
            "embeddable",
            "iframe",
            "image",
            "frame",
            "magicframe",
            "text"
        ]
    );
    assert_eq!(
        types_where(ElementType::is_text_container),
        ["rectangle", "stickynote", "diamond", "ellipse", "arrow"]
    );
    assert_eq!(
        types_where(ElementType::is_frame_like),
        ["frame", "magicframe"]
    );
    assert_eq!(
        types_where(ElementType::is_iframe_like),
        ["embeddable", "iframe"]
    );
    assert_eq!(
        types_where(ElementType::is_flowchart_node),
        ["rectangle", "stickynote", "diamond", "ellipse"]
    );
    assert_eq!(
        types_where(ElementType::uses_adaptive_radius),
        ["rectangle", "embeddable", "iframe", "image"]
    );
    assert_eq!(
        types_where(ElementType::uses_proportional_radius),
        ["stickynote", "diamond", "line", "arrow"]
    );
    assert_eq!(
        types_where(ElementType::is_library_disabled),
        ["embeddable", "iframe", "image"]
    );
    // getDefaultRoundnessTypeForElement, typeChecks.ts:357-373
    for t in ElementType::ALL {
        let want = if t.uses_proportional_radius() {
            Some(Roundness::new(RoundnessType::ProportionalRadius))
        } else if t.uses_adaptive_radius() {
            Some(Roundness::new(RoundnessType::AdaptiveRadius))
        } else {
            None
        };
        assert_eq!(t.default_roundness(), want, "{t}");
    }
}

#[test]
fn text_is_bindable_only_without_a_container() {
    // typeChecks.ts:184-202
    let mut text = Element::new(
        base("t"),
        ElementKind::Text(TextFields::new("x", FontFamily::DEFAULT, 1.25)),
    );
    assert!(text.is_bindable());
    if let ElementKind::Text(fields) = &mut text.kind {
        fields.container_id = Some("rect".into());
    }
    assert!(!text.is_bindable());
    // `!element.containerId`: an empty string is falsy, so upstream treats
    // text with containerId "" as having no container, and restore keeps
    // the value (restore.ts:569).
    if let ElementKind::Text(fields) = &mut text.kind {
        fields.container_id = Some(String::new());
    }
    assert!(text.is_bindable());
    for e in every_variant() {
        if e.element_type() != ElementType::Text {
            assert_eq!(e.is_bindable(), e.element_type().is_bindable());
        }
    }
}

#[test]
fn arrow_subtypes_follow_elbowed_and_roundness() {
    // typeChecks.ts:130-157
    let linear = LinearFields::new(vec![[0.0, 0.0], [1.0, 1.0]]);
    let mut arrow = Element::new(
        base("a"),
        ElementKind::Arrow(ArrowFields::new(linear.clone(), false)),
    );
    assert_eq!(arrow.arrow_subtype(), Some(ArrowSubtype::Sharp));
    arrow.base.roundness = Some(Roundness::new(RoundnessType::ProportionalRadius));
    assert_eq!(arrow.arrow_subtype(), Some(ArrowSubtype::Curved));
    let elbow = Element::new(
        base("e"),
        ElementKind::Arrow(ArrowFields::new(linear.clone(), true)),
    );
    assert_eq!(elbow.arrow_subtype(), Some(ArrowSubtype::Elbow));
    let line = Element::new(
        base("l"),
        ElementKind::Line(LineFields {
            linear,
            polygon: false,
        }),
    );
    assert_eq!(line.arrow_subtype(), None);
}

#[test]
fn linear_accessors_reach_points_of_lines_arrows_and_freedraw() {
    for e in every_variant() {
        let points = e.kind.points();
        match e.element_type() {
            ElementType::Line | ElementType::Arrow => {
                assert_eq!(points, Some(&[[0.0, 0.0], [100.0, 50.0]][..]));
                assert!(e.kind.linear().is_some());
            }
            ElementType::Freedraw => {
                assert_eq!(points, Some(&[[0.0, 0.0], [1.0, 1.0]][..]));
                assert!(e.kind.linear().is_none());
            }
            _ => {
                assert_eq!(points, None);
                assert!(e.kind.linear().is_none());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Keys restore leaves out (restore.ts:645-650, 697)

#[test]
fn absent_elbowed_and_polygon_read_as_false_and_stay_absent() {
    let linear = json!({"points": [[0, 0], [10, 0]], "startBinding": null, "endBinding": null,
        "startArrowhead": null, "endArrowhead": null});
    for ty in ["arrow", "line"] {
        let Value::Object(raw) = with(base_json("x", ty), linear.clone()) else {
            unreachable!()
        };
        let element = Element::from_map(raw.clone()).expect("reads");
        match &element.kind {
            ElementKind::Arrow(arrow) => assert!(!arrow.elbowed),
            ElementKind::Line(line) => assert!(!line.polygon),
            other => panic!("{other:?}"),
        }
        let back = element.to_map();
        assert!(back.keys().eq(raw.keys()), "{ty}");
        assert!(!back.contains_key("elbowed") && !back.contains_key("polygon"));

        // Changed, the field is written.
        let mut changed = element.clone();
        match &mut changed.kind {
            ElementKind::Arrow(arrow) => arrow.elbowed = true,
            ElementKind::Line(line) => line.polygon = true,
            _ => unreachable!(),
        }
        let key = if ty == "arrow" { "elbowed" } else { "polygon" };
        assert_eq!(changed.to_map()[key], json!(true));
    }
}
