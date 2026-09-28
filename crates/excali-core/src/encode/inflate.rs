//! zlib/gzip decompression, ported from pako 2.0.3 (`lib/inflate.js`,
//! `lib/zlib/inflate.js`, `lib/zlib/inffast.js`, `lib/zlib/inftrees.js`) so
//! that every input gives what upstream's `decode` gets from
//! `inflate(bytes, { to: "string" })` (`packages/excalidraw/data/encode.ts:138-141`):
//! the same text, the same error message, or no result.
//!
//! General-purpose inflaters agree with pako on valid streams but not on
//! corrupt ones: miniz_oxide, for one, does not check a back-reference
//! against the output produced so far and reads zeros from its fresh
//! window, and its errors are not pako's messages. So the whole decoder is
//! ported, as the compressor is (`deflate.rs`): the resumable state machine
//! `inflate()`, `inflate_fast()`, `inflate_table()`, `updatewindow()` and
//! the `Inflate.prototype.push` driver, with pako's names. What this gives:
//!
//! - `windowBits` 15 + 32: the wrapper is detected per stream, a gzip member
//!   (magic `1f 8b`) or a zlib stream. Errors carry pako's `strm.msg`, which
//!   upstream's `decode` throws: `incorrect header check`, `unknown
//!   compression method`, `invalid window size`, `unknown header flags set`,
//!   `header crc mismatch`, `invalid block type`, `invalid stored block
//!   lengths`, `too many length or distance symbols`, `invalid code lengths
//!   set`, `invalid bit length repeat`, `invalid code -- missing
//!   end-of-block`, `invalid literal/lengths set`, `invalid distances set`,
//!   `invalid literal/length code`, `invalid distance code`, `invalid
//!   distance too far back`, `incorrect data check`, `incorrect length
//!   check`; and `need dictionary` (`lib/zlib/messages.js`) for a zlib
//!   stream with FDICT.
//! - After a stream ends, if more input follows and its next byte is not 0,
//!   pako resets and inflates another stream into the same output
//!   ("Skip snyc markers if more data follows"). Trailing bytes starting
//!   with 0 are ignored. pako's `inflateReset` zeroes `wsize` but keeps the
//!   window buffer, and `updatewindow` only sizes a window when it
//!   allocates one; so in a stream that follows one whose output spanned
//!   more than one `inflate()` call, a back-reference that reaches before
//!   the current call's output is `invalid distance too far back`. The port
//!   keeps that.
//! - Input that ends before a stream does is not an error in pako: no
//!   result is produced and `inflate` returns `undefined`. Here that is
//!   [`InflateError::Incomplete`].
//! - Output goes through a 64 KiB buffer (`chunkSize`), one `inflate()` call
//!   per buffer. With `to: "string"`, each full buffer is converted by
//!   pako's own UTF-8 decoder (`lib/utils/strings.js`, `buf2string`), cut at
//!   `utf8border` so a sequence is not split, and the cut-off tail is moved
//!   to the front of the buffer for the next call. That decoder does not
//!   validate continuation bytes, does not strip a BOM, and drops an
//!   incomplete sequence at the very end of the output.
//!   [`inflate_to_utf16`] returns its UTF-16 code units exactly;
//!   [`inflate_to_string`] maps a lone surrogate (which a JS string can hold
//!   and a Rust string cannot) to U+FFFD.
//!
//! JavaScript arithmetic is reproduced where it matters: the bit buffer
//! `hold` is used as an unsigned 32-bit value, which is what pako's `>>>`
//! and `&` see, and a read past the end of a typed array (which gives
//! `undefined`, i.e. 0 in bit operations) reads 0.

use std::sync::OnceLock;

use super::checksum::{adler32, crc32};
use super::inftrees::{inflate_table, CodeType, ENOUGH_DISTS, ENOUGH_LENS};

/// Why [`inflate`] produced no output. The `Display` text of every variant
/// but [`InflateError::Incomplete`] is pako's message, which upstream's
/// `decode` throws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InflateError {
    /// The input ended before the stream did. pako returns `undefined`.
    Incomplete,
    /// `incorrect header check`: neither a gzip magic nor a valid zlib header.
    IncorrectHeaderCheck,
    /// `unknown compression method`: CM is not 8 (deflate).
    UnknownCompressionMethod,
    /// `invalid window size`: the zlib header asks for more than 32 KiB.
    InvalidWindowSize,
    /// `need dictionary`: the zlib header requires a preset dictionary.
    NeedDictionary,
    /// `unknown header flags set`: reserved gzip FLG bits are set.
    UnknownHeaderFlags,
    /// `header crc mismatch`: the gzip FHCRC does not match.
    HeaderCrcMismatch,
    /// `invalid block type`: BTYPE is 3.
    InvalidBlockType,
    /// `invalid stored block lengths`: NLEN is not the complement of LEN.
    InvalidStoredBlockLengths,
    /// `too many length or distance symbols`: HLIT > 29 or HDIST > 29.
    TooManyLengthOrDistanceSymbols,
    /// `invalid code lengths set`: the code length code is over-subscribed
    /// or incomplete.
    InvalidCodeLengthsSet,
    /// `invalid bit length repeat`: a repeat of the previous length with no
    /// previous length, or a repeat past the last symbol.
    InvalidBitLengthRepeat,
    /// `invalid code -- missing end-of-block`: symbol 256 has no code.
    MissingEndOfBlock,
    /// `invalid literal/lengths set`: the literal/length code is
    /// over-subscribed or incomplete.
    InvalidLiteralLengthsSet,
    /// `invalid distances set`: the distance code is over-subscribed or
    /// incomplete.
    InvalidDistancesSet,
    /// `invalid literal/length code`: symbol 286 or 287, or no code.
    InvalidLiteralLengthCode,
    /// `invalid distance code`: distance symbol 30 or 31, or no code.
    InvalidDistanceCode,
    /// `invalid distance too far back`: a back-reference before the start of
    /// the output (or of the window pako keeps).
    InvalidDistanceTooFarBack,
    /// `incorrect data check`: the Adler-32 or CRC-32 trailer does not match.
    IncorrectDataCheck,
    /// `incorrect length check`: the gzip ISIZE trailer does not match.
    IncorrectLengthCheck,
}

impl std::fmt::Display for InflateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Incomplete => "incomplete compressed data",
            Self::IncorrectHeaderCheck => "incorrect header check",
            Self::UnknownCompressionMethod => "unknown compression method",
            Self::InvalidWindowSize => "invalid window size",
            Self::NeedDictionary => "need dictionary",
            Self::UnknownHeaderFlags => "unknown header flags set",
            Self::HeaderCrcMismatch => "header crc mismatch",
            Self::InvalidBlockType => "invalid block type",
            Self::InvalidStoredBlockLengths => "invalid stored block lengths",
            Self::TooManyLengthOrDistanceSymbols => "too many length or distance symbols",
            Self::InvalidCodeLengthsSet => "invalid code lengths set",
            Self::InvalidBitLengthRepeat => "invalid bit length repeat",
            Self::MissingEndOfBlock => "invalid code -- missing end-of-block",
            Self::InvalidLiteralLengthsSet => "invalid literal/lengths set",
            Self::InvalidDistancesSet => "invalid distances set",
            Self::InvalidLiteralLengthCode => "invalid literal/length code",
            Self::InvalidDistanceCode => "invalid distance code",
            Self::InvalidDistanceTooFarBack => "invalid distance too far back",
            Self::IncorrectDataCheck => "incorrect data check",
            Self::IncorrectLengthCheck => "incorrect length check",
        })
    }
}

impl std::error::Error for InflateError {}

/// pako's `Inflate` output buffer size (`chunkSize`).
const CHUNK_SIZE: usize = 64 * 1024;

/// `inflate()` states (`lib/zlib/inflate.js`), in pako's order: the code
/// compares them (`mode < CHECK`, `mode < BAD`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Mode {
    Head,
    Flags,
    Time,
    Os,
    Exlen,
    Extra,
    Name,
    Comment,
    Hcrc,
    DictId,
    Dict,
    Type,
    TypeDo,
    Stored,
    Copy_,
    Copy,
    Table,
    LenLens,
    CodeLens,
    Len_,
    Len,
    LenExt,
    Dist,
    DistExt,
    Match,
    Lit,
    Check,
    Length,
    Done,
    Bad,
}

/// `inflate()` return codes that the driver distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ret {
    /// `Z_OK`
    Ok,
    /// `Z_STREAM_END`
    StreamEnd,
    /// `Z_NEED_DICT`
    NeedDict,
    /// `Z_DATA_ERROR`
    DataError,
    /// `Z_BUF_ERROR`: no progress was possible.
    BufError,
}

/// Permutation of code length code lengths.
const ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// The fixed Huffman tables (`fixedtables`, built once).
struct FixedTables {
    len: [u32; 512],
    dist: [u32; 32],
}

fn fixed_tables() -> &'static FixedTables {
    static T: OnceLock<FixedTables> = OnceLock::new();
    T.get_or_init(|| {
        let mut lens = [0u16; 320];
        let mut work = [0u16; 288];
        let mut t = FixedTables {
            len: [0; 512],
            dist: [0; 32],
        };
        lens[..144].fill(8);
        lens[144..256].fill(9);
        lens[256..280].fill(7);
        lens[280..288].fill(8);
        let mut bits = 9;
        inflate_table(CodeType::Lens, &lens, 288, &mut t.len, &mut work, &mut bits);
        lens[..32].fill(5);
        let mut bits = 5;
        inflate_table(
            CodeType::Dists,
            &lens,
            32,
            &mut t.dist,
            &mut work,
            &mut bits,
        );
        t
    })
}

/// `InflateState` for `windowBits` 47 (auto-detect, 32 KiB window).
struct State {
    mode: Mode,
    /// Processing the last block.
    last: bool,
    /// 0 for zlib; for gzip the method and FLG bytes (`hold` after FLAGS).
    flags: u32,
    /// Largest distance allowed.
    dmax: u32,
    /// Running Adler-32 or CRC-32 (pako keeps it as an int32; only its low
    /// 32 bits are ever compared).
    check: u32,
    /// Output count of this stream, modulo 2^32.
    total: u32,
    wbits: u32,
    wsize: usize,
    whave: usize,
    wnext: usize,
    window: Option<Vec<u8>>,
    hold: u32,
    bits: u32,
    /// Literal byte, match length, stored length or gzip EXTRA length.
    length: u32,
    offset: u32,
    extra: u32,
    /// `lencode` / `distcode` point at the fixed tables (else at `lendyn`,
    /// `distdyn`).
    len_fixed: bool,
    dist_fixed: bool,
    lenbits: u32,
    distbits: u32,
    ncode: usize,
    nlen: usize,
    ndist: usize,
    have: usize,
    lens: [u16; 320],
    work: [u16; 288],
    lendyn: Vec<u32>,
    distdyn: Vec<u32>,
}

impl State {
    /// `inflateInit2(strm, 47)`: `wrap` 3 (zlib or gzip), `wbits` 15.
    fn new() -> Self {
        let mut s = State {
            mode: Mode::Head,
            last: false,
            flags: 0,
            dmax: 32768,
            check: 0,
            total: 0,
            wbits: 15,
            wsize: 0,
            whave: 0,
            wnext: 0,
            window: None,
            hold: 0,
            bits: 0,
            length: 0,
            offset: 0,
            extra: 0,
            len_fixed: false,
            dist_fixed: false,
            lenbits: 0,
            distbits: 0,
            ncode: 0,
            nlen: 0,
            ndist: 0,
            have: 0,
            lens: [0; 320],
            work: [0; 288],
            lendyn: Vec::new(),
            distdyn: Vec::new(),
        };
        s.reset();
        s
    }

    /// `inflateReset`: clears `wsize`, `whave`, `wnext` (the window buffer
    /// itself is kept), then `inflateResetKeep`.
    fn reset(&mut self) {
        self.wsize = 0;
        self.whave = 0;
        self.wnext = 0;
        self.total = 0;
        self.mode = Mode::Head;
        self.last = false;
        self.dmax = 32768;
        self.hold = 0;
        self.bits = 0;
        self.lendyn = vec![0; ENOUGH_LENS];
        self.distdyn = vec![0; ENOUGH_DISTS];
        self.len_fixed = false;
        self.dist_fixed = false;
    }

    /// `state.lencode[i]`; out of range reads 0, as `undefined` does in
    /// pako's bit operations.
    fn lcode(&self, i: u32) -> u32 {
        let t: &[u32] = if self.len_fixed {
            &fixed_tables().len
        } else {
            &self.lendyn
        };
        t.get(i as usize).copied().unwrap_or(0)
    }

    /// `state.distcode[i]`.
    fn dcode(&self, i: u32) -> u32 {
        let t: &[u32] = if self.dist_fixed {
            &fixed_tables().dist
        } else {
            &self.distdyn
        };
        t.get(i as usize).copied().unwrap_or(0)
    }

    /// Update the Adler-32 (zlib) or CRC-32 (gzip) of the output.
    fn update_check(&mut self, data: &[u8]) {
        self.check = if self.flags != 0 {
            crc32(self.check, data)
        } else {
            adler32(self.check, data)
        };
    }
}

/// `ZStream`: the input, the current output buffer, and the state.
struct Strm<'a> {
    input: &'a [u8],
    next_in: usize,
    avail_in: usize,
    output: Vec<u8>,
    next_out: usize,
    avail_out: usize,
    msg: Option<InflateError>,
    state: State,
}

/// `hold >>> 24` etc.: the parts of a table entry.
fn entry_bits(here: u32) -> u32 {
    here >> 24
}
fn entry_op(here: u32) -> u32 {
    (here >> 16) & 0xff
}
fn entry_val(here: u32) -> u32 {
    here & 0xffff
}

/// `zswap32`: byte-swap.
fn zswap32(q: u32) -> u32 {
    q.swap_bytes()
}

/// `updatewindow(strm, src, end, copy)`: keep the last `wsize` bytes of
/// output in the circular window. The window is sized only when it is
/// allocated.
fn updatewindow(state: &mut State, src: &[u8], end: usize, mut copy: usize) {
    if state.window.is_none() {
        state.wsize = 1 << state.wbits;
        state.wnext = 0;
        state.whave = 0;
        state.window = Some(vec![0; state.wsize]);
    }
    let wsize = state.wsize;
    let window = state.window.as_mut().expect("allocated above");
    if copy >= wsize {
        window[..wsize].copy_from_slice(&src[end - wsize..end]);
        state.wnext = 0;
        state.whave = wsize;
    } else {
        let wnext = state.wnext;
        let dist = (wsize - wnext).min(copy);
        window[wnext..wnext + dist].copy_from_slice(&src[end - copy..end - copy + dist]);
        copy -= dist;
        if copy != 0 {
            window[..copy].copy_from_slice(&src[end - copy..end]);
            state.wnext = copy;
            state.whave = wsize;
        } else {
            state.wnext += dist;
            if state.wnext == wsize {
                state.wnext = 0;
            }
            if state.whave < wsize {
                state.whave += dist;
            }
        }
    }
}

/// `inflate_fast(strm, start)` (`lib/zlib/inffast.js`): decode literals and
/// matches while at least 6 input bytes and 258 output bytes are
/// available. Entry: `state.mode == LEN`; exit: `LEN`, `TYPE` (end of
/// block) or `BAD`.
fn inflate_fast(strm: &mut Strm<'_>, start: usize) {
    let input = strm.input;
    let state = &mut strm.state;
    let output = &mut strm.output;

    let mut in_ = strm.next_in;
    let last = in_ + (strm.avail_in - 5);
    let mut out = strm.next_out;
    let beg = out - (start - strm.avail_out);
    let end = out + (strm.avail_out - 257);
    let dmax = state.dmax as usize;
    let wsize = state.wsize;
    let whave = state.whave;
    let wnext = state.wnext;
    let mut hold = state.hold;
    let mut bits = state.bits;
    let lmask = (1u32 << state.lenbits) - 1;
    let dmask = (1u32 << state.distbits) - 1;

    let byte = |i: usize| u32::from(input.get(i).copied().unwrap_or(0));

    'top: loop {
        if bits < 15 {
            hold = hold.wrapping_add(byte(in_) << bits);
            in_ += 1;
            bits += 8;
            hold = hold.wrapping_add(byte(in_) << bits);
            in_ += 1;
            bits += 8;
        }
        let mut here = state.lcode(hold & lmask);

        // dolen
        loop {
            let op = entry_bits(here);
            hold >>= op;
            bits -= op;
            let op = entry_op(here);
            if op == 0 {
                // Literal.
                output[out] = entry_val(here) as u8;
                out += 1;
            } else if op & 16 != 0 {
                // Length base.
                let mut len = entry_val(here) as usize;
                let op = op & 15; // number of extra bits
                if op != 0 {
                    if bits < op {
                        hold = hold.wrapping_add(byte(in_) << bits);
                        in_ += 1;
                        bits += 8;
                    }
                    len += (hold & ((1 << op) - 1)) as usize;
                    hold >>= op;
                    bits -= op;
                }
                if bits < 15 {
                    hold = hold.wrapping_add(byte(in_) << bits);
                    in_ += 1;
                    bits += 8;
                    hold = hold.wrapping_add(byte(in_) << bits);
                    in_ += 1;
                    bits += 8;
                }
                here = state.dcode(hold & dmask);

                // dodist
                loop {
                    let op = entry_bits(here);
                    hold >>= op;
                    bits -= op;
                    let op = entry_op(here);
                    if op & 16 != 0 {
                        // Distance base.
                        let mut dist = entry_val(here) as usize;
                        let op = op & 15; // number of extra bits
                        if bits < op {
                            hold = hold.wrapping_add(byte(in_) << bits);
                            in_ += 1;
                            bits += 8;
                            if bits < op {
                                hold = hold.wrapping_add(byte(in_) << bits);
                                in_ += 1;
                                bits += 8;
                            }
                        }
                        dist += (hold & ((1 << op) - 1)) as usize;
                        if dist > dmax {
                            strm.msg = Some(InflateError::InvalidDistanceTooFarBack);
                            state.mode = Mode::Bad;
                            break 'top;
                        }
                        hold >>= op;
                        bits -= op;
                        let produced = out - beg; // max distance in output
                        if dist > produced {
                            // Copy from the window.
                            let mut op = dist - produced; // distance back in window
                            if op > whave {
                                // state.sane is always 1.
                                strm.msg = Some(InflateError::InvalidDistanceTooFarBack);
                                state.mode = Mode::Bad;
                                break 'top;
                            }
                            let window =
                                state.window.as_deref().expect("whave > 0 implies a window");
                            let mut from;
                            let mut from_output = false;
                            if wnext == 0 {
                                // Very common case.
                                from = wsize - op;
                                if op < len {
                                    // Some from the window.
                                    len -= op;
                                    while op > 0 {
                                        output[out] = window[from];
                                        out += 1;
                                        from += 1;
                                        op -= 1;
                                    }
                                    from = out - dist; // rest from output
                                    from_output = true;
                                }
                            } else if wnext < op {
                                // Wrap around the window.
                                from = wsize + wnext - op;
                                op -= wnext;
                                if op < len {
                                    // Some from the end of the window.
                                    len -= op;
                                    while op > 0 {
                                        output[out] = window[from];
                                        out += 1;
                                        from += 1;
                                        op -= 1;
                                    }
                                    from = 0;
                                    if wnext < len {
                                        // Some from the start of the window.
                                        op = wnext;
                                        len -= op;
                                        while op > 0 {
                                            output[out] = window[from];
                                            out += 1;
                                            from += 1;
                                            op -= 1;
                                        }
                                        from = out - dist; // rest from output
                                        from_output = true;
                                    }
                                }
                            } else {
                                // Contiguous in the window.
                                from = wnext - op;
                                if op < len {
                                    len -= op;
                                    while op > 0 {
                                        output[out] = window[from];
                                        out += 1;
                                        from += 1;
                                        op -= 1;
                                    }
                                    from = out - dist; // rest from output
                                    from_output = true;
                                }
                            }
                            while len > 0 {
                                output[out] = if from_output {
                                    output[from]
                                } else {
                                    window[from]
                                };
                                out += 1;
                                from += 1;
                                len -= 1;
                            }
                        } else {
                            // Copy direct from output (minimum length is three).
                            let mut from = out - dist;
                            while len > 0 {
                                output[out] = output[from];
                                out += 1;
                                from += 1;
                                len -= 1;
                            }
                        }
                    } else if op & 64 == 0 {
                        // Second-level distance code.
                        here = state.dcode(entry_val(here) + (hold & ((1 << op) - 1)));
                        continue;
                    } else {
                        strm.msg = Some(InflateError::InvalidDistanceCode);
                        state.mode = Mode::Bad;
                        break 'top;
                    }
                    break;
                }
            } else if op & 64 == 0 {
                // Second-level length code.
                here = state.lcode(entry_val(here) + (hold & ((1 << op) - 1)));
                continue;
            } else if op & 32 != 0 {
                // End of block.
                state.mode = Mode::Type;
                break 'top;
            } else {
                strm.msg = Some(InflateError::InvalidLiteralLengthCode);
                state.mode = Mode::Bad;
                break 'top;
            }
            break;
        }
        if !(in_ < last && out < end) {
            break;
        }
    }

    // Return unused bytes (on entry, bits < 8, so in_ won't go too far back).
    let len = bits >> 3;
    in_ -= len as usize;
    bits -= len << 3;
    hold &= (1u32 << bits) - 1;

    strm.next_in = in_;
    strm.next_out = out;
    strm.avail_in = (last + 5).saturating_sub(in_);
    strm.avail_out = (end + 257).saturating_sub(out);
    state.hold = hold;
    state.bits = bits;
}

/// `inflate(strm, Z_NO_FLUSH)` (`lib/zlib/inflate.js`): run the state
/// machine until the input or the output buffer is exhausted, the stream
/// ends, or an error.
fn inflate_call(strm: &mut Strm<'_>) -> Ret {
    let input = strm.input;
    if strm.state.mode == Mode::Type {
        strm.state.mode = Mode::TypeDo; // skip check
    }
    let mut put = strm.next_out;
    let mut left = strm.avail_out;
    let mut next = strm.next_in;
    let mut have = strm.avail_in;
    let mut hold = strm.state.hold;
    let mut bits = strm.state.bits;
    let in0 = have;
    let mut out0 = left;
    let mut ret = Ret::Ok;

    'leave: loop {
        // Labels in macro_rules are hygienic: these are defined inside the
        // loop so that `'leave` resolves.
        // NEEDBITS: pull one byte into the bit buffer, or leave for more input.
        macro_rules! pull_byte {
            () => {{
                if have == 0 {
                    break 'leave;
                }
                have -= 1;
                hold = hold.wrapping_add(u32::from(input[next]) << bits);
                next += 1;
                bits += 8;
            }};
        }
        macro_rules! need_bits {
            ($n:expr) => {
                while bits < $n {
                    pull_byte!();
                }
            };
        }
        macro_rules! bad {
            ($e:expr) => {{
                strm.msg = Some($e);
                strm.state.mode = Mode::Bad;
                continue 'leave;
            }};
        }

        let state = &mut strm.state;
        match state.mode {
            Mode::Head => {
                // state.wrap is 3: zlib or gzip.
                need_bits!(16);
                if hold == 0x8b1f {
                    // gzip header.
                    state.check = crc32(0, &[hold as u8, (hold >> 8) as u8]);
                    hold = 0;
                    bits = 0;
                    state.mode = Mode::Flags;
                    continue;
                }
                state.flags = 0; // expect zlib header
                if !(((hold & 0xff) << 8) + (hold >> 8)).is_multiple_of(31) {
                    bad!(InflateError::IncorrectHeaderCheck);
                }
                if hold & 0x0f != 8 {
                    bad!(InflateError::UnknownCompressionMethod);
                }
                hold >>= 4;
                bits -= 4;
                let len = (hold & 0x0f) + 8;
                // state.wbits is 15, never 0.
                if len > state.wbits {
                    bad!(InflateError::InvalidWindowSize);
                }
                state.dmax = 1 << state.wbits;
                state.check = 1; // adler32(0, Z_NULL, 0)
                state.mode = if hold & 0x200 != 0 {
                    Mode::DictId
                } else {
                    Mode::Type
                };
                hold = 0;
                bits = 0;
            }
            Mode::Flags => {
                need_bits!(16);
                state.flags = hold;
                if state.flags & 0xff != 8 {
                    bad!(InflateError::UnknownCompressionMethod);
                }
                if state.flags & 0xe000 != 0 {
                    bad!(InflateError::UnknownHeaderFlags);
                }
                if state.flags & 0x0200 != 0 {
                    state.check = crc32(state.check, &(hold as u16).to_le_bytes());
                }
                hold = 0;
                bits = 0;
                state.mode = Mode::Time;
            }
            Mode::Time => {
                need_bits!(32);
                if state.flags & 0x0200 != 0 {
                    state.check = crc32(state.check, &hold.to_le_bytes());
                }
                hold = 0;
                bits = 0;
                state.mode = Mode::Os;
            }
            Mode::Os => {
                need_bits!(16);
                if state.flags & 0x0200 != 0 {
                    state.check = crc32(state.check, &(hold as u16).to_le_bytes());
                }
                hold = 0;
                bits = 0;
                state.mode = Mode::Exlen;
            }
            Mode::Exlen => {
                if state.flags & 0x0400 != 0 {
                    need_bits!(16);
                    state.length = hold;
                    if state.flags & 0x0200 != 0 {
                        state.check = crc32(state.check, &(hold as u16).to_le_bytes());
                    }
                    hold = 0;
                    bits = 0;
                }
                state.mode = Mode::Extra;
            }
            Mode::Extra => {
                if state.flags & 0x0400 != 0 {
                    let copy = (state.length as usize).min(have);
                    if copy != 0 {
                        if state.flags & 0x0200 != 0 {
                            state.check = crc32(state.check, &input[next..next + copy]);
                        }
                        have -= copy;
                        next += copy;
                        state.length -= copy as u32;
                    }
                    if state.length != 0 {
                        break 'leave;
                    }
                }
                state.length = 0;
                state.mode = Mode::Name;
            }
            Mode::Name | Mode::Comment => {
                let bit = if state.mode == Mode::Name {
                    0x0800
                } else {
                    0x1000
                };
                if state.flags & bit != 0 {
                    if have == 0 {
                        break 'leave;
                    }
                    // Up to and including a zero byte, or all the input.
                    let mut copy = 0;
                    let mut len;
                    loop {
                        len = input[next + copy];
                        copy += 1;
                        if len == 0 || copy >= have {
                            break;
                        }
                    }
                    if state.flags & 0x0200 != 0 {
                        state.check = crc32(state.check, &input[next..next + copy]);
                    }
                    have -= copy;
                    next += copy;
                    if len != 0 {
                        break 'leave;
                    }
                }
                state.length = 0;
                state.mode = if state.mode == Mode::Name {
                    Mode::Comment
                } else {
                    Mode::Hcrc
                };
            }
            Mode::Hcrc => {
                if state.flags & 0x0200 != 0 {
                    need_bits!(16);
                    if hold != state.check & 0xffff {
                        bad!(InflateError::HeaderCrcMismatch);
                    }
                    hold = 0;
                    bits = 0;
                }
                state.check = 0;
                state.mode = Mode::Type;
            }
            Mode::DictId => {
                need_bits!(32);
                state.check = zswap32(hold);
                hold = 0;
                bits = 0;
                state.mode = Mode::Dict;
            }
            Mode::Dict => {
                // No dictionary is ever set (state.havedict is 0).
                strm.next_out = put;
                strm.avail_out = left;
                strm.next_in = next;
                strm.avail_in = have;
                strm.state.hold = hold;
                strm.state.bits = bits;
                return Ret::NeedDict;
            }
            Mode::Type | Mode::TypeDo => {
                // Z_NO_FLUSH: TYPE falls through to TYPEDO.
                if state.last {
                    hold >>= bits & 7;
                    bits -= bits & 7;
                    state.mode = Mode::Check;
                    continue;
                }
                need_bits!(3);
                state.last = hold & 0x01 != 0;
                hold >>= 1;
                bits -= 1;
                match hold & 0x03 {
                    0 => state.mode = Mode::Stored,
                    1 => {
                        // Fixed block.
                        state.len_fixed = true;
                        state.lenbits = 9;
                        state.dist_fixed = true;
                        state.distbits = 5;
                        state.mode = Mode::Len_;
                    }
                    2 => state.mode = Mode::Table,
                    _ => {
                        strm.msg = Some(InflateError::InvalidBlockType);
                        state.mode = Mode::Bad;
                    }
                }
                hold >>= 2;
                bits -= 2;
            }
            Mode::Stored => {
                // Go to a byte boundary.
                hold >>= bits & 7;
                bits -= bits & 7;
                need_bits!(32);
                if hold & 0xffff != (hold >> 16) ^ 0xffff {
                    bad!(InflateError::InvalidStoredBlockLengths);
                }
                state.length = hold & 0xffff;
                hold = 0;
                bits = 0;
                state.mode = Mode::Copy_;
            }
            Mode::Copy_ | Mode::Copy => {
                state.mode = Mode::Copy;
                let copy = state.length as usize;
                if copy != 0 {
                    let copy = copy.min(have).min(left);
                    if copy == 0 {
                        break 'leave;
                    }
                    strm.output[put..put + copy].copy_from_slice(&input[next..next + copy]);
                    have -= copy;
                    next += copy;
                    left -= copy;
                    put += copy;
                    strm.state.length -= copy as u32;
                    continue;
                }
                state.mode = Mode::Type;
            }
            Mode::Table => {
                need_bits!(14);
                state.nlen = (hold & 0x1f) as usize + 257;
                hold >>= 5;
                bits -= 5;
                state.ndist = (hold & 0x1f) as usize + 1;
                hold >>= 5;
                bits -= 5;
                state.ncode = (hold & 0x0f) as usize + 4;
                hold >>= 4;
                bits -= 4;
                if state.nlen > 286 || state.ndist > 30 {
                    bad!(InflateError::TooManyLengthOrDistanceSymbols);
                }
                state.have = 0;
                state.mode = Mode::LenLens;
            }
            Mode::LenLens => {
                while strm.state.have < strm.state.ncode {
                    need_bits!(3);
                    let state = &mut strm.state;
                    state.lens[ORDER[state.have]] = (hold & 0x07) as u16;
                    state.have += 1;
                    hold >>= 3;
                    bits -= 3;
                }
                let state = &mut strm.state;
                while state.have < 19 {
                    state.lens[ORDER[state.have]] = 0;
                    state.have += 1;
                }
                state.len_fixed = false;
                state.lenbits = 7;
                let ret = inflate_table(
                    CodeType::Codes,
                    &state.lens,
                    19,
                    &mut state.lendyn,
                    &mut state.work,
                    &mut state.lenbits,
                );
                if ret != 0 {
                    bad!(InflateError::InvalidCodeLengthsSet);
                }
                state.have = 0;
                state.mode = Mode::CodeLens;
            }
            Mode::CodeLens => {
                while strm.state.have < strm.state.nlen + strm.state.ndist {
                    let mut here;
                    loop {
                        here = strm.state.lcode(hold & ((1 << strm.state.lenbits) - 1));
                        if entry_bits(here) <= bits {
                            break;
                        }
                        pull_byte!();
                    }
                    let here_bits = entry_bits(here);
                    let here_val = entry_val(here);
                    if here_val < 16 {
                        hold >>= here_bits;
                        bits -= here_bits;
                        let state = &mut strm.state;
                        state.lens[state.have] = here_val as u16;
                        state.have += 1;
                    } else {
                        let len;
                        let copy;
                        if here_val == 16 {
                            need_bits!(here_bits + 2);
                            hold >>= here_bits;
                            bits -= here_bits;
                            if strm.state.have == 0 {
                                bad!(InflateError::InvalidBitLengthRepeat);
                            }
                            len = strm.state.lens[strm.state.have - 1];
                            copy = 3 + (hold & 0x03) as usize;
                            hold >>= 2;
                            bits -= 2;
                        } else if here_val == 17 {
                            need_bits!(here_bits + 3);
                            hold >>= here_bits;
                            bits -= here_bits;
                            len = 0;
                            copy = 3 + (hold & 0x07) as usize;
                            hold >>= 3;
                            bits -= 3;
                        } else {
                            need_bits!(here_bits + 7);
                            hold >>= here_bits;
                            bits -= here_bits;
                            len = 0;
                            copy = 11 + (hold & 0x7f) as usize;
                            hold >>= 7;
                            bits -= 7;
                        }
                        let state = &mut strm.state;
                        if state.have + copy > state.nlen + state.ndist {
                            bad!(InflateError::InvalidBitLengthRepeat);
                        }
                        state.lens[state.have..state.have + copy].fill(len);
                        state.have += copy;
                    }
                }
                let state = &mut strm.state;
                // Check for an end-of-block code (better have one).
                if state.lens[256] == 0 {
                    bad!(InflateError::MissingEndOfBlock);
                }
                state.lenbits = 9;
                let ret = inflate_table(
                    CodeType::Lens,
                    &state.lens,
                    state.nlen,
                    &mut state.lendyn,
                    &mut state.work,
                    &mut state.lenbits,
                );
                if ret != 0 {
                    bad!(InflateError::InvalidLiteralLengthsSet);
                }
                state.distbits = 6;
                state.dist_fixed = false;
                let ret = inflate_table(
                    CodeType::Dists,
                    &state.lens[state.nlen..],
                    state.ndist,
                    &mut state.distdyn,
                    &mut state.work,
                    &mut state.distbits,
                );
                if ret != 0 {
                    bad!(InflateError::InvalidDistancesSet);
                }
                state.mode = Mode::Len_;
            }
            Mode::Len_ | Mode::Len => {
                state.mode = Mode::Len;
                if have >= 6 && left >= 258 {
                    strm.next_out = put;
                    strm.avail_out = left;
                    strm.next_in = next;
                    strm.avail_in = have;
                    strm.state.hold = hold;
                    strm.state.bits = bits;
                    inflate_fast(strm, out0);
                    put = strm.next_out;
                    left = strm.avail_out;
                    next = strm.next_in;
                    have = strm.avail_in;
                    hold = strm.state.hold;
                    bits = strm.state.bits;
                    continue;
                }
                let mut here;
                loop {
                    here = strm.state.lcode(hold & ((1 << strm.state.lenbits) - 1));
                    if entry_bits(here) <= bits {
                        break;
                    }
                    pull_byte!();
                }
                if entry_op(here) != 0 && entry_op(here) & 0xf0 == 0 {
                    // Sub-table.
                    let (last_bits, last_op, last_val) =
                        (entry_bits(here), entry_op(here), entry_val(here));
                    loop {
                        here = strm.state.lcode(
                            last_val + ((hold & ((1 << (last_bits + last_op)) - 1)) >> last_bits),
                        );
                        if last_bits + entry_bits(here) <= bits {
                            break;
                        }
                        pull_byte!();
                    }
                    hold >>= last_bits;
                    bits -= last_bits;
                }
                let here_op = entry_op(here);
                hold >>= entry_bits(here);
                bits -= entry_bits(here);
                let state = &mut strm.state;
                state.length = entry_val(here);
                if here_op == 0 {
                    state.mode = Mode::Lit;
                    continue;
                }
                if here_op & 32 != 0 {
                    state.mode = Mode::Type;
                    continue;
                }
                if here_op & 64 != 0 {
                    bad!(InflateError::InvalidLiteralLengthCode);
                }
                state.extra = here_op & 15;
                state.mode = Mode::LenExt;
            }
            Mode::LenExt => {
                if state.extra != 0 {
                    need_bits!(strm.state.extra);
                    let state = &mut strm.state;
                    state.length += hold & ((1 << state.extra) - 1);
                    hold >>= state.extra;
                    bits -= state.extra;
                }
                strm.state.mode = Mode::Dist;
            }
            Mode::Dist => {
                let mut here;
                loop {
                    here = strm.state.dcode(hold & ((1 << strm.state.distbits) - 1));
                    if entry_bits(here) <= bits {
                        break;
                    }
                    pull_byte!();
                }
                if entry_op(here) & 0xf0 == 0 {
                    // Sub-table.
                    let (last_bits, last_op, last_val) =
                        (entry_bits(here), entry_op(here), entry_val(here));
                    loop {
                        here = strm.state.dcode(
                            last_val + ((hold & ((1 << (last_bits + last_op)) - 1)) >> last_bits),
                        );
                        if last_bits + entry_bits(here) <= bits {
                            break;
                        }
                        pull_byte!();
                    }
                    hold >>= last_bits;
                    bits -= last_bits;
                }
                let here_op = entry_op(here);
                hold >>= entry_bits(here);
                bits -= entry_bits(here);
                if here_op & 64 != 0 {
                    bad!(InflateError::InvalidDistanceCode);
                }
                let state = &mut strm.state;
                state.offset = entry_val(here);
                state.extra = here_op & 15;
                state.mode = Mode::DistExt;
            }
            Mode::DistExt => {
                if state.extra != 0 {
                    need_bits!(strm.state.extra);
                    let state = &mut strm.state;
                    state.offset += hold & ((1 << state.extra) - 1);
                    hold >>= state.extra;
                    bits -= state.extra;
                }
                let state = &mut strm.state;
                if state.offset > state.dmax {
                    bad!(InflateError::InvalidDistanceTooFarBack);
                }
                state.mode = Mode::Match;
            }
            Mode::Match => {
                if left == 0 {
                    break 'leave;
                }
                let produced = out0 - left;
                let offset = state.offset as usize;
                let copy;
                if offset > produced {
                    // Copy from the window.
                    let back = offset - produced;
                    if back > state.whave {
                        // state.sane is always 1.
                        bad!(InflateError::InvalidDistanceTooFarBack);
                    }
                    let from = if back > state.wnext {
                        state.wsize - (back - state.wnext)
                    } else {
                        state.wnext - back
                    };
                    let n = if back > state.wnext {
                        back - state.wnext
                    } else {
                        back
                    };
                    copy = n.min(state.length as usize).min(left);
                    let window = state.window.as_deref().expect("whave > 0 implies a window");
                    strm.output[put..put + copy].copy_from_slice(&window[from..from + copy]);
                    put += copy;
                } else {
                    // Copy from the output, byte by byte (it may overlap).
                    copy = (state.length as usize).min(left);
                    for from in put - offset..put - offset + copy {
                        strm.output[put] = strm.output[from];
                        put += 1;
                    }
                }
                left -= copy;
                let state = &mut strm.state;
                state.length -= copy as u32;
                if state.length == 0 {
                    state.mode = Mode::Len;
                }
            }
            Mode::Lit => {
                if left == 0 {
                    break 'leave;
                }
                strm.output[put] = state.length as u8;
                put += 1;
                left -= 1;
                strm.state.mode = Mode::Len;
            }
            Mode::Check => {
                // state.wrap is nonzero.
                while bits < 32 {
                    if have == 0 {
                        break 'leave;
                    }
                    have -= 1;
                    hold |= u32::from(input[next]) << bits;
                    next += 1;
                    bits += 8;
                }
                out0 -= left;
                let state = &mut strm.state;
                state.total = state.total.wrapping_add(out0 as u32);
                if out0 != 0 {
                    state.update_check(&strm.output[put - out0..put]);
                }
                out0 = left;
                let state = &mut strm.state;
                let got = if state.flags != 0 {
                    hold
                } else {
                    zswap32(hold)
                };
                if got != state.check {
                    bad!(InflateError::IncorrectDataCheck);
                }
                hold = 0;
                bits = 0;
                state.mode = Mode::Length;
            }
            Mode::Length => {
                if state.flags != 0 {
                    need_bits!(32);
                    if hold != strm.state.total {
                        bad!(InflateError::IncorrectLengthCheck);
                    }
                    hold = 0;
                    bits = 0;
                }
                strm.state.mode = Mode::Done;
            }
            Mode::Done => {
                ret = Ret::StreamEnd;
                break 'leave;
            }
            Mode::Bad => {
                ret = Ret::DataError;
                break 'leave;
            }
        }
    }

    // RESTORE(), then update the window and the check value.
    strm.next_out = put;
    strm.avail_out = left;
    strm.next_in = next;
    strm.avail_in = have;
    strm.state.hold = hold;
    strm.state.bits = bits;
    // flush is Z_NO_FLUSH, never Z_FINISH.
    if strm.state.wsize != 0 || (out0 != left && strm.state.mode < Mode::Bad) {
        updatewindow(&mut strm.state, &strm.output, put, out0 - left);
    }
    let in_used = in0 - have;
    let out_made = out0 - left;
    let state = &mut strm.state;
    state.total = state.total.wrapping_add(out_made as u32);
    if out_made != 0 {
        state.update_check(&strm.output[put - out_made..put]);
    }
    if in_used == 0 && out_made == 0 && ret == Ret::Ok {
        ret = Ret::BufError;
    }
    ret
}

/// What the driver collects: `onData` chunks as bytes or as UTF-16 code
/// units (`to: "string"`).
enum Sink {
    Bytes(Vec<u8>),
    Utf16(Vec<u16>),
}

/// `inflate(input, options)` (`lib/inflate.js`): `new Inflate(options)`,
/// one `push(input)` with `Z_NO_FLUSH`, and the result or the error.
fn run(data: &[u8], mut sink: Sink) -> Result<Sink, InflateError> {
    let mut strm = Strm {
        input: data,
        next_in: 0,
        avail_in: data.len(),
        output: Vec::new(),
        next_out: 0,
        avail_out: 0,
        msg: None,
        state: State::new(),
    };
    loop {
        if strm.avail_out == 0 {
            strm.output = vec![0; CHUNK_SIZE];
            strm.next_out = 0;
            strm.avail_out = CHUNK_SIZE;
        }

        let mut status = inflate_call(&mut strm);

        // More streams follow unless the next byte is 0.
        while strm.avail_in > 0 && status == Ret::StreamEnd && data[strm.next_in] != 0 {
            strm.state.reset();
            strm.msg = None;
            status = inflate_call(&mut strm);
        }

        match status {
            Ret::DataError => return Err(strm.msg.expect("Z_DATA_ERROR sets strm.msg")),
            // strm.msg is empty; pako throws msg[Z_NEED_DICT].
            Ret::NeedDict => return Err(InflateError::NeedDictionary),
            Ret::Ok | Ret::StreamEnd | Ret::BufError => {}
        }

        let last_avail_out = strm.avail_out;

        if strm.next_out != 0 && (strm.avail_out == 0 || status == Ret::StreamEnd) {
            match &mut sink {
                Sink::Bytes(out) => out.extend_from_slice(&strm.output[..strm.next_out]),
                Sink::Utf16(out) => {
                    let next_out_utf8 = utf8border(&strm.output[..strm.next_out]);
                    let tail = strm.next_out - next_out_utf8;
                    buf2string(&strm.output[..next_out_utf8], out);
                    strm.next_out = tail;
                    strm.avail_out = CHUNK_SIZE - tail;
                    strm.output
                        .copy_within(next_out_utf8..next_out_utf8 + tail, 0);
                }
            }
        }

        if status == Ret::Ok && last_avail_out == 0 {
            continue;
        }
        if status == Ret::StreamEnd {
            return Ok(sink);
        }
        if strm.avail_in == 0 {
            break;
        }
        // pako would call inflate() again here; with output space available
        // and no progress possible that never returns a result.
        if status == Ret::BufError {
            break;
        }
    }
    Err(InflateError::Incomplete)
}

/// Decompress `data` as `pako.inflate(data)` does: one or more concatenated
/// zlib or gzip streams.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    match run(data, Sink::Bytes(Vec::new()))? {
        Sink::Bytes(out) => Ok(out),
        Sink::Utf16(_) => unreachable!("bytes in, bytes out"),
    }
}

/// `pako.inflate(data, { to: "string" })` as the UTF-16 code units of the
/// JavaScript string it returns.
pub fn inflate_to_utf16(data: &[u8]) -> Result<Vec<u16>, InflateError> {
    match run(data, Sink::Utf16(Vec::new()))? {
        Sink::Utf16(out) => Ok(out),
        Sink::Bytes(_) => unreachable!("string in, string out"),
    }
}

/// Decompress and decode as `pako.inflate(data, { to: "string" })`; see the
/// module documentation for pako's decoder quirks. A lone surrogate becomes
/// U+FFFD.
pub fn inflate_to_string(data: &[u8]) -> Result<String, InflateError> {
    Ok(String::from_utf16_lossy(&inflate_to_utf16(data)?))
}

/// `_utf8len` in `lib/utils/strings.js`: sequence length by lead byte, with
/// 0xFE marked invalid (1).
fn utf8len(b: u8) -> usize {
    match b {
        254 => 1,
        252.. => 6,
        248.. => 5,
        240.. => 4,
        224.. => 3,
        192.. => 2,
        _ => 1,
    }
}

/// `utf8border(buf, max)`: the largest prefix length of `buf` that does not
/// end inside a sequence.
fn utf8border(buf: &[u8]) -> usize {
    let max = buf.len();
    let mut pos = max as isize - 1;
    while pos >= 0 && buf[pos as usize] & 0xc0 == 0x80 {
        pos -= 1;
    }
    if pos <= 0 {
        return max;
    }
    let pos = pos as usize;
    if pos + utf8len(buf[pos]) > max {
        pos
    } else {
        max
    }
}

/// `buf2string(buf, len)`: pako's UTF-8 decoder, appending UTF-16 code units.
fn buf2string(buf: &[u8], out: &mut Vec<u16>) {
    let len = buf.len();
    let mut i = 0;
    while i < len {
        let mut c = u32::from(buf[i]);
        i += 1;
        if c < 0x80 {
            out.push(c as u16);
            continue;
        }
        let mut c_len = utf8len(c as u8);
        if c_len > 4 {
            out.push(0xfffd);
            i += c_len - 1;
            continue;
        }
        c &= match c_len {
            2 => 0x1f,
            3 => 0x0f,
            _ => 0x07,
        };
        while c_len > 1 && i < len {
            c = (c << 6) | (u32::from(buf[i]) & 0x3f);
            i += 1;
            c_len -= 1;
        }
        if c_len > 1 {
            out.push(0xfffd);
            continue;
        }
        if c < 0x10000 {
            out.push(c as u16);
        } else {
            let c = c - 0x10000;
            out.push((0xd800 | ((c >> 10) & 0x3ff)) as u16);
            out.push((0xdc00 | (c & 0x3ff)) as u16);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8border_cases() {
        assert_eq!(utf8border(b"abc"), 3);
        assert_eq!(utf8border(&[b'a', 0xe2, 0x82]), 1);
        assert_eq!(utf8border(&[b'a', 0xe2, 0x82, 0xac]), 4);
        // Only continuation bytes, or a lead byte at 0: the whole buffer.
        assert_eq!(utf8border(&[0x80, 0x80]), 2);
        assert_eq!(utf8border(&[0xe2, 0x82]), 2);
    }

    #[test]
    fn buf2string_cases() {
        let mut out = Vec::new();
        buf2string("é😀".as_bytes(), &mut out);
        assert_eq!(out, "é😀".encode_utf16().collect::<Vec<_>>());
        out.clear();
        // Continuation bytes are masked, not validated.
        buf2string(&[0xc3, 0x28], &mut out);
        assert_eq!(out, [0xe8]);
        out.clear();
        // Truncated at the end of the buffer.
        buf2string(&[0xe2, 0x82], &mut out);
        assert_eq!(out, [0xfffd]);
    }

    /// A stored block (BFINAL, BTYPE 0) holding `bytes`, in a zlib wrapper.
    fn stored(bytes: &[u8]) -> Vec<u8> {
        let mut z = vec![0x78, 0x01, 0x01];
        let len = bytes.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(bytes);
        z.extend_from_slice(&adler32(1, bytes).to_be_bytes());
        z
    }

    #[test]
    fn final_chunk_drops_an_incomplete_sequence() {
        let units = |b: &[u8]| inflate_to_utf16(&stored(b)).unwrap();
        assert_eq!(units(&[b'a', b'b', 0xe2, 0x82]), [0x61, 0x62]);
        assert_eq!(units(&[0xe2, 0x82]), [0xfffd]);
        assert!(units(&[]).is_empty());
    }

    #[test]
    fn a_lone_surrogate_is_kept_as_a_code_unit() {
        // ED A0 80 is U+D800 to pako's decoder.
        let z = stored(&[b'x', 0xed, 0xa0, 0x80]);
        assert_eq!(inflate_to_utf16(&z).unwrap(), [0x78, 0xd800]);
        assert_eq!(inflate_to_string(&z).unwrap(), "x\u{fffd}");
    }

    #[test]
    fn fixed_tables_decode_every_literal_length_symbol() {
        let t = fixed_tables();
        // 0x30 is the 8-bit code of literal 0, reversed: 0b00001100.
        assert_eq!(t.len[0b0000_1100], 8 << 24);
        // 7-bit code 0 is end of block.
        assert_eq!(t.len[0], (7 << 24) | (96 << 16));
        // Distance codes 30 and 31 are invalid.
        assert_eq!(t.dist[0b01111], (5 << 24) | (64 << 16));
        assert_eq!(t.dist[0b11111], (5 << 24) | (64 << 16));
    }
}
