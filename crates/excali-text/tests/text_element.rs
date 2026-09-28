//! Bound-text sizing: `packages/element/src/textElement.ts` at the pinned
//! commit (`getContainerCoords` 396-417, `computeContainerDimensionForBoundText`
//! 521-538, `getBoundTextMaxWidth` 540-569, `getBoundTextMaxHeight` 571-599,
//! `computeBoundTextPosition` 249-324); research page
//! `site/content/research/rendering.md`, section 2, "Bound text".
//!
//! The first part ports `packages/element/tests/textElement.test.ts` case for
//! case (its `getContainerCoords`, `computeContainerDimensionForBoundText`,
//! `getBoundTextMaxWidth`, `getBoundTextMaxHeight` and
//! `computeBoundTextPosition` blocks). Upstream's tests leave arrow labels
//! and sticky notes out of `getBoundTextMaxWidth`; the second part pins them
//! from the source, with the constants of `packages/common/src/constants.ts`
//! (`BOUND_TEXT_PADDING` 421, `ARROW_LABEL_WIDTH_FRACTION` 422,
//! `ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO` 423, `STICKY_NOTE_PADDING` 228,
//! `STICKY_NOTE_BODY_INSET_Y` 258).

mod elements;

use excali_core::constants::{
    ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO, ARROW_LABEL_WIDTH_FRACTION, BOUND_TEXT_PADDING,
    STICKY_NOTE_BODY_INSET_Y, STICKY_NOTE_FOOTER_HEIGHT, STICKY_NOTE_PADDING,
};
use excali_core::element::{Element, ElementType};
use excali_text::text_element::{
    compute_bound_text_position, compute_container_dimension_for_bound_text,
    get_bound_text_max_height, get_bound_text_max_width, get_container_coords, js_round,
    ArrowLabelGeometry, NoArrowGeometry,
};
use serde_json::json;

use elements::element;

#[test]
fn constants_are_upstreams() {
    assert_eq!(BOUND_TEXT_PADDING, 5.0);
    assert_eq!(ARROW_LABEL_WIDTH_FRACTION, 0.7);
    assert_eq!(ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO, 11.0);
    assert_eq!(STICKY_NOTE_PADDING, 16.0);
    assert_eq!(STICKY_NOTE_FOOTER_HEIGHT, 20.0);
    assert_eq!(STICKY_NOTE_BODY_INSET_Y, 52.0);
}

/// `Math.round`: ties go up, towards positive infinity.
#[test]
fn js_round_ties_towards_positive_infinity() {
    assert_eq!(js_round(2.5), 3.0);
    assert_eq!(js_round(-2.5), -2.0);
    assert_eq!(js_round(-2.6), -3.0);
    assert_eq!(js_round(0.499_999_999_999_999_94), 0.0);
    assert_eq!(js_round(125.865_007_849_837_52), 126.0);
    assert!(js_round(f64::NAN).is_nan());
}

// textElement.test.ts:18-53
mod get_container_coords {
    use super::*;

    fn container(ty: &str) -> Element {
        element(
            ty,
            "c",
            json!({ "width": 200, "height": 100, "x": 10, "y": 20 }),
        )
    }

    #[test]
    fn ellipse() {
        assert_eq!(
            get_container_coords(&container("ellipse")),
            [44.289_321_881_345_245, 39.644_660_940_672_62]
        );
    }

    #[test]
    fn rectangle() {
        assert_eq!(get_container_coords(&container("rectangle")), [15.0, 25.0]);
    }

    #[test]
    fn diamond() {
        assert_eq!(get_container_coords(&container("diamond")), [65.0, 50.0]);
    }

    /// A sticky note pads its label by `STICKY_NOTE_PADDING` (16), not 5.
    #[test]
    fn sticky_note() {
        assert_eq!(get_container_coords(&container("stickynote")), [26.0, 36.0]);
    }

    /// Arrows get the plain padding (their label is positioned along the
    /// path instead, see `computeBoundTextPosition`).
    #[test]
    fn arrow() {
        assert_eq!(get_container_coords(&container("arrow")), [15.0, 25.0]);
    }
}

// textElement.test.ts:55-90
mod compute_container_dimension_for_bound_text {
    use super::*;

    #[test]
    fn rectangle() {
        assert_eq!(
            compute_container_dimension_for_bound_text(150.0, ElementType::Rectangle),
            160.0
        );
    }

    #[test]
    fn ellipse() {
        assert_eq!(
            compute_container_dimension_for_bound_text(150.0, ElementType::Ellipse),
            226.0
        );
    }

    #[test]
    fn diamond() {
        assert_eq!(
            compute_container_dimension_for_bound_text(150.0, ElementType::Diamond),
            320.0
        );
    }

    /// `dimension + padding * 8`: 10 * 8 on top of the ceiling.
    #[test]
    fn arrow() {
        assert_eq!(
            compute_container_dimension_for_bound_text(150.0, ElementType::Arrow),
            230.0
        );
    }

    /// The dimension is rounded up first.
    #[test]
    fn ceil_first() {
        assert_eq!(
            compute_container_dimension_for_bound_text(149.01, ElementType::Rectangle),
            160.0
        );
        assert_eq!(
            compute_container_dimension_for_bound_text(149.01, ElementType::Diamond),
            320.0
        );
        // round((150 + 10) / √2 * 2) = round(226.27...)
        assert_eq!(
            compute_container_dimension_for_bound_text(149.01, ElementType::Ellipse),
            226.0
        );
    }

    /// A sticky note falls through to the rectangle rule.
    #[test]
    fn sticky_note() {
        assert_eq!(
            compute_container_dimension_for_bound_text(150.0, ElementType::StickyNote),
            160.0
        );
    }
}

// textElement.test.ts:92-112, then arrow and sticky note from the source.
mod get_bound_text_max_width {
    use super::*;

    fn container(ty: &str, fields: serde_json::Value) -> Element {
        let mut c = element(ty, "c", json!({ "width": 178, "height": 194 }));
        if let Some(width) = fields.get("width").and_then(|w| w.as_f64()) {
            c.base.width = width;
        }
        c
    }

    fn text(font_size: f64) -> Element {
        element("text", "t", json!({ "fontSize": font_size }))
    }

    #[test]
    fn rectangle() {
        assert_eq!(
            get_bound_text_max_width(&container("rectangle", json!({})), None),
            168.0
        );
    }

    #[test]
    fn ellipse() {
        assert_eq!(
            get_bound_text_max_width(&container("ellipse", json!({})), None),
            116.0
        );
    }

    #[test]
    fn diamond() {
        assert_eq!(
            get_bound_text_max_width(&container("diamond", json!({})), None),
            79.0
        );
    }

    /// `round(w/2 · √2) - 10` rounds before subtracting the padding.
    #[test]
    fn ellipse_rounds_the_inscribed_width() {
        // 100 / 2 * √2 = 70.71 -> 71
        assert_eq!(
            get_bound_text_max_width(&container("ellipse", json!({ "width": 100 })), None),
            61.0
        );
    }

    /// `round(w / 2) - 10`, ties up: 101 / 2 = 50.5 -> 51.
    #[test]
    fn diamond_rounds_half_up() {
        assert_eq!(
            get_bound_text_max_width(&container("diamond", json!({ "width": 101 })), None),
            41.0
        );
    }

    /// A short arrow: the label may be as wide as `fontSize * 11`.
    #[test]
    fn arrow_minimum_is_font_size_times_eleven() {
        let arrow = container("arrow", json!({}));
        // max(0.7 * 178, 20 * 11) = 220
        assert_eq!(get_bound_text_max_width(&arrow, Some(&text(20.0))), 220.0);
        // max(124.6, 36 * 11) = 396
        assert_eq!(get_bound_text_max_width(&arrow, Some(&text(36.0))), 396.0);
    }

    /// A long arrow: 70% of its width.
    #[test]
    fn arrow_width_fraction() {
        let arrow = container("arrow", json!({ "width": 1000 }));
        assert_eq!(get_bound_text_max_width(&arrow, Some(&text(20.0))), 700.0);
        // 0.7 * 400 = 280.00000000000006 in doubles, as upstream computes it
        let arrow = container("arrow", json!({ "width": 400 }));
        assert_eq!(
            get_bound_text_max_width(&arrow, Some(&text(20.0))),
            0.7 * 400.0
        );
    }

    /// Without a label the default font size (20) sets the minimum.
    #[test]
    fn arrow_without_label_uses_default_font_size() {
        let arrow = container("arrow", json!({}));
        assert_eq!(get_bound_text_max_width(&arrow, None), 220.0);
    }

    /// Only arrows are measured along their length; a line is a rectangle
    /// here (it is no text container upstream).
    #[test]
    fn line_falls_through() {
        let line = container("line", json!({}));
        assert_eq!(get_bound_text_max_width(&line, None), 168.0);
    }

    /// A sticky note pads by 16 on each side.
    #[test]
    fn sticky_note() {
        assert_eq!(
            get_bound_text_max_width(&container("stickynote", json!({})), None),
            146.0
        );
        assert_eq!(
            get_bound_text_max_width(&container("stickynote", json!({ "width": 250 })), None),
            218.0
        );
    }
}

// textElement.test.ts:114-179
mod get_bound_text_max_height {
    use super::*;

    fn bound_text() -> Element {
        element(
            "text",
            "text-id",
            json!({
                "x": 560.51171875,
                "y": 202.033203125,
                "width": 154,
                "height": 175,
                "fontSize": 20,
                "fontFamily": 1,
                "text": "Excalidraw is a\nvirtual \nopensource \nwhiteboard for \nsketching \nhand-drawn like\ndiagrams",
                "textAlign": "center",
                "verticalAlign": "middle",
                "containerId": "\"container-id"
            }),
        )
    }

    fn container(ty: &str, height: f64) -> Element {
        element(
            ty,
            "\"container-id",
            json!({ "width": 178, "height": height }),
        )
    }

    #[test]
    fn rectangle() {
        assert_eq!(
            get_bound_text_max_height(&container("rectangle", 194.0), &bound_text()),
            184.0
        );
    }

    #[test]
    fn ellipse() {
        assert_eq!(
            get_bound_text_max_height(&container("ellipse", 194.0), &bound_text()),
            127.0
        );
    }

    #[test]
    fn diamond() {
        assert_eq!(
            get_bound_text_max_height(&container("diamond", 194.0), &bound_text()),
            87.0
        );
    }

    #[test]
    fn arrow() {
        assert_eq!(
            get_bound_text_max_height(&container("arrow", 194.0), &bound_text()),
            194.0
        );
    }

    #[test]
    fn arrow_below_threshold() {
        assert_eq!(
            get_bound_text_max_height(&container("arrow", 70.0), &bound_text()),
            175.0
        );
        // 80 - 80 = 0 is still at or below the threshold
        assert_eq!(
            get_bound_text_max_height(&container("arrow", 80.0), &bound_text()),
            175.0
        );
        assert_eq!(
            get_bound_text_max_height(&container("arrow", 81.0), &bound_text()),
            81.0
        );
    }

    /// The body ends above the footer: `height - 52`, never below 0.
    #[test]
    fn sticky_note() {
        assert_eq!(
            get_bound_text_max_height(&container("stickynote", 250.0), &bound_text()),
            198.0
        );
        assert_eq!(
            get_bound_text_max_height(&container("stickynote", 40.0), &bound_text()),
            0.0
        );
    }
}

// textElement.test.ts:212-378
mod compute_bound_text_position {
    use super::*;

    fn case(text_align: &str, vertical_align: &str) -> [f64; 2] {
        let container = element(
            "rectangle",
            "c",
            json!({
                "x": 100,
                "y": 100,
                "width": 200,
                "height": 100,
                "angle": std::f64::consts::PI / 2.0
            }),
        );
        let text = element(
            "text",
            "t",
            json!({
                "width": 80,
                "height": 40,
                "text": "hello darkness my old friend",
                "textAlign": text_align,
                "verticalAlign": vertical_align,
                "containerId": "c"
            }),
        );
        compute_bound_text_position(&container, &text, &[], &mut NoArrowGeometry)
            .expect("a rectangle needs no arrow geometry")
    }

    fn close(got: [f64; 2], want: [f64; 2]) {
        // toBeCloseTo(_, 1): |got - want| < 0.05
        assert!(
            (got[0] - want[0]).abs() < 0.05 && (got[1] - want[1]).abs() < 0.05,
            "got {got:?}, want {want:?}"
        );
    }

    #[test]
    fn left_top() {
        close(case("left", "top"), [185.0, 75.0]);
    }

    #[test]
    fn left_middle() {
        close(case("left", "middle"), [160.0, 75.0]);
    }

    #[test]
    fn left_bottom() {
        close(case("left", "bottom"), [135.0, 75.0]);
    }

    #[test]
    fn center_top() {
        close(case("center", "top"), [185.0, 130.0]);
    }

    #[test]
    fn center_middle() {
        close(case("center", "middle"), [160.0, 130.0]);
    }

    #[test]
    fn center_bottom() {
        close(case("center", "bottom"), [135.0, 130.0]);
    }

    #[test]
    fn right_top() {
        close(case("right", "top"), [185.0, 185.0]);
    }

    #[test]
    fn right_middle() {
        close(case("right", "middle"), [160.0, 185.0]);
    }

    #[test]
    fn right_bottom() {
        close(case("right", "bottom"), [135.0, 185.0]);
    }

    /// Unrotated, inside an ellipse: the inset corner plus half the spare
    /// room of the inscribed rectangle.
    #[test]
    fn ellipse_center_middle() {
        let container = element(
            "ellipse",
            "c",
            json!({ "x": 10, "y": 20, "width": 200, "height": 100 }),
        );
        let text = element(
            "text",
            "t",
            json!({ "width": 50, "height": 25, "textAlign": "center",
                    "verticalAlign": "middle", "containerId": "c" }),
        );
        let [x, y] =
            compute_bound_text_position(&container, &text, &[], &mut NoArrowGeometry).unwrap();
        let coords = get_container_coords(&container);
        let max_w = js_round(100.0 * 2f64.sqrt()) - 10.0;
        let max_h = js_round(50.0 * 2f64.sqrt()) - 10.0;
        assert_eq!(x, coords[0] + (max_w / 2.0 - 25.0));
        assert_eq!(y, coords[1] + (max_h / 2.0 - 12.5));
    }

    /// A sticky label in the middle is centred in the whole padded note
    /// while it clears the footer, then pushed up against the body's end.
    #[test]
    fn sticky_note_middle() {
        let note = element(
            "stickynote",
            "n",
            json!({ "x": 0, "y": 0, "width": 250, "height": 250 }),
        );
        let label = |height: f64| {
            element(
                "text",
                "t",
                json!({ "width": 100, "height": height, "textAlign": "center",
                        "verticalAlign": "middle", "containerId": "n" }),
            )
        };
        // padded 218: (218 - 50) / 2 = 84 < 198 - 50
        let [x, y] =
            compute_bound_text_position(&note, &label(50.0), &[], &mut NoArrowGeometry).unwrap();
        assert_eq!([x, y], [16.0 + (218.0 / 2.0 - 50.0), 16.0 + 84.0]);
        // (218 - 190) / 2 = 14 > 198 - 190 = 8
        let [_, y] =
            compute_bound_text_position(&note, &label(190.0), &[], &mut NoArrowGeometry).unwrap();
        assert_eq!(y, 16.0 + 8.0);
    }

    /// An arrow label's position comes from the arrow's geometry
    /// (`LinearElementEditor.getBoundTextElementPosition`).
    #[test]
    fn arrow_asks_the_geometry() {
        struct Fixed;
        impl ArrowLabelGeometry for Fixed {
            fn bound_text_element_position(
                &mut self,
                arrow: &Element,
                text: &Element,
                _elements: &[Element],
            ) -> Option<[f64; 2]> {
                assert_eq!(arrow.base.id, "a");
                assert_eq!(text.base.id, "t");
                Some([7.0, 9.0])
            }
        }
        let arrow = element("arrow", "a", json!({}));
        let text = element("text", "t", json!({ "containerId": "a" }));
        assert_eq!(
            compute_bound_text_position(&arrow, &text, &[], &mut Fixed),
            Some([7.0, 9.0])
        );
        assert_eq!(
            compute_bound_text_position(&arrow, &text, &[], &mut NoArrowGeometry),
            None
        );
    }
}
