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
