//! Adler-32 and CRC-32, as pako computes them (`lib/zlib/adler32.js`,
//! `lib/zlib/crc32.js`): the zlib trailer checksum and the gzip trailer and
//! header checksums.

/// Adler-32 of `data`, continuing from `adler` (1 for a new stream).
pub(crate) fn adler32(adler: u32, data: &[u8]) -> u32 {
    const BASE: u32 = 65521;
    // Largest n with 255 n (n + 1) / 2 + (n + 1) (BASE - 1) <= 2^32 - 1, as
    // in pako and zlib: the sums cannot overflow before the modulo.
    const NMAX: usize = 5552;
    let mut s1 = adler & 0xffff;
    let mut s2 = adler >> 16;
    for chunk in data.chunks(NMAX) {
        for &b in chunk {
            s1 += u32::from(b);
            s2 += s1;
        }
        s1 %= BASE;
        s2 %= BASE;
    }
    s1 | (s2 << 16)
}

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

static CRC_TABLE: [u32; 256] = crc_table();

/// CRC-32 (IEEE 802.3) of `data`, continuing from `crc` (0 for a new stream).
pub(crate) fn crc32(crc: u32, data: &[u8]) -> u32 {
    let mut c = !crc;
    for &b in data {
        c = CRC_TABLE[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

/// `x^(2^k) mod p` for k = 0..32, in CRC-32's reflected representation
/// (zlib's `x2n_table`).
const fn x2n_table() -> [u32; 32] {
    let mut table = [0u32; 32];
    let mut p = 1u32 << 30; // x^1
    table[0] = p;
    let mut n = 1;
    while n < 32 {
        p = multmodp(p, p);
        table[n] = p;
        n += 1;
    }
    table
}

static X2N_TABLE: [u32; 32] = x2n_table();

/// `a * b mod p` over GF(2), reflected (zlib's `multmodp`).
const fn multmodp(a: u32, mut b: u32) -> u32 {
    let mut m = 1u32 << 31;
    let mut p = 0u32;
    loop {
        if a & m != 0 {
            p ^= b;
            if a & (m - 1) == 0 {
                break;
            }
        }
        m >>= 1;
        b = if b & 1 != 0 {
            (b >> 1) ^ 0xedb8_8320
        } else {
            b >> 1
        };
    }
    p
}

/// `x^(n * 2^k) mod p` (zlib's `x2nmodp`).
fn x2nmodp(mut n: u64, mut k: usize) -> u32 {
    let mut p = 1u32 << 31; // x^0
    while n != 0 {
        if n & 1 != 0 {
            p = multmodp(X2N_TABLE[k & 31], p);
        }
        n >>= 1;
        k += 1;
    }
    p
}

/// [`crc32`] continued over `n` zero bytes, in O(log n): each zero byte
/// multiplies the (un-inverted) register by x^8.
pub(crate) fn crc32_zeros(crc: u32, n: u64) -> u32 {
    !multmodp(x2nmodp(n, 3), !crc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert_eq!(adler32(1, b""), 1);
        assert_eq!(adler32(1, b"Wikipedia"), 0x11e6_0398);
        assert_eq!(crc32(0, b""), 0);
        assert_eq!(crc32(0, b"123456789"), 0xcbf4_3926);
        // Continuation equals one pass.
        assert_eq!(crc32(crc32(0, b"1234"), b"56789"), 0xcbf4_3926);
        assert_eq!(adler32(adler32(1, b"Wiki"), b"pedia"), 0x11e6_0398);
    }

    #[test]
    fn crc32_zeros_equals_feeding_zeros() {
        for start in [0u32, 0xcbf4_3926, 0xffff_ffff, 0x1234_5678] {
            for n in [0u64, 1, 2, 3, 7, 8, 100, 1000, 65536, 100_003] {
                let direct = crc32(start, &vec![0u8; n as usize]);
                assert_eq!(crc32_zeros(start, n), direct, "start {start:#x}, {n} zeros");
            }
        }
    }

    #[test]
    fn adler32_long_input_does_not_overflow() {
        let data = vec![0xffu8; 1 << 20];
        let (mut s1, mut s2) = (1u64, 0u64);
        for &b in &data {
            s1 = (s1 + u64::from(b)) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        assert_eq!(adler32(1, &data), (s1 | (s2 << 16)) as u32);
    }
}
