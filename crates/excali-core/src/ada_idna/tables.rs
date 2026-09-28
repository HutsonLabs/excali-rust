//! ada's Unicode and IDNA tables (`src/table_store.hpp` in ada 4.0.0): the
//! blob `ada::idna::table_blob::compressed`, carried as it is in a zlib
//! wrapper (`tables.zlib`, written by `scripts/fixtures/ada-idna-tables.py`),
//! inflated once on first use and read little-endian at the offsets in
//! [`layout`](super::layout).

use std::sync::OnceLock;

use super::layout::*;
use crate::encode::checksum::crc32;

static BLOB: &[u8] = include_bytes!("tables.zlib");

// ada's static_asserts on the blob layout (table_store.hpp).
const _: () = {
    assert!(COUNT_DECOMPOSITION_INDEX == 4352);
    assert!(COUNT_CCC_INDEX == 4352);
    assert!(COUNT_COMPOSITION_INDEX == 4352);
    assert!(COUNT_DECOMPOSITION_BLOCK == DECOMPOSITION_BLOCK_ROWS * DECOMPOSITION_BLOCK_COLS);
    assert!(COUNT_CCC_BLOCK == CCC_BLOCK_ROWS * CCC_BLOCK_COLS);
    assert!(COUNT_COMPOSITION_BLOCK == COMPOSITION_BLOCK_ROWS * COMPOSITION_BLOCK_COLS);
    assert!(COUNT_DIR_START == DIR_TABLE_COUNT);
    assert!(COUNT_DIR_FINAL == DIR_TABLE_COUNT);
    assert!(COUNT_DIR_VALUE == DIR_TABLE_COUNT);
    assert!(COUNT_ID_CONTINUE_FLAT == ID_CONTINUE_COUNT * 2);
    assert!(COUNT_ID_START_FLAT == ID_START_COUNT * 2);
    assert!(COUNT_COMBINING_FLAT == COMBINING_RANGE_COUNT * 2);
    assert!(OFF_IDNA_STAGE1 + COUNT_IDNA_STAGE1 * 2 <= UNCOMPRESSED_SIZE);
    assert!(OFF_IDNA_STAGE2 + COUNT_IDNA_STAGE2 * 2 <= UNCOMPRESSED_SIZE);
    assert!(OFF_IDNA_BOOL_BLOCKS + COUNT_IDNA_BOOL_BLOCKS * 8 <= UNCOMPRESSED_SIZE);
    assert!(OFF_IDNA_UTF8_MAPPINGS + COUNT_IDNA_UTF8_MAPPINGS <= UNCOMPRESSED_SIZE);
    assert!(OFF_DECOMPOSITION_INDEX + COUNT_DECOMPOSITION_INDEX <= UNCOMPRESSED_SIZE);
    assert!(OFF_DECOMPOSITION_BLOCK + COUNT_DECOMPOSITION_BLOCK * 2 <= UNCOMPRESSED_SIZE);
    assert!(OFF_DECOMPOSITION_DATA + COUNT_DECOMPOSITION_DATA * 4 <= UNCOMPRESSED_SIZE);
    assert!(OFF_CCC_INDEX + COUNT_CCC_INDEX <= UNCOMPRESSED_SIZE);
    assert!(OFF_CCC_BLOCK + COUNT_CCC_BLOCK <= UNCOMPRESSED_SIZE);
    assert!(OFF_COMPOSITION_INDEX + COUNT_COMPOSITION_INDEX <= UNCOMPRESSED_SIZE);
    assert!(OFF_COMPOSITION_BLOCK + COUNT_COMPOSITION_BLOCK * 2 <= UNCOMPRESSED_SIZE);
    assert!(OFF_COMPOSITION_DATA + COUNT_COMPOSITION_DATA * 4 <= UNCOMPRESSED_SIZE);
    assert!(OFF_ID_CONTINUE_FLAT + COUNT_ID_CONTINUE_FLAT * 4 <= UNCOMPRESSED_SIZE);
    assert!(OFF_ID_START_FLAT + COUNT_ID_START_FLAT * 4 <= UNCOMPRESSED_SIZE);
    assert!(OFF_DIR_START + COUNT_DIR_START * 4 <= UNCOMPRESSED_SIZE);
    assert!(OFF_DIR_FINAL + COUNT_DIR_FINAL * 4 <= UNCOMPRESSED_SIZE);
    assert!(OFF_DIR_VALUE + COUNT_DIR_VALUE <= UNCOMPRESSED_SIZE);
    assert!(OFF_COMBINING_FLAT + COUNT_COMBINING_FLAT * 4 <= UNCOMPRESSED_SIZE);
};

/// The inflated blob.
pub(super) struct Tables {
    buf: Vec<u8>,
}

/// The tables, inflated on first use. The blob is part of the binary and
/// its size and CRC-32 are checked (as ada's `ensure_tables` does), so a
/// failure here is a build defect, not an input one.
pub(super) fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let buf = crate::encode::inflate(BLOB).expect("ada IDNA tables inflate");
        assert_eq!(buf.len(), UNCOMPRESSED_SIZE, "ada IDNA tables size");
        assert_eq!(crc32(0, &buf), UNCOMPRESSED_CRC32, "ada IDNA tables CRC-32");
        Tables { buf }
    })
}

impl Tables {
    fn u8(&self, off: usize, i: usize) -> u8 {
        self.buf[off + i]
    }

    fn u16(&self, off: usize, i: usize) -> u16 {
        let at = off + 2 * i;
        u16::from_le_bytes([self.buf[at], self.buf[at + 1]])
    }

    fn u32(&self, off: usize, i: usize) -> u32 {
        let at = off + 4 * i;
        u32::from_le_bytes(self.buf[at..at + 4].try_into().unwrap())
    }

    fn u64(&self, off: usize, i: usize) -> u64 {
        let at = off + 8 * i;
        u64::from_le_bytes(self.buf[at..at + 8].try_into().unwrap())
    }

    // -- mapping ------------------------------------------------------------

    pub(super) fn idna_stage1(&self, i: usize) -> u16 {
        self.u16(OFF_IDNA_STAGE1, i)
    }

    pub(super) fn idna_stage2(&self, i: usize) -> u16 {
        self.u16(OFF_IDNA_STAGE2, i)
    }

    pub(super) fn idna_bool_block(&self, i: usize) -> u64 {
        self.u64(OFF_IDNA_BOOL_BLOCKS, i)
    }

    /// `idna_utf8_mappings + offset`, to the end of the section.
    pub(super) fn idna_utf8_mapping(&self, offset: usize) -> &[u8] {
        &self.buf
            [OFF_IDNA_UTF8_MAPPINGS + offset..OFF_IDNA_UTF8_MAPPINGS + COUNT_IDNA_UTF8_MAPPINGS]
    }

    pub(super) const IDNA_UTF8_MAPPINGS_SIZE: usize = COUNT_IDNA_UTF8_MAPPINGS;

    // -- normalization ------------------------------------------------------

    /// `decomposition_block_row(decomposition_index[c >> 8]) + c % 256`:
    /// the entry and the next one.
    pub(super) fn decomposition(&self, c: u32) -> [u16; 2] {
        let mut row = usize::from(self.u8(OFF_DECOMPOSITION_INDEX, (c >> 8) as usize));
        if row >= DECOMPOSITION_BLOCK_ROWS {
            row = 0;
        }
        let at = row * DECOMPOSITION_BLOCK_COLS + (c % 256) as usize;
        [
            self.u16(OFF_DECOMPOSITION_BLOCK, at),
            self.u16(OFF_DECOMPOSITION_BLOCK, at + 1),
        ]
    }

    pub(super) fn decomposition_data(&self, i: usize) -> u32 {
        self.u32(OFF_DECOMPOSITION_DATA, i)
    }

    pub(super) const DECOMPOSITION_DATA_SIZE: usize = COUNT_DECOMPOSITION_DATA;

    /// `get_ccc`: the canonical combining class, 0 past U+10FFFF.
    pub(super) fn ccc(&self, c: u32) -> u8 {
        if c >= 0x11_0000 {
            return 0;
        }
        let mut row = usize::from(self.u8(OFF_CCC_INDEX, (c >> 8) as usize));
        if row >= CCC_BLOCK_ROWS {
            row = 0;
        }
        self.u8(OFF_CCC_BLOCK, row * CCC_BLOCK_COLS + (c % 256) as usize)
    }

    /// `composition_block_row(composition_index[c >> 8]) + c % 256`: the
    /// entry and the next one.
    pub(super) fn composition(&self, c: u32) -> [u16; 2] {
        let mut row = usize::from(self.u8(OFF_COMPOSITION_INDEX, (c >> 8) as usize));
        if row >= COMPOSITION_BLOCK_ROWS {
            row = 0;
        }
        let at = row * COMPOSITION_BLOCK_COLS + (c % 256) as usize;
        [
            self.u16(OFF_COMPOSITION_BLOCK, at),
            self.u16(OFF_COMPOSITION_BLOCK, at + 1),
        ]
    }

    pub(super) fn composition_data(&self, i: usize) -> u32 {
        self.u32(OFF_COMPOSITION_DATA, i)
    }

    pub(super) const COMPOSITION_DATA_SIZE: usize = COUNT_COMPOSITION_DATA;

    // -- validity -----------------------------------------------------------

    pub(super) fn dir_start(&self, i: usize) -> u32 {
        self.u32(OFF_DIR_START, i)
    }

    pub(super) fn dir_final(&self, i: usize) -> u32 {
        self.u32(OFF_DIR_FINAL, i)
    }

    pub(super) fn dir_value(&self, i: usize) -> u8 {
        self.u8(OFF_DIR_VALUE, i)
    }

    pub(super) const DIR_TABLE_COUNT: usize = DIR_TABLE_COUNT;

    /// The combining-mark range `i`: first and last code point.
    pub(super) fn combining_range(&self, i: usize) -> [u32; 2] {
        [
            self.u32(OFF_COMBINING_FLAT, 2 * i),
            self.u32(OFF_COMBINING_FLAT, 2 * i + 1),
        ]
    }

    pub(super) const COMBINING_RANGE_COUNT: usize = COMBINING_RANGE_COUNT;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_inflates_to_adas_tables() {
        let t = tables();
        assert_eq!(t.buf.len(), UNCOMPRESSED_SIZE);
        // The zlib wrapper adds a 2-byte header and a 4-byte Adler-32.
        assert_eq!(BLOB.len(), COMPRESSED_SIZE + 6);
    }

    /// The decomposition and composition rows ada indexes are
    /// nondecreasing, so an entry's length (next minus this) is never
    /// negative and the port's unsigned arithmetic is ada's.
    #[test]
    fn rows_are_nondecreasing() {
        let t = tables();
        for i in 0..COUNT_DECOMPOSITION_BLOCK {
            if i % DECOMPOSITION_BLOCK_COLS != DECOMPOSITION_BLOCK_COLS - 1 {
                let (a, b) = (
                    t.u16(OFF_DECOMPOSITION_BLOCK, i),
                    t.u16(OFF_DECOMPOSITION_BLOCK, i + 1),
                );
                assert!(a >> 2 <= b >> 2, "decomposition block entry {i}");
            }
        }
        for i in 0..COUNT_COMPOSITION_BLOCK {
            if i % COMPOSITION_BLOCK_COLS != COMPOSITION_BLOCK_COLS - 1 {
                let (a, b) = (
                    t.u16(OFF_COMPOSITION_BLOCK, i),
                    t.u16(OFF_COMPOSITION_BLOCK, i + 1),
                );
                assert!(a <= b, "composition block entry {i}");
            }
        }
    }

    /// ada's combining-mark ranges stop short of the marks Unicode 14
    /// added (U+1AC1..U+1ACE), while its mapping is IDNA 17.
    #[test]
    fn validity_tables_predate_unicode_14() {
        let t = tables();
        let ranges: Vec<[u32; 2]> = (0..COMBINING_RANGE_COUNT)
            .map(|i| t.combining_range(i))
            .collect();
        assert!(ranges.contains(&[0x1AB0, 0x1AC0]));
        assert!(!ranges.iter().any(|r| (r[0]..=r[1]).contains(&0x1AD3)));
    }
}
