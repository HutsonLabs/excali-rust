#!/usr/bin/env python3
"""Offline tests for scripts/fixtures/ada-idna-tables.py: the blob parser,
the zlib wrapper and the extraction of ada's source lists, on a synthetic
ada.cpp."""

import importlib.util
import struct
import unittest
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("ada_idna_tables", HERE / "ada-idna-tables.py")
tables = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tables)


def synthetic_source(raw, crc=None, compressed_size=None):
    c = zlib.compressobj(9, zlib.DEFLATED, -15)
    compressed = c.compress(raw) + c.flush()
    body = ",".join(f"0x{b:02x}" for b in compressed)
    return (
        "namespace ada::idna::table_blob {\n"
        f"constexpr size_t uncompressed_size = {len(raw)};\n"
        f"constexpr size_t compressed_size = {compressed_size or len(compressed)};\n"
        f"constexpr uint32_t uncompressed_crc32 = 0x{(crc if crc is not None else zlib.crc32(raw)):08X}u;\n"
        "constexpr size_t off_dir_start = 0;\n"
        f"alignas(8) inline constexpr uint8_t compressed[] = {{\n{body}\n}};\n"
        "}\n"
        "/* begin file src/mapping_tables.cpp */\n// IDNA 17.0.0\n"
        "constexpr uint32_t IDNA_BLOCK_BITS = 6u;\nconstexpr uint32_t IDNA_BLOCK_SIZE = 64u;\n"
        "constexpr uint32_t IDNA_BLOCK_MASK = 63u;\nconstexpr uint16_t IDNA_VALID      = 0xFFFF;\n"
        "constexpr uint16_t IDNA_DISALLOWED = 0xFFFE;\nconstexpr uint16_t IDNA_IGNORED    = 0x0000;\n"
        "constexpr uint16_t IDNA_BOOL_FLAG  = 0x8000;\nconstexpr uint32_t IDNA_LOW_RANGE_END    = 0x00033480;\n"
        "constexpr uint32_t IDNA_HIGH_IGNORED_START = 0x000E0100;\n"
        "constexpr uint32_t IDNA_HIGH_IGNORED_END   = 0x000E01F0;  // exclusive\n"
        "/* end file src/mapping_tables.cpp */\n"
        "/* begin file src/validity.cpp */\n"
        "  constexpr static uint32_t virama[] = {\n      0x094D,  0x09CD};\n"
        "  constexpr static uint32_t R[] = {\n      0x622, 0x623};\n"
        "  constexpr static uint32_t L[] = {0xa872};\n"
        "  constexpr static uint32_t D[] = {\n      0x620};\n"
        "/* end file src/validity.cpp */\n"
    )


class AdaIdnaTablesTest(unittest.TestCase):
    raw = struct.pack("<8I", *range(8)) * 64

    def test_blob_parses_and_rewraps_as_zlib(self):
        constants, compressed, raw = tables.table_blob(synthetic_source(self.raw))
        self.assertEqual(raw, self.raw)
        self.assertEqual(constants["uncompressed_size"], len(self.raw))
        self.assertEqual(zlib.decompress(tables.zlib_stream(compressed, raw)), self.raw)

    def test_crc_mismatch_is_refused(self):
        with self.assertRaises(SystemExit):
            tables.table_blob(synthetic_source(self.raw, crc=1))

    def test_size_mismatch_is_refused(self):
        with self.assertRaises(SystemExit):
            tables.table_blob(synthetic_source(self.raw, compressed_size=3))

    def test_source_lists_and_layout(self):
        source = synthetic_source(self.raw)
        constants, _, _ = tables.table_blob(source)
        arrays, mapping, version = tables.source_arrays(source)
        self.assertEqual(arrays["virama"], [0x094D, 0x09CD])
        self.assertEqual(arrays["D"], [0x620])
        self.assertEqual(mapping["IDNA_LOW_RANGE_END"], 0x33480)
        self.assertEqual(version, "17.0.0")
        text = tables.layout_rs(constants, arrays, mapping, version)
        self.assertIn("pub(super) const OFF_DIR_START: usize = 0;", text)
        self.assertIn("pub(super) const VIRAMA: [u32; 2] = [", text)
        self.assertIn("pub(super) const IDNA_BOOL_FLAG: u32 = 0x8000;", text)


if __name__ == "__main__":
    unittest.main()
