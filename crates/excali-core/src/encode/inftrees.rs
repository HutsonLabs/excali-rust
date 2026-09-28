//! Huffman decoding tables, ported from pako 2.0.3 `lib/zlib/inftrees.js`
//! (zlib's `inftrees.c`).
//!
//! A table entry packs `bits << 24 | op << 16 | val`, as pako's `Int32Array`
//! entries do:
//!
//! - `op == 0`: a literal, `val` is the byte (or the code length symbol);
//! - `op & 16`: a length or distance base `val` with `op & 15` extra bits;
//! - `op & 64 == 0` and `op != 0`: a link to a sub-table of `op` index bits
//!   at offset `val`;
//! - `op == 96` (32 + 64): end of block;
//! - any other `op & 64`: an invalid code.

pub(crate) const MAXBITS: usize = 15;
/// Table space for the literal/length code (`ENOUGH_LENS`).
pub(crate) const ENOUGH_LENS: usize = 852;
/// Table space for the distance code (`ENOUGH_DISTS`).
pub(crate) const ENOUGH_DISTS: usize = 592;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodeType {
    /// The code length code of a dynamic block (19 symbols).
    Codes,
    /// The literal/length code.
    Lens,
    /// The distance code.
    Dists,
}

/// Length codes 257..285 base.
const LBASE: [u16; 31] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258, 0, 0,
];
/// Length codes 257..285 extra bits (as `op`: 16 + extra; 72 and 78 mark
/// the invalid symbols 286 and 287).
const LEXT: [u8; 31] = [
    16, 16, 16, 16, 16, 16, 16, 16, 17, 17, 17, 17, 18, 18, 18, 18, 19, 19, 19, 19, 20, 20, 20, 20,
    21, 21, 21, 21, 16, 72, 78,
];
/// Distance codes 0..29 base.
const DBASE: [u16; 32] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577, 0, 0,
];
/// Distance codes 0..29 extra bits (as `op`; 64 marks codes 30 and 31).
const DEXT: [u8; 32] = [
    16, 16, 16, 16, 17, 17, 18, 18, 19, 19, 20, 20, 21, 21, 22, 22, 23, 23, 24, 24, 25, 25, 26, 26,
    27, 27, 28, 28, 29, 29, 64, 64,
];

/// `inflate_table(type, lens, lens_index, codes, table, table_index, work,
/// opts)`: build the decoding table for the code lengths `lens[..codes]`
/// into `table`, with a root table of `*bits` index bits (updated to the
/// root size used). Returns 0 on success, -1 for an over-subscribed or
/// incomplete set of lengths, 1 if the table space would be exceeded.
pub(crate) fn inflate_table(
    ty: CodeType,
    lens: &[u16],
    codes: usize,
    table: &mut [u32],
    work: &mut [u16],
    bits: &mut u32,
) -> i32 {
    let mut count = [0u16; MAXBITS + 1];
    let mut offs = [0u16; MAXBITS + 1];

    // Accumulate lengths for codes (lens[] all in 0..MAXBITS).
    for &l in &lens[..codes] {
        count[l as usize] += 1;
    }

    // Bound code lengths, force root to be within code lengths.
    let mut root = *bits;
    let mut max = MAXBITS as u32;
    while max >= 1 {
        if count[max as usize] != 0 {
            break;
        }
        max -= 1;
    }
    if root > max {
        root = max;
    }
    if max == 0 {
        // No symbols to code at all: make a table that decodes an invalid
        // code, and wait for decoding to report the error.
        table[0] = (1 << 24) | (64 << 16);
        table[1] = (1 << 24) | (64 << 16);
        *bits = 1;
        return 0;
    }
    let mut min = 1u32;
    while min < max {
        if count[min as usize] != 0 {
            break;
        }
        min += 1;
    }
    if root < min {
        root = min;
    }

    // Check for an over-subscribed or incomplete set of lengths.
    let mut left: i32 = 1;
    for &c in &count[1..=MAXBITS] {
        left <<= 1;
        left -= i32::from(c);
        if left < 0 {
            return -1;
        }
    }
    if left > 0 && (ty == CodeType::Codes || max != 1) {
        return -1;
    }

    // Offsets into the symbol table for each length, then sort symbols by
    // length, by symbol order within each length.
    offs[1] = 0;
    for len in 1..MAXBITS {
        offs[len + 1] = offs[len] + count[len];
    }
    for (sym, &l) in lens[..codes].iter().enumerate() {
        if l != 0 {
            work[offs[l as usize] as usize] = sym as u16;
            offs[l as usize] += 1;
        }
    }

    // Symbols below `end` are literals, above it bases, `end` itself is
    // end-of-block.
    let end: i32 = match ty {
        CodeType::Codes => 19,
        CodeType::Lens => 256,
        CodeType::Dists => -1,
    };
    let entry = |sym: u16| -> (u32, u32) {
        let s = i32::from(sym);
        if s < end {
            (0, u32::from(sym))
        } else if s > end {
            let s = sym as usize;
            match ty {
                CodeType::Lens => (u32::from(LEXT[s - 257]), u32::from(LBASE[s - 257])),
                CodeType::Dists => (u32::from(DEXT[s]), u32::from(DBASE[s])),
                CodeType::Codes => unreachable!("code length symbols are all below 19"),
            }
        } else {
            (32 + 64, 0)
        }
    };

    let mut huff: u32 = 0; // starting code
    let mut sym: usize = 0; // starting code symbol
    let mut len: u32 = min; // starting code length
    let mut next: usize = 0; // current table to fill in
    let mut curr: u32 = root; // current table index bits
    let mut drop: u32 = 0; // current bits to drop from code for index
    let mut low: u32 = u32::MAX; // trigger new sub-table when len > root
    let mut used: u32 = 1 << root; // use root table entries
    let mask: u32 = used - 1; // mask for comparing low

    let too_big = |used: u32| {
        (ty == CodeType::Lens && used as usize > ENOUGH_LENS)
            || (ty == CodeType::Dists && used as usize > ENOUGH_DISTS)
    };
    if too_big(used) {
        return 1;
    }

    // Process all codes and make table entries.
    loop {
        let here_bits = len - drop;
        let (here_op, here_val) = entry(work[sym]);
        let here = (here_bits << 24) | (here_op << 16) | here_val;

        // Replicate for those indices with low len bits equal to huff.
        let incr = 1u32 << (len - drop);
        let mut fill = 1u32 << curr;
        let table_size = fill; // offset to the next table
        loop {
            fill -= incr;
            table[next + (huff >> drop) as usize + fill as usize] = here;
            if fill == 0 {
                break;
            }
        }

        // Backwards increment the len-bit code huff.
        let mut incr = 1u32 << (len - 1);
        while huff & incr != 0 {
            incr >>= 1;
        }
        if incr != 0 {
            huff &= incr - 1;
            huff += incr;
        } else {
            huff = 0;
        }

        // Go to the next symbol, update count, len.
        sym += 1;
        count[len as usize] -= 1;
        if count[len as usize] == 0 {
            if len == max {
                break;
            }
            len = u32::from(lens[work[sym] as usize]);
        }

        // Create a new sub-table if needed.
        if len > root && (huff & mask) != low {
            // If first time, transition to sub-tables.
            if drop == 0 {
                drop = root;
            }
            // Increment past the last table.
            next += table_size as usize;

            // Determine the length of the next table.
            curr = len - drop;
            let mut left: i32 = 1 << curr;
            while curr + drop < max {
                left -= i32::from(count[(curr + drop) as usize]);
                if left <= 0 {
                    break;
                }
                curr += 1;
                left <<= 1;
            }

            // Check for enough space.
            used += 1 << curr;
            if too_big(used) {
                return 1;
            }

            // Point the entry in the root table to the sub-table.
            low = huff & mask;
            table[low as usize] = (root << 24) | (curr << 16) | next as u32;
        }
    }

    // Fill in the remaining table entry if the code is incomplete (at most
    // one entry, since an incomplete code has a maximum length of one bit).
    if huff != 0 {
        table[next + huff as usize] = ((len - drop) << 24) | (64 << 16);
    }

    *bits = root;
    0
}
