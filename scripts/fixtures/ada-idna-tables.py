#!/usr/bin/env python3
"""ada's IDNA tables for excali-core's `ada_idna` (ex-109).

`new URL` in Node 26 (the engine the library URL fixtures are recorded with)
parses a host outside ASCII with ada's `ada::idna::to_ascii`, whose mapping,
normalization, bidi and combining-mark tables ship in ada.cpp as one raw
DEFLATE blob (`ada::idna::table_blob`). This script takes that blob, as it
is, from the ada release Node 26 bundles (process.versions.ada), and writes:

  crates/excali-core/src/ada_idna/tables.zlib  the blob in a zlib wrapper
                                               (the port's inflate reads zlib)
  crates/excali-core/src/ada_idna/layout.rs    the blob's section offsets,
                                               counts and CRC-32, the mapping
                                               table's constants and the
                                               ContextJ code point lists

  scripts/fixtures/ada-idna-tables.py          regenerate both
  scripts/fixtures/ada-idna-tables.py --check  exit 1 if either would change

ada.cpp is downloaded once into the shared .tools directory and refused
unless its SHA-256 matches the pin below. ada is Apache-2.0 OR MIT.

Environment:
  TOOLS_DIR  where ada.cpp is kept; default <main clone>/.tools
"""

import hashlib
import os
import re
import subprocess
import sys
import urllib.request
import zlib
from pathlib import Path

ADA_VERSION = "4.0.0"
ADA_URL = f"https://github.com/ada-url/ada/releases/download/v{ADA_VERSION}/ada.cpp"
ADA_SHA256 = "1c722f8f5355d6bf2bf3601870684edc875aa8f56b4d4735440d96263dd0207c"

ROOT = Path(__file__).resolve().parents[2]
OUT_DIR = ROOT / "crates" / "excali-core" / "src" / "ada_idna"
ZLIB = OUT_DIR / "tables.zlib"
LAYOUT = OUT_DIR / "layout.rs"


def die(message):
    print(f"ada-idna-tables: {message}", file=sys.stderr)
    sys.exit(1)


def tools_dir():
    if os.environ.get("TOOLS_DIR"):
        return Path(os.environ["TOOLS_DIR"])
    common = subprocess.run(
        ["git", "-C", str(ROOT), "rev-parse", "--path-format=absolute", "--git-common-dir"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    return Path(common).parent / ".tools"


def ada_source():
    path = tools_dir() / f"ada-{ADA_VERSION}" / "ada.cpp"
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        print(f"downloading {ADA_URL}", file=sys.stderr)
        with urllib.request.urlopen(ADA_URL) as response:
            data = response.read()
        got = hashlib.sha256(data).hexdigest()
        if got != ADA_SHA256:
            die(f"sha256 mismatch for {ADA_URL}: got {got}, want {ADA_SHA256}")
        part = path.with_suffix(".part")
        part.write_bytes(data)
        part.rename(path)
    data = path.read_bytes()
    got = hashlib.sha256(data).hexdigest()
    if got != ADA_SHA256:
        die(f"sha256 mismatch for {path}: got {got}, want {ADA_SHA256}")
    return data.decode("utf-8")


def table_blob(source):
    m = re.search(
        r"namespace ada::idna::table_blob \{(.*?)alignas\(8\) inline constexpr uint8_t compressed\[\] = \{(.*?)\};",
        source,
        re.S,
    )
    if not m:
        die("no ada::idna::table_blob in ada.cpp")
    constants = {}
    for name, value in re.findall(r"constexpr (?:size_t|uint32_t) (\w+) = (0x[0-9A-Fa-f]+|\d+)u?;", m.group(1)):
        constants[name] = int(value, 0)
    compressed = bytes(int(x, 16) for x in re.findall(r"0x([0-9a-fA-F]{2})", m.group(2)))
    if len(compressed) != constants["compressed_size"]:
        die(f"compressed blob is {len(compressed)} bytes, table_blob says {constants['compressed_size']}")
    raw = zlib.decompress(compressed, -15)
    if len(raw) != constants["uncompressed_size"]:
        die(f"inflated blob is {len(raw)} bytes, table_blob says {constants['uncompressed_size']}")
    if zlib.crc32(raw) != constants["uncompressed_crc32"]:
        die("inflated blob fails table_blob::uncompressed_crc32")
    return constants, compressed, raw


def source_arrays(source):
    """The ContextJ lists in validity.cpp and the mapping constants in
    mapping_tables.cpp, which are code, not blob."""
    validity = source[source.index("/* begin file src/validity.cpp */") : source.index("/* end file src/validity.cpp */")]
    arrays = {}
    for name in ("virama", "R", "L", "D"):
        m = re.search(r"constexpr static uint32_t " + name + r"\[\] = \{(.*?)\};", validity, re.S)
        if not m:
            die(f"no {name}[] in validity.cpp")
        arrays[name] = [int(x, 16) for x in re.findall(r"0x[0-9a-fA-F]+", m.group(1))]
    mapping = source[
        source.index("/* begin file src/mapping_tables.cpp */") : source.index("/* end file src/mapping_tables.cpp */")
    ]
    constants = {}
    for name, value in re.findall(r"constexpr uint(?:16|32)_t (IDNA_\w+)\s*=\s*(0x[0-9A-Fa-f]+|\d+)u?;", mapping):
        constants[name] = int(value, 0)
    want = {
        "IDNA_BLOCK_BITS", "IDNA_BLOCK_SIZE", "IDNA_BLOCK_MASK", "IDNA_VALID", "IDNA_DISALLOWED", "IDNA_IGNORED",
        "IDNA_BOOL_FLAG", "IDNA_LOW_RANGE_END", "IDNA_HIGH_IGNORED_START", "IDNA_HIGH_IGNORED_END",
    }
    if set(constants) != want:
        die(f"mapping_tables.cpp constants are {sorted(constants)}")
    unicode = re.search(r"// IDNA (\d+\.\d+\.\d+)", mapping)
    return arrays, constants, unicode.group(1) if unicode else None


def zlib_stream(compressed, raw):
    # CMF 0x78 (deflate, 32K window), FLG 0x9c: (0x78 * 256 + 0x9c) % 31 == 0.
    return b"\x78\x9c" + compressed + zlib.adler32(raw).to_bytes(4, "big")


def rust_array(name, values):
    lines = [f"pub(super) const {name}: [u32; {len(values)}] = ["]
    for i in range(0, len(values), 8):
        lines.append("    " + ", ".join(f"0x{v:04X}" for v in values[i : i + 8]) + ",")
    lines.append("];")
    return lines


def layout_rs(constants, arrays, mapping, idna_version):
    lines = [
        "// Generated by scripts/fixtures/ada-idna-tables.py from ada.cpp",
        f"// {ADA_VERSION}. Do not edit.",
        "",
        "// ada::idna::table_blob: the blob's layout.",
    ]
    for name, value in constants.items():
        rust = "u32" if name == "uncompressed_crc32" else "usize"
        text = f"0x{value:08X}" if name == "uncompressed_crc32" else str(value)
        lines.append(f"pub(super) const {name.upper()}: {rust} = {text};")
    lines += ["", f"// src/mapping_tables.cpp (IDNA {idna_version})."]
    for name, value in mapping.items():
        lines.append(f"pub(super) const {name}: u32 = 0x{value:X};")
    lines += ["", "// src/validity.cpp: ContextJ (virama, joining types R, L and D)."]
    for name, rust in (("virama", "VIRAMA"), ("R", "JOINING_R"), ("L", "JOINING_L"), ("D", "JOINING_D")):
        lines += rust_array(rust, arrays[name])
    return "\n".join(lines) + "\n"


def main(argv):
    check = False
    for arg in argv:
        if arg == "--check":
            check = True
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        else:
            print("usage: ada-idna-tables.py [--check]", file=sys.stderr)
            return 2
    source = ada_source()
    constants, compressed, raw = table_blob(source)
    arrays, mapping, idna_version = source_arrays(source)
    outputs = {
        ZLIB: zlib_stream(compressed, raw),
        LAYOUT: layout_rs(constants, arrays, mapping, idna_version).encode(),
    }
    if check:
        stale = [p for p, data in outputs.items() if not p.exists() or p.read_bytes() != data]
        for p in stale:
            print(f"stale: {p.relative_to(ROOT)}", file=sys.stderr)
        if stale:
            print("run scripts/fixtures/ada-idna-tables.py", file=sys.stderr)
            return 1
        print(f"ada {ADA_VERSION} IDNA tables up to date")
        return 0
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for p, data in outputs.items():
        p.write_bytes(data)
        print(f"wrote {p.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
