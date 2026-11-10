//! The engine's CRC-32: crc32 update's table, from all ones and not
//! inverted. Text ids
//! ([`super::fourcc::string_id`]) and a mod's quest's code
//! (`quest::key_of`) are made by it.

/// The CRC-32 polynomial, reflected, of [`CRC_TABLE`].
const CRC_POLY: u32 = 0xedb8_8320;

/// Crc32 update's table, at crc32 table: CRC-32's.
pub const CRC_TABLE: [u32; 256] = crc_table();

const fn crc_table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut n: u32 = 0;

    while n < 256 {
        let mut c = n;
        let mut bit = 0;

        while bit < 8 {
            c = if c & 1 == 0 {
                c >> 1
            } else {
                (c >> 1) ^ CRC_POLY
            };
            bit += 1;
        }

        table[n as usize] = c;
        n += 1;
    }

    table
}

/// CRC-32 of `bytes` from all ones, not inverted: crc add on a
/// new crc ctor.
#[must_use]
pub fn crc(bytes: &[u8]) -> u32 {
    bytes.iter().fold(u32::MAX, |c, b| {
        CRC_TABLE[((c ^ u32::from(*b)) & 0xff) as usize] ^ (c >> 8)
    })
}
