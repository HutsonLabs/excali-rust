//! Cropping an image on the canvas (`App.startImageCropping`,
//! `finishImageCropping`, `maybeHandleCrop` and the crop region's drag,
//! `packages/excalidraw/components/App.tsx`).
//!
//! A double-click on the one selected image, or Enter with it selected,
//! starts cropping it (`croppingElementId`); Enter, Escape or a press off
//! the image ends it. While cropping, the image's handles crop it
//! (`cropElement`, [`excali_editor::crop`]) instead of resizing it, snapped
//! like a resize; a drag inside a cropped image moves the crop over the
//! image.
//!
//! Upstream crops only an image whose bitmap has loaded, reading its
//! natural size; the element reads the size from the file's data URL
//! header (PNG, GIF, JPEG, WebP), or from what the host set with
//! [`Editor::set_image_size`].

use excali_core::element::{Element, ElementKind};
use excali_core::encode::{atob, byte_string_to_bytes};
use excali_editor::binding::update_bound_elements;
use excali_editor::crop::{crop_element, get_uncropped_width_and_height};
use excali_editor::scene::Scene;
use excali_editor::snapping::{snap_resizing_elements, SnapEvent};
use excali_editor::transform::get_grid_point;
use excali_editor::transform_handles::TransformHandleType;
use excali_math::{point_from, point_rotate_rads, GlobalPoint, Radians};
use excali_scene::bounds::{get_element_absolute_coords, ElementsMap};
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{json, Value};

use crate::editor::{Editor, PointerInput};
use crate::interact::snap_lines_json;

/// A crop handle's press (`pointerDownState.resize` while cropping).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CropPress {
    pub(crate) id: String,
    pub(crate) handle: TransformHandleType,
    /// `resize.offset`.
    pub(crate) offset: [f64; 2],
    /// `originInGrid`.
    pub(crate) origin_in_grid: [f64; 2],
    /// The image at the press (`originalElements`).
    pub(crate) original: Element,
}

fn be_u32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn le_u16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be_u16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

/// The pixel size an image's header gives (`naturalWidth`,
/// `naturalHeight`): PNG, GIF, JPEG (its first SOF marker) and WebP.
pub(crate) fn image_natural_size(bytes: &[u8]) -> Option<(f64, f64)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((f64::from(be_u32(bytes, 16)?), f64::from(be_u32(bytes, 20)?)));
    }
    if bytes.starts_with(b"GIF8") {
        return Some((f64::from(le_u16(bytes, 6)?), f64::from(le_u16(bytes, 8)?)));
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        let mut at = 2;
        while at + 9 < bytes.len() {
            if bytes[at] != 0xff {
                return None;
            }
            let marker = bytes[at + 1];
            let length = usize::from(be_u16(bytes, at + 2)?);
            let sof = matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
            if sof {
                let height = be_u16(bytes, at + 5)?;
                let width = be_u16(bytes, at + 7)?;
                return Some((f64::from(width), f64::from(height)));
            }
            at += 2 + length;
        }
        return None;
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return match bytes.get(12..16)? {
            b"VP8 " => Some((
                f64::from(le_u16(bytes, 26)? & 0x3fff),
                f64::from(le_u16(bytes, 28)? & 0x3fff),
            )),
            b"VP8L" => {
                let b = bytes.get(21..25)?;
                let bits = u32::from_le_bytes(b.try_into().ok()?);
                Some((
                    f64::from((bits & 0x3fff) + 1),
                    f64::from(((bits >> 14) & 0x3fff) + 1),
                ))
            }
            b"VP8X" => {
                let w = u32::from_le_bytes([*bytes.get(24)?, *bytes.get(25)?, *bytes.get(26)?, 0]);
                let h = u32::from_le_bytes([*bytes.get(27)?, *bytes.get(28)?, *bytes.get(29)?, 0]);
                Some((f64::from(w + 1), f64::from(h + 1)))
            }
            _ => None,
        };
    }
    None
}

/// The bytes of a `data:` URL whose body is base64.
fn data_url_bytes(url: &str) -> Option<Vec<u8>> {
    let (head, body) = url.strip_prefix("data:")?.split_once(',')?;
    if !head.ends_with(";base64") {
        return None;
    }
    // the header is all a size needs
    let prefix: String = body.chars().take(4096).collect();
    let prefix = &prefix[..prefix.len() - prefix.len() % 4];
    let decoded = atob(prefix).ok()?;
    Some(byte_string_to_bytes(&decoded))
}

fn rotate(p: [f64; 2], c: [f64; 2], angle: f64) -> [f64; 2] {
    let r: GlobalPoint = point_rotate_rads(point_from(p[0], p[1]), point_from(c[0], c[1]), Radians(angle));
    [r.x, r.y]
}

fn normalize(v: [f64; 2]) -> [f64; 2] {
    let m = excali_math::js::hypot(v[0], v[1]);
    if m == 0.0 {
        [0.0, 0.0]
    } else {
        [v[0] / m, v[1] / m]
    }
}

impl<P: TextMetricsProvider + Clone> Editor<P> {
    /// The natural size of image `file_id`, as the host measured it: the
    /// size the element uses to crop that image.
    pub fn set_image_size(&mut self, file_id: &str, width: f64, height: f64) {
        self.image_sizes.insert(file_id.to_owned(), (width, height));
    }

    /// The natural size of the image element's file: the host's, else the
    /// data URL's header.
    pub(crate) fn natural_size(&self, element: &Element) -> Option<(f64, f64)> {
        let ElementKind::Image(image) = &element.kind else {
            return None;
        };
        let file_id = image.file_id.as_ref()?.0.clone();
        if let Some(size) = self.image_sizes.get(&file_id) {
            return Some(*size);
        }
        let url = self
            .file
            .files
            .get(&file_id)?
            .get("dataURL")?
            .as_str()?;
        image_natural_size(&data_url_bytes(url)?)
    }

    pub(crate) fn cropping_id(&self) -> Option<String> {
        self.session
            .app_state()
            .get("croppingElementId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    }

    /// `startImageCropping(image)` (`App.tsx:7300-7305`).
    pub(crate) fn start_image_cropping(&mut self, id: &str) {
        self.session.store.schedule_capture();
        self.set_keys(vec![("croppingElementId", json!(id))]);
        self.session.commit();
        self.report();
    }

    /// `finishImageCropping()` (`App.tsx:7307-7314`).
    pub(crate) fn finish_image_cropping(&mut self) {
        if self.cropping_id().is_some() {
            self.session.store.schedule_capture();
            self.set_keys(vec![("croppingElementId", Value::Null)]);
            self.session.commit();
            self.report();
        }
    }

    /// The one selected element, when it is an image.
    pub(crate) fn selected_image(&self) -> Option<String> {
        let selected = self.selected_ids();
        let [id] = selected.as_slice() else {
            return None;
        };
        self.session
            .elements()
            .iter()
            .any(|e| &e.base.id == id && !e.base.is_deleted && matches!(e.kind, ElementKind::Image(_)))
            .then(|| id.clone())
    }

    /// `maybeHandleCrop` (`App.tsx:13590-13680`): the image cropped to the
    /// pointer less the press's offset (on the grid), snapped like a
    /// resize; Shift keeps the aspect ratio of the image at the press.
    pub(crate) fn crop_move(&mut self, press: &CropPress, input: PointerInput) {
        let pointer = self.scene_point(input.client_x, input.client_y);
        let grid = self.grid_size(input.ctrl_or_cmd);
        let Some(element) = self
            .session
            .elements()
            .iter()
            .find(|e| e.base.id == press.id && !e.base.is_deleted)
            .cloned()
        else {
            return;
        };
        let Some((natural_width, natural_height)) = self.natural_size(&element) else {
            return;
        };
        let [x, y] = get_grid_point(pointer[0] - press.offset[0], pointer[1] - press.offset[1], grid);
        let [gx, gy] = get_grid_point(pointer[0], pointer[1], grid);
        let drag_offset = [gx - press.origin_in_grid[0], gy - press.origin_in_grid[1]];
        let snap_state = self.snap_state();
        let event = Some(SnapEvent {
            ctrl_or_cmd: input.ctrl_or_cmd,
        });
        let elements = self.session.elements().to_vec();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        self.snap_cache
            .maybe_cache_reference_snap_points(&snap_state, event, &[&element], &live, &map);
        let snapped = snap_resizing_elements(
            &[&element],
            &[&press.original],
            &self.snap_cache,
            &snap_state,
            event,
            drag_offset,
            Some(press.handle),
        );
        let ratio = input
            .shift_key
            .then(|| press.original.base.width / press.original.base.height);
        let update = crop_element(
            &element,
            &map,
            press.handle,
            natural_width,
            natural_height,
            x + snapped.snap_offset[0],
            y + snapped.snap_offset[1],
            ratio,
        );
        drop(map);
        let mut cropped = element.clone();
        update.apply_to(&mut cropped);
        excali_editor::mutate::bump_version(&mut cropped, None, &mut self.session.env);
        let mut scene = Scene::new(elements);
        scene.replace_element(cropped);
        update_bound_elements(&mut scene, &mut self.session.env, &press.id, None, None);
        let app_state = self.session.app_state().clone();
        self.apply(scene, app_state);
        self.set_keys(vec![
            ("isCropping", json!(press.handle != TransformHandleType::Rotation)),
            ("snapLines", snap_lines_json(&snapped.snap_lines)),
        ]);
        self.session.commit();
        self.report();
    }

    /// A drag inside the image being cropped (`App.tsx:11095-11184`): the
    /// crop moves the other way over the image, by the pointer's movement
    /// since the last move in the image's pixels, clamped to the image.
    /// Returns whether the drag was the crop's.
    pub(crate) fn crop_region_drag(&mut self, id: &str, last: [f64; 2], pointer: [f64; 2]) -> bool {
        let Some(element) = self
            .session
            .elements()
            .iter()
            .find(|e| e.base.id == id && !e.base.is_deleted)
            .cloned()
        else {
            return false;
        };
        let ElementKind::Image(image) = &element.kind else {
            return false;
        };
        let Some(crop) = image.crop else {
            return false;
        };
        let Some((natural_width, natural_height)) = self.natural_size(&element) else {
            return false;
        };
        let (uncropped_width, uncropped_height) = get_uncropped_width_and_height(&element);
        let instant = [
            (pointer[0] - last[0]) * natural_width / uncropped_width,
            (pointer[1] - last[1]) * natural_height / uncropped_height,
        ];
        let elements = self.session.elements().to_vec();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        let [x1, y1, x2, y2, cx, cy] = get_element_absolute_coords(&element, &map, false);
        drop(map);
        let angle = element.base.angle.0;
        let top_left = rotate([x1, y1], [cx, cy], angle);
        let top_right = rotate([x2, y1], [cx, cy], angle);
        let bottom_left = rotate([x1, y2], [cx, cy], angle);
        let top_edge = normalize([top_right[0] - top_left[0], top_right[1] - top_left[1]]);
        let left_edge = normalize([bottom_left[0] - top_left[0], bottom_left[1] - top_left[1]]);
        let offset = [
            instant[0] * top_edge[0] + instant[1] * top_edge[1],
            instant[0] * left_edge[0] + instant[1] * left_edge[1],
        ];
        let clamp = |v: f64, min: f64, max: f64| v.max(min).min(max);
        // Math.sign
        let sign = |v: f64| {
            if v > 0.0 {
                1.0
            } else if v < 0.0 {
                -1.0
            } else {
                v
            }
        };
        let mut next = crop;
        next.x = clamp(
            next.x - offset[0] * sign(image.scale[0]),
            0.0,
            natural_width - next.width,
        );
        next.y = clamp(
            next.y - offset[1] * sign(image.scale[1]),
            0.0,
            natural_height - next.height,
        );
        let mut moved = element.clone();
        if let ElementKind::Image(img) = &mut moved.kind {
            img.crop = Some(next);
        }
        excali_editor::mutate::bump_version(&mut moved, None, &mut self.session.env);
        let mut scene = Scene::new(elements);
        scene.replace_element(moved);
        let app_state = self.session.app_state().clone();
        self.apply(scene, app_state);
        self.session.commit();
        self.report();
        true
    }
}
