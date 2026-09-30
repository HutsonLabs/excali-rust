//! Cropping an image (`packages/element/src/cropElement.ts`).
//!
//! [`crop_element`] is what the editor calls on every pointer move while an
//! image's crop handle is dragged (`App.tsx:13626-13676`): the pointer,
//! turned back by the image's angle, moves the grabbed side(s) of the crop
//! within the uncropped image, never below [`MINIMAL_CROP_SIZE`]. A flipped
//! image (`scale` of -1) keeps its crop in the unflipped image's
//! coordinates, so the handles move the opposite side of the crop. With an
//! aspect ratio (Shift) the other dimension follows, limited by the room
//! left in the image. The result is the new box and crop; a crop back at the
//! image's natural size is `None`.
//!
//! [`get_uncropped_image_element`] is the whole image a cropped element
//! shows part of, [`get_uncropped_width_and_height`] its size and
//! [`get_flip_adjusted_crop_position`] where the crop starts on it as drawn.
//!
//! Upstream mutates an existing `element.crop` in place and returns it; here
//! the element is untouched and the crop is returned.

use excali_core::element::{Element, ElementKind, ImageCrop};
use excali_math::{
    clamp, is_close_to, point_center, point_from, point_from_vector, point_rotate_rads, vector_add,
    vector_from_point, vector_normalize, vector_scale, vector_subtract, GlobalPoint, Point,
    Radians,
};
use excali_scene::bounds::{element_center_point, get_element_absolute_coords, ElementsMap};

use crate::resize_elements::get_resized_element_absolute_coords;
use crate::transform_handles::TransformHandleType;

/// `MINIMAL_CROP_SIZE` (`cropElement.ts:31`): the smallest crop width and
/// height, in scene units.
pub const MINIMAL_CROP_SIZE: f64 = 10.0;

/// What [`crop_element`] returns (`cropElement.ts:398-404`): the element's
/// next position, size and crop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropUpdate {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub crop: Option<ImageCrop>,
}

impl CropUpdate {
    /// Assigns the update to `element` (`mutateElement(element, update)`);
    /// the crop only to an image.
    pub fn apply_to(&self, element: &mut Element) {
        element.base.x = self.x;
        element.base.y = self.y;
        element.base.width = self.width;
        element.base.height = self.height;
        if let ElementKind::Image(fields) = &mut element.kind {
            fields.crop = self.crop;
        }
    }
}

/// The image's crop and scale; an element that is not an image has
/// neither (scale `[1, 1]`).
fn image_crop_and_scale(element: &Element) -> (Option<ImageCrop>, [f64; 2]) {
    match &element.kind {
        ElementKind::Image(fields) => (fields.crop, fields.scale),
        _ => (None, [1.0, 1.0]),
    }
}

/// `transformHandle.includes(c)`: on the handle's name, so `"rotation"`
/// includes `n`, as upstream's string does.
fn includes(handle: TransformHandleType, c: char) -> bool {
    handle.as_str().contains(c)
}

/// `!!value` for an optional number: `None`, zero and NaN are falsy.
fn truthy(value: Option<f64>) -> Option<f64> {
    value.filter(|v| *v != 0.0 && !v.is_nan())
}

/// `cropElement(element, elementsMap, transformHandle, naturalWidth,
/// naturalHeight, pointerX, pointerY, widthAspectRatio)`
/// (`cropElement.ts:33-405`): the image's box and crop with the handle
/// dragged to the pointer. `natural_width` and `natural_height` are the
/// image file's size; `width_aspect_ratio` keeps the crop's width to height
/// ratio (upstream passes the ratio at the start of the crop while Shift is
/// held).
#[allow(clippy::too_many_arguments)]
pub fn crop_element(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    transform_handle: TransformHandleType,
    natural_width: f64,
    natural_height: f64,
    pointer_x: f64,
    pointer_y: f64,
    width_aspect_ratio: Option<f64>,
) -> CropUpdate {
    let b = &element.base;
    let (element_crop, scale) = image_crop_and_scale(element);
    let width_aspect_ratio = truthy(width_aspect_ratio);

    let (uncropped_width, uncropped_height) = get_uncropped_width_and_height(element);

    let natural_width_to_uncropped = natural_width / uncropped_width;
    let natural_height_to_uncropped = natural_height / uncropped_height;

    let cropped_left = element_crop.map_or(0.0, |c| c.x) / natural_width_to_uncropped;
    let cropped_top = element_crop.map_or(0.0, |c| c.y) / natural_height_to_uncropped;

    let [center_x, center_y] = element_center_point(element, elements_map);
    let rotated_pointer: GlobalPoint = point_rotate_rads(
        point_from(pointer_x, pointer_y),
        point_from(center_x, center_y),
        Radians(-b.angle.0),
    );

    let pointer_x = rotated_pointer.x;
    let pointer_y = rotated_pointer.y;

    let mut next_width = b.width;
    let mut next_height = b.height;

    let mut crop = element_crop.unwrap_or(ImageCrop {
        x: 0.0,
        y: 0.0,
        width: natural_width,
        height: natural_height,
        natural_width,
        natural_height,
    });

    let previous_crop_height = crop.height;
    let previous_crop_width = crop.width;

    let is_flipped_by_x = scale[0] == -1.0;
    let is_flipped_by_y = scale[1] == -1.0;

    let mut change_in_height = pointer_y - b.y;
    let mut change_in_width = pointer_x - b.x;

    if includes(transform_handle, 'n') {
        next_height = clamp(
            b.height - change_in_height,
            MINIMAL_CROP_SIZE,
            if is_flipped_by_y {
                uncropped_height - cropped_top
            } else {
                b.height + cropped_top
            },
        );
    }

    if includes(transform_handle, 's') {
        change_in_height = pointer_y - b.y - b.height;
        next_height = clamp(
            b.height + change_in_height,
            MINIMAL_CROP_SIZE,
            if is_flipped_by_y {
                b.height + cropped_top
            } else {
                uncropped_height - cropped_top
            },
        );
    }

    if includes(transform_handle, 'e') {
        change_in_width = pointer_x - b.x - b.width;

        next_width = clamp(
            b.width + change_in_width,
            MINIMAL_CROP_SIZE,
            if is_flipped_by_x {
                b.width + cropped_left
            } else {
                uncropped_width - cropped_left
            },
        );
    }

    if includes(transform_handle, 'w') {
        next_width = clamp(
            b.width - change_in_width,
            MINIMAL_CROP_SIZE,
            if is_flipped_by_x {
                uncropped_width - cropped_left
            } else {
                b.width + cropped_left
            },
        );
    }

    let update_crop_width_and_height = |crop: &mut ImageCrop, next_width: f64, next_height: f64| {
        crop.height = next_height * natural_height_to_uncropped;
        crop.width = next_width * natural_width_to_uncropped;
    };

    update_crop_width_and_height(&mut crop, next_width, next_height);

    let adjust_flip_for_handle = |crop: &mut ImageCrop, next_width: f64, next_height: f64| {
        update_crop_width_and_height(crop, next_width, next_height);
        if includes(transform_handle, 'n') && !is_flipped_by_y {
            crop.y += previous_crop_height - crop.height;
        }
        if includes(transform_handle, 's') && is_flipped_by_y {
            crop.y += previous_crop_height - crop.height;
        }
        if includes(transform_handle, 'e') && is_flipped_by_x {
            crop.x += previous_crop_width - crop.width;
        }
        if includes(transform_handle, 'w') && !is_flipped_by_x {
            crop.x += previous_crop_width - crop.width;
        }
    };

    match transform_handle.as_str() {
        "n" | "s" => {
            if let Some(ratio) = width_aspect_ratio {
                let distance_to_left = cropped_left + b.width / 2.0;
                let distance_to_right = uncropped_width - cropped_left - b.width / 2.0;

                let max_width = excali_math::js::min(distance_to_left, distance_to_right) * 2.0;

                next_width = clamp(next_height * ratio, MINIMAL_CROP_SIZE, max_width);
                next_height = next_width / ratio;
            }

            adjust_flip_for_handle(&mut crop, next_width, next_height);

            if width_aspect_ratio.is_some() {
                crop.x += (previous_crop_width - crop.width) / 2.0;
            }
        }
        "w" | "e" => {
            if let Some(ratio) = width_aspect_ratio {
                let distance_to_top = cropped_top + b.height / 2.0;
                let distance_to_bottom = uncropped_height - cropped_top - b.height / 2.0;

                let max_height = excali_math::js::min(distance_to_top, distance_to_bottom) * 2.0;

                next_height = clamp(next_width / ratio, MINIMAL_CROP_SIZE, max_height);
                next_width = next_height * ratio;
            }

            adjust_flip_for_handle(&mut crop, next_width, next_height);

            if width_aspect_ratio.is_some() {
                crop.y += (previous_crop_height - crop.height) / 2.0;
            }
        }
        corner @ ("ne" | "nw" | "se" | "sw") => {
            if let Some(ratio) = width_aspect_ratio {
                // Whether the height follows the width, and the room the
                // image leaves each way (cropElement.ts:259-376).
                let (by_width, max_height, max_width) = match corner {
                    "ne" => (
                        change_in_width > -change_in_height,
                        if is_flipped_by_y {
                            uncropped_height - cropped_top
                        } else {
                            cropped_top + b.height
                        },
                        if is_flipped_by_x {
                            cropped_left + b.width
                        } else {
                            uncropped_width - cropped_left
                        },
                    ),
                    "nw" => (
                        change_in_width < change_in_height,
                        if is_flipped_by_y {
                            uncropped_height - cropped_top
                        } else {
                            cropped_top + b.height
                        },
                        if is_flipped_by_x {
                            uncropped_width - cropped_left
                        } else {
                            cropped_left + b.width
                        },
                    ),
                    "se" => (
                        change_in_width > change_in_height,
                        if is_flipped_by_y {
                            cropped_top + b.height
                        } else {
                            uncropped_height - cropped_top
                        },
                        if is_flipped_by_x {
                            cropped_left + b.width
                        } else {
                            uncropped_width - cropped_left
                        },
                    ),
                    _ => (
                        -change_in_width > change_in_height,
                        if is_flipped_by_y {
                            cropped_top + b.height
                        } else {
                            uncropped_height - cropped_top
                        },
                        if is_flipped_by_x {
                            uncropped_width - cropped_left
                        } else {
                            cropped_left + b.width
                        },
                    ),
                };
                if by_width {
                    next_height = clamp(next_width / ratio, MINIMAL_CROP_SIZE, max_height);
                    next_width = next_height * ratio;
                } else {
                    next_width = clamp(next_height * ratio, MINIMAL_CROP_SIZE, max_width);
                    next_height = next_width / ratio;
                }
            }

            adjust_flip_for_handle(&mut crop, next_width, next_height);
        }
        _ => {}
    }

    let new_origin = recompute_origin(
        element,
        transform_handle,
        next_width,
        next_height,
        width_aspect_ratio.is_some(),
    );

    // reset crop to null if we're back to orig size
    let crop = if is_close_to(crop.width, crop.natural_width)
        && is_close_to(crop.height, crop.natural_height)
    {
        None
    } else {
        Some(crop)
    };

    CropUpdate {
        x: new_origin[0],
        y: new_origin[1],
        width: next_width,
        height: next_height,
        crop,
    }
}

/// `recomputeOrigin(stateAtCropStart, transformHandle, width, height,
/// shouldMaintainAspectRatio)` (`cropElement.ts:407-475`): the element's
/// `x` and `y` at the new size, the side or corner opposite the handle
/// staying where it was on the rotated element.
fn recompute_origin(
    state_at_crop_start: &Element,
    transform_handle: TransformHandleType,
    width: f64,
    height: f64,
    should_maintain_aspect_ratio: bool,
) -> [f64; 2] {
    let s = &state_at_crop_start.base;
    let [x1, y1, x2, y2] =
        get_resized_element_absolute_coords(state_at_crop_start, s.width, s.height, true);
    let start_top_left: GlobalPoint = point_from(x1, y1);
    let start_bottom_right: GlobalPoint = point_from(x2, y2);
    let start_center = point_center(start_top_left, start_bottom_right);

    let [new_bounds_x1, new_bounds_y1, new_bounds_x2, new_bounds_y2] =
        get_resized_element_absolute_coords(state_at_crop_start, width, height, true);
    let new_bounds_width = new_bounds_x2 - new_bounds_x1;
    let new_bounds_height = new_bounds_y2 - new_bounds_y1;

    // Calculate new topLeft based on fixed corner during resize
    let mut new_top_left = [start_top_left.x, start_top_left.y];

    let handle = transform_handle.as_str();
    if matches!(handle, "n" | "w" | "nw") {
        new_top_left = [
            start_bottom_right.x - new_bounds_width.abs(),
            start_bottom_right.y - new_bounds_height.abs(),
        ];
    }
    if handle == "ne" {
        let bottom_left = [start_top_left.x, start_bottom_right.y];
        new_top_left = [bottom_left[0], bottom_left[1] - new_bounds_height.abs()];
    }
    if handle == "sw" {
        let top_right = [start_bottom_right.x, start_top_left.y];
        new_top_left = [top_right[0] - new_bounds_width.abs(), top_right[1]];
    }

    if should_maintain_aspect_ratio {
        if matches!(handle, "s" | "n") {
            new_top_left[0] = start_center.x - new_bounds_width / 2.0;
        }
        if matches!(handle, "e" | "w") {
            new_top_left[1] = start_center.y - new_bounds_height / 2.0;
        }
    }

    // adjust topLeft to new rotation point
    let angle = Radians(s.angle.0);
    let new_top_left_point: GlobalPoint = point_from(new_top_left[0], new_top_left[1]);
    let rotated_top_left = point_rotate_rads(new_top_left_point, start_center, angle);
    let new_center: GlobalPoint = point_from(
        new_top_left[0] + new_bounds_width.abs() / 2.0,
        new_top_left[1] + new_bounds_height.abs() / 2.0,
    );
    let rotated_new_center = point_rotate_rads(new_center, start_center, angle);
    let new_top_left = point_rotate_rads(rotated_top_left, rotated_new_center, Radians(-angle.0));

    [
        new_top_left.x + (s.x - new_bounds_x1),
        new_top_left.y + (s.y - new_bounds_y1),
    ]
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

/// `getFlipAdjustedCropPosition(element, natural)`
/// (`cropElement.ts:592-628`): `[x, y]` of the crop on the image as drawn
/// (flips applied), in the image file's pixels when `natural`, else at the
/// element's scale; `None` when the element is not cropped.
pub fn get_flip_adjusted_crop_position(element: &Element, natural: bool) -> Option<[f64; 2]> {
    let (crop, scale) = image_crop_and_scale(element);
    let crop = crop?;

    let is_flipped_by_x = scale[0] == -1.0;
    let is_flipped_by_y = scale[1] == -1.0;

    let mut crop_x = crop.x;
    let mut crop_y = crop.y;

    if is_flipped_by_x {
        crop_x = crop.natural_width - crop.width - crop.x;
    }

    if is_flipped_by_y {
        crop_y = crop.natural_height - crop.height - crop.y;
    }

    if natural {
        return Some([crop_x, crop_y]);
    }

    let (width, height) = get_uncropped_width_and_height(element);

    Some([
        crop_x / (crop.natural_width / width),
        crop_y / (crop.natural_height / height),
    ])
}
