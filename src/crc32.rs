//! Tiny dependency-free CRC-32 (IEEE 802.3, the same one as zip/PNG/gzip).

const fn make_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

static TABLE: [u32; 256] = make_table();

/// Incremental CRC-32 hasher.
#[derive(Clone, Copy)]
pub(crate) struct Crc32(u32);

impl Crc32 {
    pub(crate) fn new() -> Self {
        Crc32(0xFFFF_FFFF)
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        let mut c = self.0;
        for &b in bytes {
            c = TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        self.0 = c;
    }

    pub(crate) fn finish(self) -> u32 {
        !self.0
    }
}

/// CRC-32 of `key` followed by `data` (the checksum stored in every entry).
pub(crate) fn entry_crc(key: &[u8], data: &[u8]) -> u32 {
    let mut c = Crc32::new();
    c.update(key);
    c.update(data);
    c.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        let mut c = Crc32::new();
        c.update(b"123456789");
        assert_eq!(c.finish(), 0xCBF4_3926);
    }

    #[test]
    fn incremental_equals_one_shot() {
        let mut a = Crc32::new();
        a.update(b"hello ");
        a.update(b"world");
        let mut b = Crc32::new();
        b.update(b"hello world");
        assert_eq!(a.finish(), b.finish());
    }
}
