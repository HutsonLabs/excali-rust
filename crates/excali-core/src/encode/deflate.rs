//! zlib compression, ported from pako 2.0.3 so the output is byte-identical
//! to upstream's `deflate(text)` (`packages/excalidraw/data/encode.ts:1,107`).
//!
//! pako is itself a line-by-line port of zlib's deflate. The general-purpose
//! Rust compressors (miniz_oxide, zlib-rs) choose different matches and
//! block boundaries, so their bytes differ from what upstream writes into
//! PNG and SVG files. This module ports exactly the path upstream runs:
//! `pako.deflate(data)` with its defaults (`lib/deflate.js`: level
//! `Z_DEFAULT_COMPRESSION` = 6, `windowBits` 15, `memLevel` 8,
//! `Z_DEFAULT_STRATEGY`, zlib wrapper), which is `deflate_slow` with lazy
//! matching (`lib/zlib/deflate.js`) and the Huffman coder
//! (`lib/zlib/trees.js`). Other levels, strategies, gzip/raw wrappers and
//! preset dictionaries are not used by upstream and are not ported.
//!
//! pako streams output through a 16 KiB buffer (`chunkSize`) and re-enters
//! `deflate()` each time it fills. deflate's state machine resumes exactly
//! where it stopped, so the bytes do not depend on the buffer size; here the
//! output goes straight into one `Vec`. Likewise zlib's shared
//! `pending_buf` (pending output overlaid on the symbol buffers) becomes
//! separate buffers: with the default strategy the overlay never changes a
//! byte.
//!
//! Function and field names follow pako so the two can be read side by
//! side; comments cite `deflate.js` / `trees.js` in pako 2.0.3.

use std::sync::OnceLock;

use super::checksum::adler32;

// deflate.js constants.
const MAX_WBITS: usize = 15;
const W_SIZE: usize = 1 << MAX_WBITS;
const W_MASK: usize = W_SIZE - 1;
const WINDOW_SIZE: usize = 2 * W_SIZE;
const DEF_MEM_LEVEL: usize = 8;
const HASH_BITS: usize = DEF_MEM_LEVEL + 7;
const HASH_SIZE: usize = 1 << HASH_BITS;
const HASH_MASK: usize = HASH_SIZE - 1;
const HASH_SHIFT: usize = HASH_BITS.div_ceil(MIN_MATCH);
const LIT_BUFSIZE: usize = 1 << (DEF_MEM_LEVEL + 6);

const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const MIN_LOOKAHEAD: usize = MAX_MATCH + MIN_MATCH + 1;
const MAX_DIST: usize = W_SIZE - MIN_LOOKAHEAD;
/// deflate_slow drops a length-3 match farther back than this.
const TOO_FAR: usize = 4096;

// configuration_table[6]: good 8, lazy 16, nice 128, chain 128.
const GOOD_MATCH: usize = 8;
const MAX_LAZY_MATCH: usize = 16;
const NICE_MATCH: usize = 128;
const MAX_CHAIN: usize = 128;
/// zlib header FLEVEL for level 6 ("default algorithm").
const LEVEL_FLAGS: u16 = 2;
const Z_DEFLATED: u16 = 8;

// trees.js constants.
const LENGTH_CODES: usize = 29;
const LITERALS: usize = 256;
const L_CODES: usize = LITERALS + 1 + LENGTH_CODES;
const D_CODES: usize = 30;
const BL_CODES: usize = 19;
const HEAP_SIZE: usize = 2 * L_CODES + 1;
const MAX_BITS: usize = 15;
const MAX_BL_BITS: usize = 7;
const BUF_SIZE: u32 = 16;
const END_BLOCK: usize = 256;
const REP_3_6: usize = 16;
const REPZ_3_10: usize = 17;
const REPZ_11_138: usize = 18;
const STORED_BLOCK: u32 = 0;
const STATIC_TREES: u32 = 1;
const DYN_TREES: u32 = 2;
const DIST_CODE_LEN: usize = 512;

const EXTRA_LBITS: [u8; LENGTH_CODES] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const EXTRA_DBITS: [u8; D_CODES] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const EXTRA_BLBITS: [u8; BL_CODES] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 3, 7];
const BL_ORDER: [u8; BL_CODES] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// The tables `tr_static_init` builds once (trees.js).
struct StaticTables {
    /// `static_ltree`: (code, len) pairs for L_CODES + 2 symbols.
    ltree: [u16; (L_CODES + 2) * 2],
    /// `static_dtree`.
    dtree: [u16; D_CODES * 2],
    /// `_dist_code`.
    dist_code: [u8; DIST_CODE_LEN],
    /// `_length_code`.
    length_code: [u8; MAX_MATCH - MIN_MATCH + 1],
    base_length: [u16; LENGTH_CODES],
    base_dist: [u16; D_CODES],
}

fn tables() -> &'static StaticTables {
    static TABLES: OnceLock<StaticTables> = OnceLock::new();
    TABLES.get_or_init(tr_static_init)
}

/// `tr_static_init` (trees.js).
fn tr_static_init() -> StaticTables {
    let mut t = StaticTables {
        ltree: [0; (L_CODES + 2) * 2],
        dtree: [0; D_CODES * 2],
        dist_code: [0; DIST_CODE_LEN],
        length_code: [0; MAX_MATCH - MIN_MATCH + 1],
        base_length: [0; LENGTH_CODES],
        base_dist: [0; D_CODES],
    };
    // length (0..255) -> length code (0..28)
    let mut length = 0usize;
    let mut code = 0usize;
    while code < LENGTH_CODES - 1 {
        t.base_length[code] = length as u16;
        for _ in 0..(1usize << EXTRA_LBITS[code]) {
            t.length_code[length] = code as u8;
            length += 1;
        }
        code += 1;
    }
    // Length 258 can be code 284 + 5 bits or code 285; use the latter.
    t.length_code[length - 1] = code as u8;

    // dist (0..32K) -> dist code (0..29)
    let mut dist = 0usize;
    code = 0;
    while code < 16 {
        t.base_dist[code] = dist as u16;
        for _ in 0..(1usize << EXTRA_DBITS[code]) {
            t.dist_code[dist] = code as u8;
            dist += 1;
        }
        code += 1;
    }
    dist >>= 7;
    while code < D_CODES {
        t.base_dist[code] = (dist << 7) as u16;
        for _ in 0..(1usize << (EXTRA_DBITS[code] - 7)) {
            t.dist_code[256 + dist] = code as u8;
            dist += 1;
        }
        code += 1;
    }

    // The static literal tree.
    let mut bl_count = [0u16; MAX_BITS + 1];
    for n in 0..=287 {
        let len = match n {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
        t.ltree[n * 2 + 1] = len;
        bl_count[len as usize] += 1;
    }
    gen_codes(&mut t.ltree, L_CODES + 1, &bl_count);

    // The static distance tree is trivial.
    for n in 0..D_CODES {
        t.dtree[n * 2 + 1] = 5;
        t.dtree[n * 2] = bi_reverse(n as u32, 5) as u16;
    }
    t
}

/// `bi_reverse`: reverse the low `len` bits of `code`.
fn bi_reverse(mut code: u32, mut len: u32) -> u32 {
    let mut res = 0u32;
    loop {
        res |= code & 1;
        code >>= 1;
        res <<= 1;
        len -= 1;
        if len == 0 {
            break;
        }
    }
    res >> 1
}

/// `gen_codes`: assign canonical codes from the bit lengths in `tree`.
fn gen_codes(tree: &mut [u16], max_code: usize, bl_count: &[u16; MAX_BITS + 1]) {
    let mut next_code = [0u32; MAX_BITS + 1];
    let mut code = 0u32;
    for bits in 1..=MAX_BITS {
        code = (code + u32::from(bl_count[bits - 1])) << 1;
        next_code[bits] = code;
    }
    for n in 0..=max_code {
        let len = tree[n * 2 + 1] as usize;
        if len == 0 {
            continue;
        }
        tree[n * 2] = bi_reverse(next_code[len], len as u32) as u16;
        next_code[len] += 1;
    }
}

fn d_code(t: &StaticTables, dist: usize) -> usize {
    if dist < 256 {
        t.dist_code[dist] as usize
    } else {
        t.dist_code[256 + (dist >> 7)] as usize
    }
}

/// Which tree a `TreeDesc` describes; selects the static description.
#[derive(Clone, Copy)]
enum Tree {
    Literal,
    Distance,
    BitLength,
}

/// `StaticTreeDesc` fields for one tree.
struct StaticDesc {
    static_tree: Option<&'static [u16]>,
    extra_bits: &'static [u8],
    extra_base: usize,
    elems: usize,
    max_length: u16,
}

fn static_desc(which: Tree) -> StaticDesc {
    let t = tables();
    match which {
        Tree::Literal => StaticDesc {
            static_tree: Some(&t.ltree),
            extra_bits: &EXTRA_LBITS,
            extra_base: LITERALS + 1,
            elems: L_CODES,
            max_length: MAX_BITS as u16,
        },
        Tree::Distance => StaticDesc {
            static_tree: Some(&t.dtree),
            extra_bits: &EXTRA_DBITS,
            extra_base: 0,
            elems: D_CODES,
            max_length: MAX_BITS as u16,
        },
        // `new Array(0)`: has_stree is false.
        Tree::BitLength => StaticDesc {
            static_tree: None,
            extra_bits: &EXTRA_BLBITS,
            extra_base: 0,
            elems: BL_CODES,
            max_length: MAX_BL_BITS as u16,
        },
    }
}

/// `DeflateState`, restricted to the fields the default configuration uses.
struct State<'a> {
    input: &'a [u8],
    next_in: usize,
    out: Vec<u8>,

    window: Vec<u8>,
    prev: Vec<u16>,
    head: Vec<u16>,
    ins_h: usize,

    block_start: isize,
    match_length: usize,
    prev_match: usize,
    match_available: bool,
    strstart: usize,
    match_start: usize,
    lookahead: usize,
    prev_length: usize,
    insert: usize,

    dyn_ltree: Vec<u16>,
    dyn_dtree: Vec<u16>,
    bl_tree: Vec<u16>,
    l_max_code: usize,
    d_max_code: usize,
    bl_max_code: usize,
    bl_count: [u16; MAX_BITS + 1],
    heap: [u16; HEAP_SIZE],
    heap_len: usize,
    heap_max: usize,
    depth: [u16; HEAP_SIZE],

    /// `d_buf`: match distances (0 for a literal).
    d_buf: Vec<u16>,
    /// `l_buf`: literal bytes or match lengths - MIN_MATCH.
    l_buf: Vec<u8>,
    last_lit: usize,
    opt_len: i64,
    static_len: i64,

    bi_buf: u32,
    bi_valid: u32,
}

/// zlib-compress `data` exactly as `pako.deflate(data)` does with its default
/// options (level 6, 32 KiB window, zlib header and Adler-32 trailer).
///
/// Upstream's `deflate(text)` first converts the string to UTF-8
/// (`lib/utils/strings.js`, `string2buf`), which for a Rust `&str` is
/// `text.as_bytes()`.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut s = State {
        input: data,
        next_in: 0,
        out: Vec::with_capacity(data.len() / 2 + 64),
        window: vec![0; WINDOW_SIZE],
        prev: vec![0; W_SIZE],
        head: vec![0; HASH_SIZE],
        ins_h: 0,
        block_start: 0,
        match_length: MIN_MATCH - 1,
        prev_match: 0,
        match_available: false,
        strstart: 0,
        match_start: 0,
        lookahead: 0,
        prev_length: MIN_MATCH - 1,
        insert: 0,
        dyn_ltree: vec![0; HEAP_SIZE * 2],
        dyn_dtree: vec![0; (2 * D_CODES + 1) * 2],
        bl_tree: vec![0; (2 * BL_CODES + 1) * 2],
        l_max_code: 0,
        d_max_code: 0,
        bl_max_code: 0,
        bl_count: [0; MAX_BITS + 1],
        heap: [0; HEAP_SIZE],
        heap_len: 0,
        heap_max: 0,
        depth: [0; HEAP_SIZE],
        d_buf: vec![0; LIT_BUFSIZE],
        l_buf: vec![0; LIT_BUFSIZE],
        last_lit: 0,
        opt_len: 0,
        static_len: 0,
        bi_buf: 0,
        bi_valid: 0,
    };
    s.init_block();

    // deflate(): the zlib header (INIT_STATE).
    let mut header = (Z_DEFLATED + (((MAX_WBITS - 8) as u16) << 4)) << 8;
    header |= LEVEL_FLAGS << 6;
    header += 31 - (header % 31);
    s.put_short_msb(header);

    // deflate(): one call with Z_FINISH and the whole input.
    s.deflate_slow();

    // The trailer: Adler-32 of the input, most significant byte first.
    let adler = adler32(1, data);
    s.put_short_msb((adler >> 16) as u16);
    s.put_short_msb((adler & 0xffff) as u16);
    s.out
}

impl State<'_> {
    fn put_byte(&mut self, b: u8) {
        self.out.push(b);
    }

    /// `putShortMSB`.
    fn put_short_msb(&mut self, b: u16) {
        self.out.push((b >> 8) as u8);
        self.out.push((b & 0xff) as u8);
    }

    /// `put_short`: least significant byte first.
    fn put_short(&mut self, w: u32) {
        self.out.push((w & 0xff) as u8);
        self.out.push(((w >> 8) & 0xff) as u8);
    }

    /// `HASH` / `UPDATE_HASH`.
    fn update_hash(&self, h: usize, c: u8) -> usize {
        ((h << HASH_SHIFT) ^ c as usize) & HASH_MASK
    }

    /// `INSERT_STRING`: insert window[str .. str + 2] and return the previous
    /// head of its hash chain.
    fn insert_string(&mut self, str: usize) -> usize {
        self.ins_h = self.update_hash(self.ins_h, self.window[str + MIN_MATCH - 1]);
        let hash_head = self.head[self.ins_h];
        self.prev[str & W_MASK] = hash_head;
        self.head[self.ins_h] = str as u16;
        hash_head as usize
    }

    /// `read_buf`: copy up to `size` input bytes into the window at `start`.
    fn read_buf(&mut self, start: usize, size: usize) -> usize {
        let len = (self.input.len() - self.next_in).min(size);
        if len == 0 {
            return 0;
        }
        self.window[start..start + len]
            .copy_from_slice(&self.input[self.next_in..self.next_in + len]);
        self.next_in += len;
        len
    }

    fn avail_in(&self) -> usize {
        self.input.len() - self.next_in
    }

    /// `longest_match`.
    fn longest_match(&mut self, mut cur_match: usize) -> usize {
        let mut chain_length = MAX_CHAIN;
        let mut best_len = self.prev_length;
        let mut nice_match = NICE_MATCH;
        let limit = self.strstart.saturating_sub(MAX_DIST);
        let strend = self.strstart + MAX_MATCH;
        let win = &self.window;
        let mut scan_end1 = win[self.strstart + best_len - 1];
        let mut scan_end = win[self.strstart + best_len];

        if self.prev_length >= GOOD_MATCH {
            chain_length >>= 2;
        }
        if nice_match > self.lookahead {
            nice_match = self.lookahead;
        }

        loop {
            let m = cur_match;
            let skip = win[m + best_len] != scan_end
                || win[m + best_len - 1] != scan_end1
                || win[m] != win[self.strstart]
                || win[m + 1] != win[self.strstart + 1];
            if !skip {
                // scan[2] and match[2] are equal when the hashes are.
                let mut scan = self.strstart + 2;
                let mut mt = m + 2;
                'compare: loop {
                    for _ in 0..8 {
                        scan += 1;
                        mt += 1;
                        if win[scan] != win[mt] {
                            break 'compare;
                        }
                    }
                    if scan >= strend {
                        break;
                    }
                }
                let len = MAX_MATCH - (strend - scan);
                if len > best_len {
                    self.match_start = cur_match;
                    best_len = len;
                    if len >= nice_match {
                        break;
                    }
                    scan_end1 = win[self.strstart + best_len - 1];
                    scan_end = win[self.strstart + best_len];
                }
            }
            cur_match = self.prev[cur_match & W_MASK] as usize;
            if cur_match <= limit {
                break;
            }
            chain_length -= 1;
            if chain_length == 0 {
                break;
            }
        }
        best_len.min(self.lookahead)
    }

    /// `fill_window`.
    fn fill_window(&mut self) {
        loop {
            let mut more = WINDOW_SIZE - self.lookahead - self.strstart;

            if self.strstart >= W_SIZE + MAX_DIST {
                self.window.copy_within(W_SIZE..2 * W_SIZE, 0);
                self.match_start = self.match_start.wrapping_sub(W_SIZE);
                self.strstart -= W_SIZE;
                self.block_start -= W_SIZE as isize;
                for h in self.head.iter_mut() {
                    *h = if *h as usize >= W_SIZE {
                        *h - W_SIZE as u16
                    } else {
                        0
                    };
                }
                for p in self.prev.iter_mut() {
                    *p = if *p as usize >= W_SIZE {
                        *p - W_SIZE as u16
                    } else {
                        0
                    };
                }
                more += W_SIZE;
            }
            if self.avail_in() == 0 {
                break;
            }

            let n = self.read_buf(self.strstart + self.lookahead, more);
            self.lookahead += n;

            if self.lookahead + self.insert >= MIN_MATCH {
                let mut str = self.strstart - self.insert;
                self.ins_h = self.window[str] as usize;
                self.ins_h = self.update_hash(self.ins_h, self.window[str + 1]);
                while self.insert != 0 {
                    self.ins_h = self.update_hash(self.ins_h, self.window[str + MIN_MATCH - 1]);
                    self.prev[str & W_MASK] = self.head[self.ins_h];
                    self.head[self.ins_h] = str as u16;
                    str += 1;
                    self.insert -= 1;
                    if self.lookahead + self.insert < MIN_MATCH {
                        break;
                    }
                }
            }

            if !(self.lookahead < MIN_LOOKAHEAD && self.avail_in() != 0) {
                break;
            }
        }
    }

    /// `flush_block_only`.
    fn flush_block_only(&mut self, last: bool) {
        let buf = (self.block_start >= 0).then_some(self.block_start as usize);
        let stored_len = (self.strstart as isize - self.block_start) as usize;
        self.tr_flush_block(buf, stored_len, last);
        self.block_start = self.strstart as isize;
    }

    /// `deflate_slow` with `flush == Z_FINISH`: lazy match evaluation over
    /// the whole input, then the final block.
    fn deflate_slow(&mut self) {
        loop {
            if self.lookahead < MIN_LOOKAHEAD {
                self.fill_window();
                if self.lookahead == 0 {
                    break;
                }
            }

            let mut hash_head = 0usize;
            if self.lookahead >= MIN_MATCH {
                hash_head = self.insert_string(self.strstart);
            }

            self.prev_length = self.match_length;
            self.prev_match = self.match_start;
            self.match_length = MIN_MATCH - 1;

            if hash_head != 0
                && self.prev_length < MAX_LAZY_MATCH
                && self.strstart - hash_head <= MAX_DIST
            {
                self.match_length = self.longest_match(hash_head);
                // `match_length <= 5 && (strategy === Z_FILTERED || (match_length
                // === MIN_MATCH && strstart - match_start > TOO_FAR))`; the
                // strategy is Z_DEFAULT_STRATEGY. When longest_match found
                // nothing longer than prev_length, match_start is stale and may
                // have gone "negative" in a window slide (pako keeps it as a
                // negative number); wrapping arithmetic gives the same difference.
                if self.match_length == MIN_MATCH
                    && self.strstart.wrapping_sub(self.match_start) > TOO_FAR
                {
                    self.match_length = MIN_MATCH - 1;
                }
            }

            if self.prev_length >= MIN_MATCH && self.match_length <= self.prev_length {
                let max_insert = self.strstart + self.lookahead - MIN_MATCH;
                let bflush = self.tr_tally(
                    self.strstart - 1 - self.prev_match,
                    self.prev_length - MIN_MATCH,
                );
                self.lookahead -= self.prev_length - 1;
                self.prev_length -= 2;
                loop {
                    self.strstart += 1;
                    if self.strstart <= max_insert {
                        self.insert_string(self.strstart);
                    }
                    self.prev_length -= 1;
                    if self.prev_length == 0 {
                        break;
                    }
                }
                self.match_available = false;
                self.match_length = MIN_MATCH - 1;
                self.strstart += 1;
                if bflush {
                    self.flush_block_only(false);
                }
            } else if self.match_available {
                let bflush = self.tr_tally(0, self.window[self.strstart - 1] as usize);
                if bflush {
                    self.flush_block_only(false);
                }
                self.strstart += 1;
                self.lookahead -= 1;
            } else {
                self.match_available = true;
                self.strstart += 1;
                self.lookahead -= 1;
            }
        }
        if self.match_available {
            self.tr_tally(0, self.window[self.strstart - 1] as usize);
            self.match_available = false;
        }
        self.insert = self.strstart.min(MIN_MATCH - 1);
        self.flush_block_only(true);
    }

    // --- trees.js ----------------------------------------------------------

    /// `send_bits`.
    fn send_bits(&mut self, value: u32, length: u32) {
        if self.bi_valid > BUF_SIZE - length {
            self.bi_buf |= (value << self.bi_valid) & 0xffff;
            self.put_short(self.bi_buf);
            self.bi_buf = value >> (BUF_SIZE - self.bi_valid);
            self.bi_valid = self.bi_valid + length - BUF_SIZE;
        } else {
            self.bi_buf |= (value << self.bi_valid) & 0xffff;
            self.bi_valid += length;
        }
    }

    /// `send_code`.
    fn send_code(&mut self, c: usize, tree: &[u16]) {
        self.send_bits(u32::from(tree[c * 2]), u32::from(tree[c * 2 + 1]));
    }

    /// `bi_windup`.
    fn bi_windup(&mut self) {
        if self.bi_valid > 8 {
            self.put_short(self.bi_buf);
        } else if self.bi_valid > 0 {
            self.put_byte((self.bi_buf & 0xff) as u8);
        }
        self.bi_buf = 0;
        self.bi_valid = 0;
    }

    /// `init_block`.
    fn init_block(&mut self) {
        for n in 0..L_CODES {
            self.dyn_ltree[n * 2] = 0;
        }
        for n in 0..D_CODES {
            self.dyn_dtree[n * 2] = 0;
        }
        for n in 0..BL_CODES {
            self.bl_tree[n * 2] = 0;
        }
        self.dyn_ltree[END_BLOCK * 2] = 1;
        self.opt_len = 0;
        self.static_len = 0;
        self.last_lit = 0;
    }

    /// `_tr_tally`: record a literal (`dist == 0`, `lc` the byte) or a match
    /// (`lc` = length - MIN_MATCH). True when the block must be flushed.
    fn tr_tally(&mut self, dist: usize, lc: usize) -> bool {
        self.d_buf[self.last_lit] = dist as u16;
        self.l_buf[self.last_lit] = lc as u8;
        self.last_lit += 1;
        if dist == 0 {
            self.dyn_ltree[lc * 2] += 1;
        } else {
            let t = tables();
            let dist = dist - 1;
            self.dyn_ltree[(t.length_code[lc] as usize + LITERALS + 1) * 2] += 1;
            self.dyn_dtree[d_code(t, dist) * 2] += 1;
        }
        self.last_lit == LIT_BUFSIZE - 1
    }

    fn take_tree(&mut self, which: Tree) -> Vec<u16> {
        std::mem::take(match which {
            Tree::Literal => &mut self.dyn_ltree,
            Tree::Distance => &mut self.dyn_dtree,
            Tree::BitLength => &mut self.bl_tree,
        })
    }

    fn put_tree(&mut self, which: Tree, tree: Vec<u16>, max_code: usize) {
        match which {
            Tree::Literal => {
                self.dyn_ltree = tree;
                self.l_max_code = max_code;
            }
            Tree::Distance => {
                self.dyn_dtree = tree;
                self.d_max_code = max_code;
            }
            Tree::BitLength => {
                self.bl_tree = tree;
                self.bl_max_code = max_code;
            }
        }
    }

    /// `smaller`.
    fn smaller(tree: &[u16], n: usize, m: usize, depth: &[u16]) -> bool {
        tree[n * 2] < tree[m * 2] || (tree[n * 2] == tree[m * 2] && depth[n] <= depth[m])
    }

    /// `pqdownheap`.
    fn pqdownheap(&mut self, tree: &[u16], mut k: usize) {
        let v = self.heap[k] as usize;
        let mut j = k << 1;
        while j <= self.heap_len {
            if j < self.heap_len
                && Self::smaller(
                    tree,
                    self.heap[j + 1] as usize,
                    self.heap[j] as usize,
                    &self.depth,
                )
            {
                j += 1;
            }
            if Self::smaller(tree, v, self.heap[j] as usize, &self.depth) {
                break;
            }
            self.heap[k] = self.heap[j];
            k = j;
            j <<= 1;
        }
        self.heap[k] = v as u16;
    }

    /// `gen_bitlen`.
    fn gen_bitlen(&mut self, tree: &mut [u16], max_code: usize, desc: &StaticDesc) {
        let stree = desc.static_tree;
        let extra = desc.extra_bits;
        let base = desc.extra_base;
        let max_length = desc.max_length;
        let mut overflow = 0i32;

        self.bl_count = [0; MAX_BITS + 1];

        tree[self.heap[self.heap_max] as usize * 2 + 1] = 0;
        let mut h = self.heap_max + 1;
        while h < HEAP_SIZE {
            let n = self.heap[h] as usize;
            let mut bits = tree[tree[n * 2 + 1] as usize * 2 + 1] + 1;
            if bits > max_length {
                bits = max_length;
                overflow += 1;
            }
            tree[n * 2 + 1] = bits;
            h += 1;
            if n > max_code {
                continue;
            }
            self.bl_count[bits as usize] += 1;
            let xbits = if n >= base {
                i64::from(extra[n - base])
            } else {
                0
            };
            let f = i64::from(tree[n * 2]);
            self.opt_len += f * (i64::from(bits) + xbits);
            if let Some(stree) = stree {
                self.static_len += f * (i64::from(stree[n * 2 + 1]) + xbits);
            }
        }
        if overflow == 0 {
            return;
        }

        loop {
            let mut bits = max_length as usize - 1;
            while self.bl_count[bits] == 0 {
                bits -= 1;
            }
            self.bl_count[bits] -= 1;
            self.bl_count[bits + 1] += 2;
            self.bl_count[max_length as usize] -= 1;
            overflow -= 2;
            if overflow <= 0 {
                break;
            }
        }

        let mut h = HEAP_SIZE;
        let mut bits = max_length as usize;
        while bits != 0 {
            let mut n = self.bl_count[bits];
            while n != 0 {
                h -= 1;
                let m = self.heap[h] as usize;
                if m > max_code {
                    continue;
                }
                if tree[m * 2 + 1] as usize != bits {
                    self.opt_len +=
                        (bits as i64 - i64::from(tree[m * 2 + 1])) * i64::from(tree[m * 2]);
                    tree[m * 2 + 1] = bits as u16;
                }
                n -= 1;
            }
            bits -= 1;
        }
    }

    /// `build_tree`.
    fn build_tree(&mut self, which: Tree) {
        let desc = static_desc(which);
        let mut tree = self.take_tree(which);
        let elems = desc.elems;
        let mut max_code: isize = -1;

        self.heap_len = 0;
        self.heap_max = HEAP_SIZE;

        for n in 0..elems {
            if tree[n * 2] != 0 {
                self.heap_len += 1;
                self.heap[self.heap_len] = n as u16;
                max_code = n as isize;
                self.depth[n] = 0;
            } else {
                tree[n * 2 + 1] = 0;
            }
        }

        while self.heap_len < 2 {
            let node = if max_code < 2 {
                max_code += 1;
                max_code as usize
            } else {
                0
            };
            self.heap_len += 1;
            self.heap[self.heap_len] = node as u16;
            tree[node * 2] = 1;
            self.depth[node] = 0;
            self.opt_len -= 1;
            if let Some(stree) = desc.static_tree {
                self.static_len -= i64::from(stree[node * 2 + 1]);
            }
        }
        let max_code = max_code as usize;

        let mut n = self.heap_len >> 1;
        while n >= 1 {
            self.pqdownheap(&tree, n);
            n -= 1;
        }

        let mut node = elems;
        loop {
            // pqremove
            let n = self.heap[1] as usize;
            self.heap[1] = self.heap[self.heap_len];
            self.heap_len -= 1;
            self.pqdownheap(&tree, 1);

            let m = self.heap[1] as usize;
            self.heap_max -= 1;
            self.heap[self.heap_max] = n as u16;
            self.heap_max -= 1;
            self.heap[self.heap_max] = m as u16;

            tree[node * 2] = tree[n * 2] + tree[m * 2];
            self.depth[node] = self.depth[n].max(self.depth[m]) + 1;
            tree[n * 2 + 1] = node as u16;
            tree[m * 2 + 1] = node as u16;

            self.heap[1] = node as u16;
            node += 1;
            self.pqdownheap(&tree, 1);
            if self.heap_len < 2 {
                break;
            }
        }
        self.heap_max -= 1;
        self.heap[self.heap_max] = self.heap[1];

        self.gen_bitlen(&mut tree, max_code, &desc);
        let bl_count = self.bl_count;
        gen_codes(&mut tree, max_code, &bl_count);
        self.put_tree(which, tree, max_code);
    }

    /// `scan_tree`.
    fn scan_tree(&mut self, which: Tree, max_code: usize) {
        let mut tree = self.take_tree(which);
        let mut prevlen: i32 = -1;
        let mut nextlen = i32::from(tree[1]);
        let mut count = 0;
        let (mut max_count, mut min_count) = if nextlen == 0 { (138, 3) } else { (7, 4) };

        tree[(max_code + 1) * 2 + 1] = 0xffff; // guard
        for n in 0..=max_code {
            let curlen = nextlen;
            nextlen = i32::from(tree[(n + 1) * 2 + 1]);
            count += 1;
            if count < max_count && curlen == nextlen {
                continue;
            } else if count < min_count {
                self.bl_tree[curlen as usize * 2] += count as u16;
            } else if curlen != 0 {
                if curlen != prevlen {
                    self.bl_tree[curlen as usize * 2] += 1;
                }
                self.bl_tree[REP_3_6 * 2] += 1;
            } else if count <= 10 {
                self.bl_tree[REPZ_3_10 * 2] += 1;
            } else {
                self.bl_tree[REPZ_11_138 * 2] += 1;
            }
            count = 0;
            prevlen = curlen;
            (max_count, min_count) = if nextlen == 0 {
                (138, 3)
            } else if curlen == nextlen {
                (6, 3)
            } else {
                (7, 4)
            };
        }
        let max = match which {
            Tree::Literal => self.l_max_code,
            Tree::Distance => self.d_max_code,
            Tree::BitLength => self.bl_max_code,
        };
        self.put_tree(which, tree, max);
    }

    /// `send_tree`.
    fn send_tree(&mut self, which: Tree, max_code: usize) {
        let tree = self.take_tree(which);
        let bl_tree = std::mem::take(&mut self.bl_tree);
        let mut prevlen: i32 = -1;
        let mut nextlen = i32::from(tree[1]);
        let mut count: i32 = 0;
        let (mut max_count, mut min_count) = if nextlen == 0 { (138, 3) } else { (7, 4) };

        for n in 0..=max_code {
            let curlen = nextlen;
            nextlen = i32::from(tree[(n + 1) * 2 + 1]);
            count += 1;
            if count < max_count && curlen == nextlen {
                continue;
            } else if count < min_count {
                loop {
                    self.send_code(curlen as usize, &bl_tree);
                    count -= 1;
                    if count == 0 {
                        break;
                    }
                }
            } else if curlen != 0 {
                if curlen != prevlen {
                    self.send_code(curlen as usize, &bl_tree);
                    count -= 1;
                }
                self.send_code(REP_3_6, &bl_tree);
                self.send_bits((count - 3) as u32, 2);
            } else if count <= 10 {
                self.send_code(REPZ_3_10, &bl_tree);
                self.send_bits((count - 3) as u32, 3);
            } else {
                self.send_code(REPZ_11_138, &bl_tree);
                self.send_bits((count - 11) as u32, 7);
            }
            count = 0;
            prevlen = curlen;
            (max_count, min_count) = if nextlen == 0 {
                (138, 3)
            } else if curlen == nextlen {
                (6, 3)
            } else {
                (7, 4)
            };
        }
        self.bl_tree = bl_tree;
        let max = match which {
            Tree::Literal => self.l_max_code,
            Tree::Distance => self.d_max_code,
            Tree::BitLength => self.bl_max_code,
        };
        self.put_tree(which, tree, max);
    }

    /// `build_bl_tree`: returns the index in BL_ORDER of the last bit length
    /// code to send.
    fn build_bl_tree(&mut self) -> usize {
        self.scan_tree(Tree::Literal, self.l_max_code);
        self.scan_tree(Tree::Distance, self.d_max_code);
        self.build_tree(Tree::BitLength);
        let mut max_blindex = BL_CODES - 1;
        while max_blindex >= 3 {
            if self.bl_tree[BL_ORDER[max_blindex] as usize * 2 + 1] != 0 {
                break;
            }
            max_blindex -= 1;
        }
        self.opt_len += 3 * (max_blindex as i64 + 1) + 5 + 5 + 4;
        max_blindex
    }

    /// `send_all_trees`.
    fn send_all_trees(&mut self, lcodes: usize, dcodes: usize, blcodes: usize) {
        self.send_bits((lcodes - 257) as u32, 5);
        self.send_bits((dcodes - 1) as u32, 5);
        self.send_bits((blcodes - 4) as u32, 4);
        for &order in BL_ORDER.iter().take(blcodes) {
            let len = self.bl_tree[order as usize * 2 + 1];
            self.send_bits(u32::from(len), 3);
        }
        self.send_tree(Tree::Literal, lcodes - 1);
        self.send_tree(Tree::Distance, dcodes - 1);
    }

    /// `compress_block`.
    fn compress_block(&mut self, ltree: &[u16], dtree: &[u16]) {
        let t = tables();
        for lx in 0..self.last_lit {
            let dist = self.d_buf[lx] as usize;
            let lc = self.l_buf[lx] as usize;
            if dist == 0 {
                self.send_code(lc, ltree);
            } else {
                let code = t.length_code[lc] as usize;
                self.send_code(code + LITERALS + 1, ltree);
                let extra = u32::from(EXTRA_LBITS[code]);
                if extra != 0 {
                    self.send_bits((lc - t.base_length[code] as usize) as u32, extra);
                }
                let dist = dist - 1;
                let code = d_code(t, dist);
                self.send_code(code, dtree);
                let extra = u32::from(EXTRA_DBITS[code]);
                if extra != 0 {
                    self.send_bits((dist - t.base_dist[code] as usize) as u32, extra);
                }
            }
        }
        self.send_code(END_BLOCK, ltree);
    }

    /// `_tr_stored_block` / `copy_block`.
    fn tr_stored_block(&mut self, buf: usize, stored_len: usize, last: bool) {
        self.send_bits((STORED_BLOCK << 1) + u32::from(last), 3);
        self.bi_windup();
        self.put_short(stored_len as u32);
        self.put_short(!(stored_len as u32));
        self.out
            .extend_from_slice(&self.window[buf..buf + stored_len]);
    }

    /// `_tr_flush_block`: emit the current block as stored, static or dynamic,
    /// whichever is smallest.
    fn tr_flush_block(&mut self, buf: Option<usize>, stored_len: usize, last: bool) {
        self.build_tree(Tree::Literal);
        self.build_tree(Tree::Distance);
        let max_blindex = self.build_bl_tree();

        let mut opt_lenb = ((self.opt_len + 3 + 7) as u64 & 0xffff_ffff) >> 3;
        let static_lenb = ((self.static_len + 3 + 7) as u64 & 0xffff_ffff) >> 3;
        if static_lenb <= opt_lenb {
            opt_lenb = static_lenb;
        }

        match buf {
            Some(buf) if stored_len as u64 + 4 <= opt_lenb => {
                self.tr_stored_block(buf, stored_len, last);
            }
            _ if static_lenb == opt_lenb => {
                self.send_bits((STATIC_TREES << 1) + u32::from(last), 3);
                let t = tables();
                self.compress_block(&t.ltree, &t.dtree);
            }
            _ => {
                self.send_bits((DYN_TREES << 1) + u32::from(last), 3);
                self.send_all_trees(self.l_max_code + 1, self.d_max_code + 1, max_blindex + 1);
                let ltree = std::mem::take(&mut self.dyn_ltree);
                let dtree = std::mem::take(&mut self.dyn_dtree);
                self.compress_block(&ltree, &dtree);
                self.dyn_ltree = ltree;
                self.dyn_dtree = dtree;
            }
        }
        self.init_block();
        if last {
            self.bi_windup();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_matches_zlib() {
        // pako.deflate("") (and zlib's compress2 at level 6).
        assert_eq!(
            deflate(b""),
            [0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01]
        );
    }

    #[test]
    fn static_tables_match_rfc_1951() {
        let t = tables();
        // Literal 0 is the 8-bit code 00110000, stored bit-reversed.
        assert_eq!(
            (t.ltree[1], t.ltree[0]),
            (8, bi_reverse(0b0011_0000, 8) as u16)
        );
        // End of block (256) is the 7-bit code 0000000.
        assert_eq!((t.ltree[256 * 2 + 1], t.ltree[256 * 2]), (7, 0));
        // Literal 255 is the 9-bit code 111111111.
        assert_eq!((t.ltree[255 * 2 + 1], t.ltree[255 * 2]), (9, 0x1ff));
        assert_eq!(t.length_code[0], 0);
        assert_eq!(t.length_code[255], 28);
        assert_eq!(t.length_code[254], 27);
        assert_eq!(d_code(t, 0), 0);
        assert_eq!(d_code(t, 32767), 29);
        assert_eq!(t.base_dist[29], 24576);
    }
}
