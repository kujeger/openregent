// Vendored from NihAV, nihav-core/src/io/bitreader.rs; author Kostya
// Shishkov; GNU AGPL version 3. Source, commit and changes: ../README.md.
// Changed here: only the LSB-first little-endian mode Bink uses is kept
// (`BitReaderMode` and the other modes' fill paths removed); `read_s`,
// `peek` beyond the codebook's use, `seek`, `align`, `reverse_bits`,
// `get_data` and the doc examples removed; `pub` narrowed to `pub(crate)`.

// Upstream formatting kept.
#![cfg_attr(rustfmt, rustfmt::skip)]

//! Bitstream reader functionality.
//!
//! Bitstream reader operates on `&[u8]` and reads bits from the slice
//! least significant bit first, in little-endian 32-bit words.

/// A list specifying general bitstream reading errors.
#[derive(Debug,Clone,Copy)]
pub(crate) enum BitReaderError {
    /// The reader is at the end of bitstream.
    BitstreamEnd,
    /// The caller tried to read too many bits at once (e.g. 128).
    TooManyBitsRequested,
}

use self::BitReaderError::*;

/// A specialised `Result` type for bitstream operations.
pub(crate) type BitReaderResult<T> = Result<T, BitReaderError>;

/// Bitstream reader.
#[derive(Debug,Clone)]
pub(crate) struct BitReader<'a> {
    cache: u64,
    bits:  u8,
    pos:   usize,
    src:   &'a [u8],
}

#[allow(clippy::identity_op)]
impl<'a> BitReader<'a> {

    /// Constructs a new instance of bitstream reader.
    pub(crate) fn new(src: &'a [u8]) -> Self {
        BitReader{ cache: 0, pos: 0, bits: 0, src }
    }

    /// Reports the current bit position in the bitstream (usually simply the number of bits read so far).
    pub(crate) fn tell(&self) -> usize {
        self.pos * 8 - (self.bits as usize)
    }

    /// Reports the amount of bits left until the end of the bitstream.
    pub(crate) fn left(&self) -> isize {
        ((self.src.len() as isize) - (self.pos as isize)) * 8 + (self.bits as isize)
    }

    fn fill32le32(&mut self, src: &[u8]) {
        let nw = (u32::from(src[3]) << 24) |
                 (u32::from(src[2]) << 16) |
                 (u32::from(src[1]) <<  8) |
                 (u32::from(src[0]) <<  0);
        self.cache |= u64::from(nw) << self.bits;
    }

    #[inline(always)]
    fn refill(&mut self) -> BitReaderResult<()> {
        if self.pos >= self.src.len() { return Err(BitstreamEnd) }
        while self.bits <= 32 {
            if self.pos + 4 <= self.src.len() {
                let buf = &self.src[self.pos..];
                self.fill32le32(buf);
                self.pos  +=  4;
                self.bits += 32;
            } else {
                let mut buf: [u8; 4] = [0, 0, 0, 0];
                let mut newbits: u8 = 0;
                for out in buf.iter_mut().take(3) {
                    if self.pos < self.src.len() {
                        *out = self.src[self.pos];
                        self.pos += 1;
                        newbits += 8;
                    }
                }
                if newbits == 0 { break; }
                self.fill32le32(&buf);
                self.bits += newbits;
            }
        }
        Ok(())
    }

    #[inline(always)]
    fn read_cache(&mut self, nbits: u8) -> u32 {
        let res = ((1u64 << nbits) - 1) & self.cache;
        res as u32
    }

    #[inline(always)]
    fn skip_cache(&mut self, nbits: u8) {
        self.cache >>= nbits;
        self.bits -= nbits;
    }

    #[inline(always)]
    fn reset_cache(&mut self) {
        self.bits = 0;
        self.cache = 0;
    }

    /// Reads the specified amount of bits as an unsigned value.
    ///
    /// The amount should fit into 32 bits, if you need more then
    /// you should read it as several parts. If the amount of bits
    /// requested to read is larger than the amount of bits left the
    /// call will return [`BitstreamEnd`].
    #[inline(always)]
    pub(crate) fn read(&mut self, nbits: u8) -> BitReaderResult<u32> {
        if nbits == 0 { return Ok(0) }
        if nbits > 32 { return Err(TooManyBitsRequested) }
        if self.bits < nbits {
            self.refill()?;
            if self.bits < nbits { return Err(BitstreamEnd) }
        }
        let res = self.read_cache(nbits);
        self.skip_cache(nbits);
        Ok(res)
    }

    /// Reads single bit from the stream and interprets it as a boolean value.
    #[inline(always)]
    pub(crate) fn read_bool(&mut self) -> BitReaderResult<bool> {
        if self.bits < 1 {
            self.refill()?;
            if self.bits < 1 { return Err(BitstreamEnd) }
        }
        let res = self.read_cache(1);
        self.skip_cache(1);
        Ok(res == 1)
    }

    /// Retrieves the next bits from the stream without advancing.
    ///
    /// If the bitstream is shorter than the amount of bits requested the result is padded with zeroes.
    #[inline(always)]
    pub(crate) fn peek(&mut self, nbits: u8) -> u32 {
        if nbits > 32 { return 0 }
        if self.bits < nbits { let _ = self.refill(); }
        self.read_cache(nbits)
    }

    /// Skips the requested amount of bits.
    ///
    /// The amount of bits to skip can be arbitrary large.
    /// If it skips more bits than there are actually in the stream the call will return [`BitstreamEnd`]
    #[inline(always)]
    pub(crate) fn skip(&mut self, nbits: u32) -> BitReaderResult<()> {
        if u32::from(self.bits) >= nbits {
            self.skip_cache(nbits as u8);
            return Ok(());
        }
        let mut skip_bits = nbits - u32::from(self.bits);
        self.reset_cache();
        self.pos += ((skip_bits / 32) * 4) as usize;
        skip_bits &= 0x1F;
        if skip_bits > 0 {
            self.refill()?;
            if u32::from(self.bits) < skip_bits {
                return Err(BitstreamEnd);
            }
            self.skip_cache(skip_bits as u8);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn br_works() {
        const DATA2: [u8; 1] = [ 0b00011011 ];
        let src = &DATA2;
        let mut br = BitReader::new(src);
        assert_eq!(br.read(5).unwrap(), 0b11011);
    }
}
