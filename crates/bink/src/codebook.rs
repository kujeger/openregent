// Vendored from NihAV, nihav-core/src/io/codebook.rs; author Kostya
// Shishkov; GNU AGPL version 3. Source, commit and changes: ../README.md.
// Changed here: only the LSB mode and `TableCodebookDescReader` are kept;
// codes longer than the lookup table (escape tables, the `HashMap`
// buckets) are refused rather than built, as Bink's are at most 7 bits;
// the MSB mode, the full and short description readers, the doc examples
// and the `CodebookReader` trait (now a `BitReader` method) removed; `pub`
// narrowed to `pub(crate)`.

// Upstream formatting kept.
#![cfg_attr(rustfmt, rustfmt::skip)]

//! Codebook support for bitstream reader.
//!
//! Codebook is a set of unique bit strings and values assigned to them.

use std::cmp::max;
use super::bitreader::BitReader;

/// A list specifying general codebook operations errors.
#[derive(Debug)]
pub(crate) enum CodebookError {
    /// Codebook description contains errors.
    InvalidCodebook,
    /// Bitstream contains a sequence not present in codebook.
    InvalidCode,
}

/// A specialised `Result` type for codebook operations.
pub(crate) type CodebookResult<T> = Result<T, CodebookError>;

/// The interface for providing a list of codeword definitions to the codebook creator.
///
/// Codeword definitions with zero length are ignored (those may be used to create sparse codebook definitions though).
pub(crate) trait CodebookDescReader<S> {
    /// Returns the codeword length for the provided index.
    fn bits(&mut self, idx: usize) -> u8;
    /// Returns the codeword bits for the provided index.
    fn code(&mut self, idx: usize) -> u32;
    /// Returns the codeword value (aka codeword symbol) for the provided index.
    fn sym (&mut self, idx: usize) -> S;
    /// Returns the total number of defined codewords.
    fn len (&mut self)             -> usize;
}

/// The codebook structure for code reading.
pub(crate) struct Codebook<S> {
    table: Vec<u32>,
    syms:  Vec<S>,
    lut_bits: u8,
}

const TABLE_FILL_VALUE: u32 = 0x7F;
const MAX_LUT_BITS: u8 = 10;

fn fill_lut_lsb(table: &mut [u32], code: u32, bits: u8, lut_bits: u8, symidx: u32) -> CodebookResult<()> {
    let fill_len  = lut_bits - bits;
    let fill_size = 1 << fill_len;
    let fill_code = code;
    let step = lut_bits - fill_len;
    for j in 0..fill_size {
        let idx = (fill_code + (j << step)) as usize;
        if table[idx] != TABLE_FILL_VALUE { return Err(CodebookError::InvalidCodebook); }
        table[idx] = (symidx << 8) | u32::from(bits);
    }
    Ok(())
}

impl<S: Copy> Codebook<S> {

    /// Constructs a new `Codebook` instance using provided codebook description, read LSB first.
    pub(crate) fn new(cb: &mut dyn CodebookDescReader<S>) -> CodebookResult<Self> {
        let mut maxbits = 0;
        let mut nnz = 0;

        for i in 0..cb.len() {
            let bits = cb.bits(i);
            if bits > 0 {
                nnz += 1;
                if cb.code(i) >= (1 << bits) {
                    return Err(CodebookError::InvalidCodebook);
                }
            }
            maxbits = max(bits, maxbits);
        }
        if maxbits == 0 || maxbits > MAX_LUT_BITS { return Err(CodebookError::InvalidCodebook); }

        let tab_len = 1 << maxbits;
        let mut table: Vec<u32> = vec![TABLE_FILL_VALUE; tab_len];
        let mut syms:  Vec<S>   = Vec::with_capacity(nnz);

        let mut symidx: u32 = 0;
        for i in 0..cb.len() {
            let bits = cb.bits(i);
            let code = cb.code(i);
            if bits == 0 { continue; }
            fill_lut_lsb(&mut table, code, bits, maxbits, symidx)?;
            symidx += 1;
        }

        for i in 0..cb.len() {
            if cb.bits(i) > 0 {
                syms.push(cb.sym(i));
            }
        }

        Ok(Codebook { table, syms, lut_bits: maxbits })
    }
}

impl BitReader<'_> {
    /// Reads the codeword from the bitstream and returns its value (or [`CodebookError::InvalidCode`] on error).
    pub(crate) fn read_cb<S: Copy>(&mut self, cb: &Codebook<S>) -> CodebookResult<S> {
        let lut_idx = self.peek(cb.lut_bits) as usize;
        if cb.table[lut_idx] == TABLE_FILL_VALUE { return Err(CodebookError::InvalidCode); }
        let bits = cb.table[lut_idx] & 0x7F;
        let idx  = (cb.table[lut_idx] >> 8) as usize;
        if (bits as isize) > self.left() {
            return Err(CodebookError::InvalidCode);
        }
        self.skip(bits).map_err(|_| CodebookError::InvalidCode)?;
        Ok(cb.syms[idx])
    }
}

/// Flexible codebook description that uses two separate arrays for codeword bits and lengths and a function that maps codeword index into its symbol.
pub(crate) struct TableCodebookDescReader<'a, CodeType:'static, IndexType:'static> {
    bits:       &'a [u8],
    codes:      &'a [CodeType],
    idx_map:    fn(usize) -> IndexType,
}

impl<'a, CodeType, IndexType> TableCodebookDescReader<'a, CodeType, IndexType> {
    /// Constructs a new `TableCodebookDescReader` instance.
    pub(crate) fn new(codes: &'a [CodeType], bits: &'a [u8], map_func: fn(usize) -> IndexType) -> Self {
        Self { bits, codes, idx_map: map_func }
    }
}
impl<CodeType: Copy+Into<u32>, IndexType> CodebookDescReader<IndexType> for TableCodebookDescReader<'_, CodeType, IndexType>
{
    fn bits(&mut self, idx: usize) -> u8  { self.bits[idx] }
    fn code(&mut self, idx: usize) -> u32 { self.codes[idx].into() }
    fn sym (&mut self, idx: usize) -> IndexType { (self.idx_map)(idx) }
    fn len(&mut self) -> usize { self.bits.len() }
}

#[cfg(test)]
mod test {
    use super::*;

    // From upstream's `test_cb`, its LSB part, through a table reader.
    #[test]
    fn test_cb() {
        const BITS_LE: [u8; 3] = [0b11101111, 0b01110010, 0b01];
        const CODES: [u32; 6] = [0b00, 0b01, 0b011, 0b10111, 0b00111, 0b1011101111];
        const LENS: [u8; 6] = [2, 2, 3, 5, 5, 10];
        let mut brl = BitReader::new(&BITS_LE);
        let mut cfr = TableCodebookDescReader::new(&CODES, &LENS, |idx| idx);
        let cb = Codebook::new(&mut cfr).unwrap();
        assert_eq!(brl.read_cb(&cb).unwrap(), 5);
        assert_eq!(brl.read_cb(&cb).unwrap(), 0);
        assert_eq!(brl.read_cb(&cb).unwrap(), 3);
        assert_eq!(brl.read_cb(&cb).unwrap(), 0);
    }
}
