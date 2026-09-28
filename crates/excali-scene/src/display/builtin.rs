//! Upstream's own images: the SVG documents it loads from `data:` URLs and
//! draws with `drawImage`, which every backend has without being given
//! them. A display list names one by its `id` in an [`super::ImageItem`];
//! a backend resolves it with [`builtin_image_by_id`] (and draws nothing
//! for an id that is neither one of these nor a file it holds).

/// One of upstream's own images: the image placeholders
/// (`renderElement.ts:343-359`) and the link icons
/// (`components/hyperlink/helpers.ts:19-27`). The display list names it by
/// `id`; a backend loads it from `data_url`, the exact `src` upstream gives
/// its `<img>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltinImage {
    pub id: &'static str,
    pub svg: &'static str,
    pub data_url: String,
}

/// `IMAGE_PLACEHOLDER_IMG`, drawn while an image is not loaded.
pub const IMAGE_PLACEHOLDER_ID: &str = "excalidraw:image-placeholder";
/// `IMAGE_ERROR_PLACEHOLDER_IMG`, drawn for an image whose status is
/// `error`.
pub const IMAGE_ERROR_PLACEHOLDER_ID: &str = "excalidraw:image-error-placeholder";
/// `EXTERNAL_LINK_IMG`, the icon of an element with a link.
pub const EXTERNAL_LINK_ID: &str = "excalidraw:external-link";
/// `ELEMENT_LINK_IMG`, the icon of an element linking to an element.
pub const ELEMENT_LINK_ID: &str = "excalidraw:element-link";

const IMAGE_PLACEHOLDER_SVG: &str = r##"<svg aria-hidden="true" focusable="false" data-prefix="fas" data-icon="image" class="svg-inline--fa fa-image fa-w-16" role="img" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><path fill="#888" d="M464 448H48c-26.51 0-48-21.49-48-48V112c0-26.51 21.49-48 48-48h416c26.51 0 48 21.49 48 48v288c0 26.51-21.49 48-48 48zM112 120c-30.928 0-56 25.072-56 56s25.072 56 56 56 56-25.072 56-56-25.072-56-56-56zM64 384h384V272l-87.515-87.515c-4.686-4.686-12.284-4.686-16.971 0L208 320l-55.515-55.515c-4.686-4.686-12.284-4.686-16.971 0L64 336v48z"></path></svg>"##;

const IMAGE_ERROR_PLACEHOLDER_SVG: &str = r##"<svg viewBox="0 0 668 668" xmlns="http://www.w3.org/2000/svg" xml:space="preserve" style="fill-rule:evenodd;clip-rule:evenodd;stroke-linejoin:round;stroke-miterlimit:2"><path d="M464 448H48c-26.51 0-48-21.49-48-48V112c0-26.51 21.49-48 48-48h416c26.51 0 48 21.49 48 48v288c0 26.51-21.49 48-48 48ZM112 120c-30.928 0-56 25.072-56 56s25.072 56 56 56 56-25.072 56-56-25.072-56-56-56ZM64 384h384V272l-87.515-87.515c-4.686-4.686-12.284-4.686-16.971 0L208 320l-55.515-55.515c-4.686-4.686-12.284-4.686-16.971 0L64 336v48Z" style="fill:#888;fill-rule:nonzero" transform="matrix(.81709 0 0 .81709 124.825 145.825)"/><path d="M256 8C119.034 8 8 119.033 8 256c0 136.967 111.034 248 248 248s248-111.034 248-248S392.967 8 256 8Zm130.108 117.892c65.448 65.448 70 165.481 20.677 235.637L150.47 105.216c70.204-49.356 170.226-44.735 235.638 20.676ZM125.892 386.108c-65.448-65.448-70-165.481-20.677-235.637L361.53 406.784c-70.203 49.356-170.226 44.736-235.638-20.676Z" style="fill:#888;fill-rule:nonzero" transform="matrix(.30366 0 0 .30366 506.822 60.065)"/></svg>"##;

const EXTERNAL_LINK_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#1971c2" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" class="feather feather-external-link"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path><polyline points="15 3 21 3 21 9"></polyline><line x1="10" y1="14" x2="21" y2="3"></line></svg>"##;

const ELEMENT_LINK_SVG: &str = r##"<svg  xmlns="http://www.w3.org/2000/svg"  width="16"  height="16"  viewBox="0 0 24 24"  fill="none"  stroke="#1971c2"  stroke-width="2"  stroke-linecap="round"  stroke-linejoin="round"  class="icon icon-tabler icons-tabler-outline icon-tabler-arrow-big-right-line"><path stroke="none" d="M0 0h24v24H0z" fill="none"/><path d="M12 9v-3.586a1 1 0 0 1 1.707 -.707l6.586 6.586a1 1 0 0 1 0 1.414l-6.586 6.586a1 1 0 0 1 -1.707 -.707v-3.586h-6v-6h6z" /><path d="M3 9v6" /></svg>"##;

/// `encodeURIComponent(s)`: every UTF-8 byte of `s` percent-encoded except
/// ASCII letters, digits and `-_.!~*'()`.
pub fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The names [`builtin_image`] knows, in the order upstream defines them.
/// Each image's id is `excalidraw:` and its name; file ids (SHA-1 hex
/// digests or nanoids) never contain a colon.
pub const BUILTIN_IMAGE_NAMES: [&str; 4] = [
    "image-placeholder",
    "image-error-placeholder",
    "external-link",
    "element-link",
];

/// The built-in image upstream's code calls `name`: `image-placeholder`,
/// `image-error-placeholder`, `external-link` or `element-link`.
pub fn builtin_image(name: &str) -> Option<BuiltinImage> {
    // `data:${MIME_TYPES.svg},${…}`; the link icons have a space after the
    // comma
    let (id, svg, separator) = match name {
        "image-placeholder" => (IMAGE_PLACEHOLDER_ID, IMAGE_PLACEHOLDER_SVG, ","),
        "image-error-placeholder" => (IMAGE_ERROR_PLACEHOLDER_ID, IMAGE_ERROR_PLACEHOLDER_SVG, ","),
        "external-link" => (EXTERNAL_LINK_ID, EXTERNAL_LINK_SVG, ", "),
        "element-link" => (ELEMENT_LINK_ID, ELEMENT_LINK_SVG, ", "),
        _ => return None,
    };
    Some(BuiltinImage {
        id,
        svg,
        data_url: format!("data:image/svg+xml{separator}{}", encode_uri_component(svg)),
    })
}

/// The built-in image a display list names by `id` (an [`ImageItem`]'s
/// id), if `id` is one: how a backend resolves the images every backend
/// has without being given them.
pub fn builtin_image_by_id(id: &str) -> Option<BuiltinImage> {
    let name = id.strip_prefix("excalidraw:")?;
    builtin_image(name).filter(|image| image.id == id)
}
