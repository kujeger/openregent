//! FNV-1a, 64 bits: a hash stable across builds, with no dependency.
//! Saves, the art cache and the tests key on it.

/// The offset basis: the hash of no bytes.
pub const START: u64 = 0xcbf2_9ce4_8422_2325;

/// The prime each step multiplies by.
pub const PRIME: u64 = 0x0000_0100_0000_01b3;

/// `bytes` hashed on from `h`: [`START`] for a fresh hash.
pub fn fold(h: u64, bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes
        .into_iter()
        .fold(h, |h, b| (h ^ u64::from(b)).wrapping_mul(PRIME))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published test vectors.
    #[test]
    fn known_strings_hash_as_published() {
        assert_eq!(fold(START, *b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fold(START, *b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fold(START, *b"foobar"), 0x8594_4171_f739_67e8);
    }
}
