//! The whole image a cropped image shows part of
//! (`packages/element/src/cropElement.ts`): what the crop editor draws
//! faintly under the image being cropped (`renderElement.ts:1220-1251`)
//! and what cropping measures against (`excali_editor::crop`).
//!
//! [`get_uncropped_image_element`] is the uncropped image placed so the
//! crop sits where the element is, [`get_uncropped_width_and_height`] its
//! size.

use excali_core::element::{Element, ElementKind, ImageCrop};
use excali_math::{
    point_from, point_from_vector, point_rotate_rads, vector_add, vector_from_point,
    vector_normalize, vector_scale, vector_subtract, GlobalPoint, Point, Radians,
};

use crate::bounds::{get_element_absolute_coords, ElementsMap};

/// The image's crop and scale; an element that is not an image has
/// neither (scale `[1, 1]`).
pub fn image_crop_and_scale(element: &Element) -> (Option<ImageCrop>, [f64; 2]) {
    match &element.kind {
        ElementKind::Image(fields) => (fields.crop, fields.scale),
        _ => (None, [1.0, 1.0]),
    }
}

/// `getUncroppedImageElement(element, elementsMap)`
/// (`cropElement.ts:477-547`): the whole image a cropped image shows part
/// of, placed so the crop sits where the element is, with no crop. An
/// uncropped element is returned as is.
pub fn get_uncropped_image_element(element: &Element, elements_map: &ElementsMap<'_>) -> Element {
    let (crop, scale) = image_crop_and_scale(element);
    let Some(crop) = crop else {
        return element.clone();
    };
    let (width, height) = get_uncropped_width_and_height(element);

    let [x1, y1, x2, y2, cx, cy] = get_element_absolute_coords(element, elements_map, false);
    let angle = Radians(element.base.angle.0);
    let center: GlobalPoint = point_from(cx, cy);
    let origin = Point::ORIGIN;

    let top_left_vector =
        vector_from_point(point_rotate_rads(point_from(x1, y1), center, angle), origin);
    let top_right_vector =
        vector_from_point(point_rotate_rads(point_from(x2, y1), center, angle), origin);
    let top_edge_normalized = vector_normalize(vector_subtract(top_right_vector, top_left_vector));
    let bottom_left_vector =
        vector_from_point(point_rotate_rads(point_from(x1, y2), center, angle), origin);
    let left_edge_vector = vector_subtract(bottom_left_vector, top_left_vector);
    let left_edge_normalized = vector_normalize(left_edge_vector);

    let [crop_x, crop_y] = adjust_crop_position(&crop, scale);

    let rotated_top_left = vector_add(
        vector_add(
            top_left_vector,
            vector_scale(top_edge_normalized, (-crop_x * width) / crop.natural_width),
        ),
        vector_scale(
            left_edge_normalized,
            (-crop_y * height) / crop.natural_height,
        ),
    );

    let center: GlobalPoint = point_from_vector(
        vector_add(
            vector_add(
                rotated_top_left,
                vector_scale(top_edge_normalized, width / 2.0),
            ),
            vector_scale(left_edge_normalized, height / 2.0),
        ),
        origin,
    );

    let unrotated_top_left = point_rotate_rads(
        point_from_vector(rotated_top_left, origin),
        center,
        Radians(-angle.0),
    );

    let mut uncropped = element.clone();
    uncropped.base.x = unrotated_top_left.x;
    uncropped.base.y = unrotated_top_left.y;
    uncropped.base.width = width;
    uncropped.base.height = height;
    if let ElementKind::Image(fields) = &mut uncropped.kind {
        fields.crop = None;
    }
    uncropped
}

/// `getUncroppedWidthAndHeight(element)` (`cropElement.ts:549-566`): the
/// `(width, height)` the whole image has at the element's scale; the
/// element's own size when it is not cropped.
pub fn get_uncropped_width_and_height(element: &Element) -> (f64, f64) {
    let b = &element.base;
    match image_crop_and_scale(element).0 {
        Some(crop) => (
            b.width / (crop.width / crop.natural_width),
            b.height / (crop.height / crop.natural_height),
        ),
        None => (b.width, b.height),
    }
}

/// `adjustCropPosition(crop, scale)` (`cropElement.ts:568-590`): the
/// crop's position measured from the flipped image's top left.
fn adjust_crop_position(crop: &ImageCrop, scale: [f64; 2]) -> [f64; 2] {
    let mut crop_x = crop.x;
    let mut crop_y = crop.y;

    let flip_x = scale[0] == -1.0;
    let flip_y = scale[1] == -1.0;

    if flip_x {
        crop_x = crop.natural_width - crop_x.abs() - crop.width;
    }

    if flip_y {
        crop_y = crop.natural_height - crop_y.abs() - crop.height;
    }

    [crop_x, crop_y]
}
