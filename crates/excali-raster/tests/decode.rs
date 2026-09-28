//! Image files from data URLs (ex-404): what the browser does with
//! `img.src = fileData.dataURL` in `loadHTMLImageElement`
//! (`packages/element/src/image.ts:21-32`), for the files `updateImageCache`
//! puts in the export's image cache (`:36-87`).
//!
//! - The URL is read as the fetch standard's `data:` URL processor reads it:
//!   the MIME type before the comma (case-insensitive, parameters after
//!   `;`, `text/plain` when empty), `;base64` bodies through forgiving
//!   base64 (ASCII whitespace dropped, padding optional), others
//!   percent-decoded.
//! - Raster formats are recognised from their bytes, as Chrome's image
//!   decoders sniff them whatever the declared type: PNG, JPEG (with its
//!   Exif orientation applied), GIF (first frame), WebP, BMP and ICO. SVG is
//!   read only when the type says `image/svg+xml`.
//! - A file whose `mimeType` is `application/octet-stream` never loads
//!   (`image.ts:57-59`), and neither does one whose bytes decode to nothing.
//!
//! The input files are `tests/fixtures/images/` (written by
//! `scripts/fixtures/raster-images.py`).

use std::path::PathBuf;

use base64::Engine;
use excali_raster::decode::{decode_data_url, parse_data_url, DecodeError, DecodedImage, ImageFiles};
use excali_raster::tiny_skia::Pixmap;
use excali_raster::{Image, ImageStore};

fn file(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/images")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn data_url(mime: &str, name: &str) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(file(name))
    )
}

fn bitmap(image: DecodedImage) -> Pixmap {
    match image {
        DecodedImage::Bitmap(p) => p,
        DecodedImage::Svg(_) => panic!("a bitmap"),
    }
}

/// `scripts/fixtures/raster-images.py`'s pixel formula, straight alpha.
fn source_pixel(x: u32, y: u32, alpha: bool) -> [u8; 4] {
    let (w, h) = (24.0, 16.0);
    let ramp = (f64::from(x) * 255.0 / (w - 1.0)).round() as u8;
    let rgb = if y < 8 {
        if x < 12 {
            [230, ramp, 40]
        } else {
            [ramp, 60, 220]
        }
    } else if x < 12 {
        [40, 200, ramp]
    } else {
        [250, 250 - ramp / 2, ramp / 3]
    };
    let a = if alpha {
        (255.0 - f64::from(y) * 200.0 / (h - 1.0)).round() as u8
    } else {
        255
    };
    [rgb[0], rgb[1], rgb[2], a]
}

/// The pixel as straight RGBA.
fn straight(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
    let c = p.pixel(x, y).unwrap().demultiply();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

fn max_difference(a: &Pixmap, b: &Pixmap) -> u8 {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    a.data()
        .iter()
        .zip(b.data())
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap()
}

// -- data URLs --------------------------------------------------------------

#[test]
fn base64_data_urls() {
    let url = parse_data_url("data:image/png;base64,aGVsbG8=").unwrap();
    assert_eq!(url.mime_type, "image/png");
    assert_eq!(url.body, b"hello");
    // Forgiving base64: ASCII whitespace anywhere, padding optional.
    let url = parse_data_url("data:image/png;base64, aGVs\nbG8").unwrap();
    assert_eq!(url.body, b"hello");
    // The scheme, type and `base64` are case-insensitive; the type is
    // lower-cased, parameters dropped.
    let url = parse_data_url("DATA:Image/PNG;Charset=x;BASE64,aGVsbG8=").unwrap();
    assert_eq!(url.mime_type, "image/png");
    assert_eq!(url.body, b"hello");
    // Broken base64 is a network error: nothing loads.
    for bad in [
        "data:image/png;base64,a",
        "data:image/png;base64,aGVsbG8==x",
        "data:image/png;base64,a*b=",
    ] {
        assert!(
            matches!(parse_data_url(bad), Err(DecodeError::DataUrl(_))),
            "{bad}"
        );
    }
}

#[test]
fn percent_encoded_data_urls() {
    // Upstream's placeholders: `data:${MIME_TYPES.svg},${encodeURIComponent(svg)}`.
    let url = parse_data_url("data:image/svg+xml,%3Csvg%20a%3D%22b%22%2F%3E").unwrap();
    assert_eq!(url.mime_type, "image/svg+xml");
    assert_eq!(url.body, br#"<svg a="b"/>"#);
    // A stray % stays as it is.
    assert_eq!(parse_data_url("data:,50%").unwrap().body, b"50%");
    assert_eq!(parse_data_url("data:,%zz%4").unwrap().body, b"%zz%4");
    // No type: text/plain.
    assert_eq!(parse_data_url("data:,x").unwrap().mime_type, "text/plain");
    assert_eq!(parse_data_url("data:;charset=utf-8,x").unwrap().mime_type, "text/plain");
}

#[test]
fn not_data_urls() {
    for bad in ["", "data:", "data:image/png;base64", "http://x/y.png", "blob:x"] {
        assert!(
            matches!(parse_data_url(bad), Err(DecodeError::DataUrl(_))),
            "{bad:?}"
        );
    }
}

// -- raster formats -----------------------------------------------------------

#[test]
fn png_decodes_to_its_pixels() {
    let p = bitmap(decode_data_url(&data_url("image/png", "quad.png")).unwrap());
    assert_eq!((p.width(), p.height()), (24, 16));
    for y in 0..16 {
        for x in 0..24 {
            let expected = source_pixel(x, y, true);
            let actual = straight(&p, x, y);
            // Premultiplying loses precision at low alpha; unpremultiplied
            // back it is within a level or two.
            for c in 0..4 {
                assert!(
                    actual[c].abs_diff(expected[c]) <= 2,
                    "({x}, {y}): {actual:?} != {expected:?}"
                );
            }
        }
    }
    // Stored premultiplied, as the canvas keeps bitmaps.
    let c = p.pixel(0, 15).unwrap();
    assert_eq!(c.alpha(), 55);
    assert_eq!(c.red(), ((230.0 * 55.0) / 255.0_f64).round() as u8);
}

#[test]
fn lossless_formats_agree_with_the_png() {
    let png = bitmap(decode_data_url(&data_url("image/png", "opaque.png")).unwrap());
    for y in 0..16 {
        for x in 0..24 {
            assert_eq!(straight(&png, x, y), source_pixel(x, y, false));
        }
    }
    for (mime, name) in [
        ("image/bmp", "opaque.bmp"),
        ("image/gif", "opaque.gif"),
        ("image/webp", "lossless.webp"),
    ] {
        let p = bitmap(decode_data_url(&data_url(mime, name)).unwrap());
        assert_eq!(max_difference(&p, &png), 0, "{name}");
    }
}

#[test]
fn lossy_formats_come_close_to_the_png() {
    let png = bitmap(decode_data_url(&data_url("image/png", "opaque.png")).unwrap());
    for (mime, name, bound) in [
        ("image/jpeg", "opaque.jpg", 64),
        ("image/webp", "lossy.webp", 64),
    ] {
        let p = bitmap(decode_data_url(&data_url(mime, name)).unwrap());
        let d = max_difference(&p, &png);
        assert!(d <= bound, "{name}: {d}");
        assert!(p.pixels().iter().all(|c| c.alpha() == 255), "{name} is opaque");
    }
}

#[test]
fn the_declared_type_does_not_pick_the_raster_decoder() {
    // Chrome sniffs raster formats from their signatures; image/jpeg on a
    // PNG, or image/jfif, still decodes.
    let png = bitmap(decode_data_url(&data_url("image/png", "quad.png")).unwrap());
    for mime in ["image/jpeg", "image/jfif", "application/octet-stream", "text/plain"] {
        let p = bitmap(decode_data_url(&data_url(mime, "quad.png")).unwrap());
        assert_eq!(p.data(), png.data(), "{mime}");
    }
}

#[test]
fn jpeg_exif_orientation_is_applied() {
    // image-orientation: from-image (the default since Chrome 81, also for
    // drawImage): Orientation 6 shows the stored pixels rotated 90 degrees
    // clockwise.
    let stored = bitmap(decode_data_url(&data_url("image/jpeg", "opaque.jpg")).unwrap());
    let shown = bitmap(decode_data_url(&data_url("image/jpeg", "oriented.jpg")).unwrap());
    assert_eq!((stored.width(), stored.height()), (24, 16));
    assert_eq!((shown.width(), shown.height()), (16, 24));
    // Rotating clockwise: shown(x, y) = stored(y, H - 1 - x).
    let mut worst = 0;
    for y in 0..24 {
        for x in 0..16 {
            let a = straight(&shown, x, y);
            let b = straight(&stored, y, 15 - x);
            for c in 0..3 {
                worst = worst.max(a[c].abs_diff(b[c]));
            }
        }
    }
    // The two files share their scan data, so the pixels are identical.
    assert_eq!(worst, 0);
}

#[test]
fn undecodable_bytes_are_errors() {
    assert!(matches!(
        decode_data_url("data:image/png;base64,aGVsbG8="),
        Err(DecodeError::Image(_))
    ));
    // SVG is only read when the type says so (Chrome does not sniff it).
    assert!(matches!(
        decode_data_url(&data_url("image/png", "shape.svg")),
        Err(DecodeError::Image(_))
    ));
    // A truncated PNG.
    let mut bytes = file("quad.png");
    bytes.truncate(60);
    let url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    assert!(decode_data_url(&url).is_err());
}

// -- SVG --------------------------------------------------------------------

#[test]
fn svg_is_kept_as_a_vector() {
    let DecodedImage::Svg(tree) = decode_data_url(&data_url("image/svg+xml", "shape.svg")).unwrap()
    else {
        panic!("an SVG")
    };
    assert_eq!((tree.size().width(), tree.size().height()), (40.0, 30.0));
    // Percent-encoded with parameters, as upstream writes its placeholders.
    let svg = String::from_utf8(file("shape.svg")).unwrap();
    let encoded: String = svg
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    let url = format!("data:image/svg+xml;charset=utf-8,{encoded}");
    assert!(matches!(decode_data_url(&url).unwrap(), DecodedImage::Svg(_)));
    assert!(matches!(
        decode_data_url("data:image/svg+xml,%3Cnot-svg"),
        Err(DecodeError::Image(_))
    ));
}

// -- the store ----------------------------------------------------------------

#[test]
fn image_files_hold_what_loaded() {
    let png = data_url("image/png", "quad.png");
    let svg = data_url("image/svg+xml", "shape.svg");
    let files = ImageFiles::decode([
        ("png", "image/png", png.as_str()),
        ("svg", "image/svg+xml", svg.as_str()),
        ("broken", "image/png", "data:image/png;base64,aGVsbG8="),
        // updateImageCache: "Only images can be added to ImageCache".
        ("binary", "application/octet-stream", png.as_str()),
    ]);
    assert!(matches!(files.image("png"), Some(Image::Bitmap(p)) if p.width() == 24));
    assert!(matches!(files.image("svg"), Some(Image::Svg(_))));
    assert!(files.image("broken").is_none());
    assert!(files.image("binary").is_none());
    assert!(files.image("absent").is_none());
    assert_eq!(files.mime_type("svg"), Some("image/svg+xml"));
    assert!(matches!(files.error("broken"), Some(DecodeError::Image(_))));
    assert!(matches!(files.error("binary"), Some(DecodeError::NotAnImage)));
    assert!(files.error("png").is_none());
    let mut loaded: Vec<&str> = files.loaded().collect();
    loaded.sort_unstable();
    assert_eq!(loaded, ["png", "svg"]);
}
