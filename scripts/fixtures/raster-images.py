#!/usr/bin/env python3
"""Image files for the raster image fixtures (ex-404).

Writes crates/excali-raster/tests/fixtures/images/:

- quad.png: 24x16 RGBA, four coloured quadrants with a horizontal ramp and
  an alpha ramp down the rows (straight alpha, no colour chunks);
- opaque.png: the same without alpha (every pixel alpha 255);
- opaque.bmp: opaque.png as a 24-bit bottom-up BMP;
- opaque.gif, opaque.jpg: opaque.png through macOS `sips` (GIF palette,
  baseline JPEG), with any colour profile removed;
- oriented.jpg: opaque.jpg with its Exif block replaced by one whose
  Orientation is 6 (the stored pixels are shown rotated 90 degrees
  clockwise, as browsers draw them: image-orientation from-image);
- lossless.webp, lossy.webp: opaque.png through `cwebp -lossless` and
  `cwebp -q 80`;
- shape.svg: a 40x30 SVG (width, height and viewBox) with a rectangle, a
  circle and a stroked path.

The files are inputs, committed as written: the fixtures embed them as data
URLs, Chrome decodes them with its own codecs and the port with the image
crate and resvg. Rerun only to change the inputs, then regenerate the
fixtures that embed them (tools/goldens/image-elements.mjs,
scripts/fixtures/raster-references.sh).

It then writes the display lists that draw the files
(crates/excali-raster/tests/fixtures/display-lists/images-decoded.json for
the raster formats, images-svg.json for the SVG), keeping each file's
tolerance and note. `--lists` writes only those, from the committed files.
"""

import base64
import json

import pathlib
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = ROOT / "crates" / "excali-raster" / "tests" / "fixtures" / "images"

W, H = 24, 16


def pixel(x, y, alpha):
    """Quadrant colour with a ramp across x; alpha falls down the rows."""
    ramp = round(x * 255 / (W - 1))
    if y < H // 2:
        rgb = (230, ramp, 40) if x < W // 2 else (ramp, 60, 220)
    else:
        rgb = (40, 200, ramp) if x < W // 2 else (250, 250 - ramp // 2, ramp // 3)
    a = round(255 - y * 200 / (H - 1)) if alpha else 255
    return (*rgb, a)


def rows(alpha):
    return [[pixel(x, y, alpha) for x in range(W)] for y in range(H)]


def png(data):
    def chunk(kind, body):
        return (
            struct.pack(">I", len(body))
            + kind
            + body
            + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
        )

    raw = b"".join(b"\x00" + bytes(c for px in row for c in px) for row in data)
    ihdr = struct.pack(">IIBBBBB", W, H, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def bmp(data):
    stride = (W * 3 + 3) & ~3
    body = b""
    for row in reversed(data):
        line = b"".join(bytes((b, g, r)) for (r, g, b, _) in row)
        body += line + b"\x00" * (stride - len(line))
    header = struct.pack("<IiiHHIIiiII", 40, W, H, 1, 24, 0, len(body), 2835, 2835, 0, 0)
    size = 14 + len(header) + len(body)
    return b"BM" + struct.pack("<IHHI", size, 0, 0, 14 + len(header)) + header + body


SVG = """<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30" viewBox="0 0 40 30">
  <rect x="2" y="2" width="20" height="14" fill="#1971c2"/>
  <circle cx="28" cy="18" r="9" fill="#e03131" fill-opacity="0.8"/>
  <path d="M4 26 L20 20 L36 27" fill="none" stroke="#2f9e44" stroke-width="2.5"/>
</svg>
"""


def with_orientation(jpeg, orientation):
    """The JPEG with every APP1 segment replaced by an Exif block holding
    only IFD0's Orientation tag (0x0112, SHORT), placed after APP0."""
    assert jpeg[:2] == b"\xff\xd8"
    tiff = (
        b"MM\x00\x2a\x00\x00\x00\x08"
        + struct.pack(">H", 1)
        + struct.pack(">HHIHH", 0x0112, 3, 1, orientation, 0)
        + struct.pack(">I", 0)
    )
    payload = b"Exif\x00\x00" + tiff
    app1 = b"\xff\xe1" + struct.pack(">H", len(payload) + 2) + payload
    out, i = [jpeg[:2]], 2
    while True:
        marker = jpeg[i : i + 2]
        if marker == b"\xff\xda":  # start of scan: the rest is image data
            out.append(jpeg[i:])
            break
        length = struct.unpack(">H", jpeg[i + 2 : i + 4])[0]
        segment = jpeg[i : i + 2 + length]
        if marker != b"\xff\xe1":
            out.append(segment)
        if marker == b"\xff\xe0":
            out.append(app1)
        i += 2 + length
    return b"".join(out)


LISTS = ROOT / "crates" / "excali-raster" / "tests" / "fixtures" / "display-lists"

RASTER_FORMATS = [
    ("png", "image/png", "quad.png"),
    ("bmp", "image/bmp", "opaque.bmp"),
    ("gif", "image/gif", "opaque.gif"),
    ("jpeg", "image/jpeg", "opaque.jpg"),
    ("webp", "image/webp", "lossless.webp"),
    ("webp-lossy", "image/webp", "lossy.webp"),
]


def data_url(mime, name):
    return f"data:{mime};base64,{base64.b64encode((OUT / name).read_bytes()).decode()}"


def decoded_list():
    """Each raster format at natural size, scaled 2x with smoothing, and
    cropped 3x; the Exif-oriented JPEG at its displayed size and 2x; a PNG
    flipped inside a rounded clip."""
    images = {id: {"dataUrl": data_url(mime, name)} for id, mime, name in RASTER_FORMATS}
    images["oriented"] = {"dataUrl": data_url("image/jpeg", "oriented.jpg")}
    items = [{"type": "fill", "color": "#ffffff", "path": [["rect", 0, 0, 300, 160]]}]
    for i, (id, _, _) in enumerate(RASTER_FORMATS):
        col = 4 + i * 48
        items.append({"type": "image", "id": id, "dest": [col, 4, 24, 16]})
        items.append({"type": "image", "id": id, "dest": [col, 24, 48, 32]})
        items.append({"type": "image", "id": id, "source": [4, 2, 12, 10], "dest": [col, 60, 36, 30]})
    items.append({"type": "image", "id": "oriented", "dest": [4, 96, 16, 24]})
    items.append({"type": "image", "id": "oriented", "dest": [24, 96, 32, 48]})
    items.append(
        {
            "type": "group",
            "transform": [-1, 0, 0, 1, 290, 100],
            "clip": {"path": [["roundRect", 0, 0, 60, 40, 10]]},
            "items": [{"type": "image", "id": "png", "dest": [0, 0, 60, 40]}],
        }
    )
    return {
        "description": "drawImage of raster image files as the browser decodes them from data URLs (tests/fixtures/images, scripts/fixtures/raster-images.py): PNG with alpha, BMP, GIF, baseline JPEG, lossless and lossy WebP at natural size, scaled 2x with smoothing and a crop scaled 3x; a JPEG with Exif Orientation 6; a PNG flipped inside a rounded clip. Chrome decodes with its own codecs, the port with the image crate (excali_raster::decode) and Skia's 4-bit bilinear filter.",
        "width": 300,
        "height": 160,
        "images": images,
        "items": items,
    }


def svg_list():
    """The SVG at natural size, scaled 2.5x, cropped, with the dark filter,
    rotated at 70% opacity."""
    items = [{"type": "fill", "color": "#ffffff", "path": [["rect", 0, 0, 220, 170]]}]
    items.append({"type": "image", "id": "svg", "dest": [4, 4, 40, 30]})
    items.append({"type": "image", "id": "svg", "dest": [48, 4, 100, 75]})
    items.append({"type": "image", "id": "svg", "source": [10, 5, 25, 20], "dest": [152, 4, 50, 40]})
    items.append({"type": "image", "id": "svg", "filter": "dark", "dest": [152, 50, 40, 30]})
    items.append(
        {
            "type": "group",
            "transform": [0.8660254037844387, 0.5, -0.5, 0.8660254037844387, 40, 90],
            "opacity": 0.7,
            "items": [{"type": "image", "id": "svg", "dest": [0, 0, 60, 45]}],
        }
    )
    return {
        "description": "drawImage of an SVG file from a data URL (tests/fixtures/images/shape.svg: a rectangle, a circle at 80% fill opacity and a stroked polyline): at natural size, scaled 2.5x, cropped, with the dark filter, rotated at 70% opacity. Chrome renders the SVG itself; the port parses it with usvg and draws it as vector fills and strokes at the draw's resolution.",
        "width": 220,
        "height": 170,
        "images": {"svg": {"dataUrl": data_url("image/svg+xml", "shape.svg")}},
        "items": items,
    }


def write_list(name, fixture):
    """The fixture with the committed file's tolerance and note, one item
    per line."""
    path = LISTS / f"{name}.json"
    old = json.loads(path.read_text()) if path.exists() else {}
    head = {
        "description": fixture["description"],
        "width": fixture["width"],
        "height": fixture["height"],
        "tolerance": old.get("tolerance", {"channel": 0, "pixels": 0}),
        "toleranceNote": old.get("toleranceNote", "to be measured"),
        "images": fixture["images"],
    }
    text = "{\n" + ",\n".join(f" {json.dumps(k)}: {json.dumps(v)}" for k, v in head.items())
    text += ',\n "items": [\n' + ",\n".join("  " + json.dumps(i) for i in fixture["items"]) + "\n ]\n}\n"
    path.write_text(text, encoding="utf-8")
    print(f"{path.relative_to(ROOT)}")


def write_lists():
    write_list("images-decoded", decoded_list())
    write_list("images-svg", svg_list())


def run(*cmd):
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL)


def main():
    if sys.argv[1:] == ["--lists"]:
        write_lists()
        return
    if sys.argv[1:]:
        sys.exit("usage: raster-images.py [--lists]")
    for tool in ("sips", "cwebp"):
        if shutil.which(tool) is None:
            sys.exit(f"raster images: {tool} not found (macOS sips, libwebp cwebp)")
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "quad.png").write_bytes(png(rows(alpha=True)))
    (OUT / "opaque.png").write_bytes(png(rows(alpha=False)))
    (OUT / "opaque.bmp").write_bytes(bmp(rows(alpha=False)))
    (OUT / "shape.svg").write_text(SVG, encoding="utf-8")
    with tempfile.TemporaryDirectory() as tmp:
        src = pathlib.Path(tmp) / "opaque.png"
        shutil.copy(OUT / "opaque.png", src)
        for fmt, name in (("gif", "opaque.gif"), ("jpeg", "opaque.jpg")):
            out = pathlib.Path(tmp) / name
            run("sips", "-s", "format", fmt, str(src), "--out", str(out))
            run("sips", "-d", "profile", str(out))
            shutil.copy(out, OUT / name)
    jpeg = (OUT / "opaque.jpg").read_bytes()
    (OUT / "oriented.jpg").write_bytes(with_orientation(jpeg, 6))
    run("cwebp", "-quiet", "-lossless", str(OUT / "opaque.png"), "-o", str(OUT / "lossless.webp"))
    run("cwebp", "-quiet", "-q", "80", str(OUT / "opaque.png"), "-o", str(OUT / "lossy.webp"))
    for p in sorted(OUT.iterdir()):
        print(f"{p.relative_to(ROOT)}: {p.stat().st_size} bytes")
    write_lists()


if __name__ == "__main__":
    main()
