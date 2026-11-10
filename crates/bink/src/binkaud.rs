// Vendored from NihAV, nihav-rad/src/codecs/binkaud.rs; author Kostya
// Shishkov; GNU AGPL version 3. Source, commit and changes: ../README.md.
// Changed here:
// - only the RDFT flavour (every Majesty track's) is kept: the DCT
//   transform and Bink 'b' audio (`version_b`, raw floats) removed, so
//   `use_dct` is gone and its branches with it;
// - the `NADecoder`/`NAOptionHandler` glue, `NAAudioInfo`, channel maps
//   and frame buffers replaced: `new(rate, channels)`, and `decode`
//   appending interleaved samples to a `Vec<f32>`;
// - the RDFT runs on a `FFTComplex` scratch copy instead of transmuting
//   the coefficients in place (`unsafe`);
// - stereo comes out channel 1 first: upstream's channel order is
//   swapped against ffmpeg's (Majesty's MV02 compared);
// - the tests (they read upstream's sample files) removed; `validate!`
//   returns without printing;
// - a sample count the packet cannot hold is refused before it is
//   allocated.

// Upstream formatting kept.
#![cfg_attr(rustfmt, rustfmt::skip)]

use crate::Error as DecoderError;
use crate::bitreader::*;
use crate::fft::*;

use super::binkauddata::*;

type DecoderResult<T> = Result<T, DecoderError>;

/// Bits a subframe reads at the least: its two leading floats.
const MIN_SUBFRAME_BITS: usize = 2 * 29;

macro_rules! validate {
    ($a:expr) => { if !$a { return Err(DecoderError::InvalidData); } };
}

pub(crate) struct BinkAudioDecoder {
    channels:   usize,
    transform:  RDFT,
    /// The RDFT's input and output, as complex pairs.
    scratch:    Vec<FFTComplex>,
    len:        usize,
    quants:     [f32; 96],
    bands:      [usize; MAX_BANDS + 1],
    num_bands:  usize,
    duration:   usize,
    coeffs:     [f32; 4096],
    delay:      [[f32; 256]; 2],
    first_frm:  bool,
    scale:      f32,
}

fn read_bink_float(br: &mut BitReader) -> DecoderResult<f32> {
    let exp                                     = br.read(5)? as u8;
    let mant                                    = br.read(23)?;
    let sign                                    = br.read(1)?;
    let nexp = exp.wrapping_add(0x7E) as u32;
    let nmant = (mant << 1) & ((1 << 23) - 1);
    Ok(f32::from_bits((sign << 31) | (nexp << 23) | nmant))
}

fn overlap(a: &[f32], b: &[f32], dst: &mut [f32], len: usize, step: usize) {
    for i in 0..len {
        dst[i] = (a[i] * ((len - i) as f32) + b[i * step] * (i as f32)) / (len as f32);
    }
}

impl BinkAudioDecoder {
    /// A decoder for a track at `srate` Hz of `channels` (1 or 2), as
    /// upstream's `init` sets it up.
    pub(crate) fn new(srate: usize, channels: usize) -> DecoderResult<Self> {
        validate!((1..=2).contains(&channels) && srate > 0);
        let mut frame_bits = if srate < 22050 { 9 } else if srate < 44100 { 10 } else { 11 };
        if channels > 1 {
            frame_bits += 1;
        }
        let len = 1 << frame_bits;
        let mut duration = len - (len >> 4);
        let single = channels == 2; // RDFT codes samples interleaved as single buffer
        if single {
            duration >>= 1;
        }
        let transform = new_inverse_rdft(len >> 1).ok_or(DecoderError::InvalidData)?;
        let scale = 1.0 / (32768.0 * (len as f32).sqrt());
        let s_srate = if single { srate } else { srate >> 1 };
        let mut bands = [MAX_BANDS + 1; 26];
        let mut num_bands = 0;
        init_bands(s_srate, len, &mut num_bands, &mut bands);

        Ok(Self {
            channels,
            transform,
            scratch:    vec![FFTC_ZERO; len >> 1],
            len,
            quants:     get_quants_table(),
            bands,
            num_bands,
            duration,
            coeffs:     [0.0; 4096],
            delay:      [[0.0; 256]; 2],
            first_frm:  true,
            scale,
        })
    }
    fn decode_block(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        self.coeffs[0] = read_bink_float(br)? * self.scale;
        self.coeffs[1] = read_bink_float(br)? * self.scale;
        let mut quants: [f32; MAX_BANDS] = [0.0; MAX_BANDS];
        for quant in quants[..self.num_bands].iter_mut() {
            let idx                             = br.read(8)? as usize;
            *quant = self.quants[idx.min(self.quants.len() - 1)] * self.scale;
        }
        let mut idx = 2;
        let mut band_idx = 0;
        self.coeffs = [0.0; 4096];
        while idx < self.len {
            let width = if br.read_bool()? {
                    let idx                     = br.read(4)? as usize;
                    RUN_TAB[idx] * 8
                } else {
                    8
                };
            let end = (idx + width).min(self.len);
            let bits                            = br.read(4)? as u8;
            if bits != 0 {
                for i in idx..end {
                    while self.bands[band_idx] <= i { band_idx += 1; }
                    let q = quants[band_idx - 1];
                    let coeff                   = br.read(bits)?;
                    if coeff != 0 {
                        if br.read_bool()? {
                            self.coeffs[i] = -(coeff as f32) * q;
                        } else {
                            self.coeffs[i] =  (coeff as f32) * q;
                        }
                    }
                }
            }
            idx = end;
        }
        Ok(())
    }
    #[allow(clippy::identity_op)]
    fn output(&mut self, dst: &mut [f32], off0: usize, off1: usize, chno: usize) {
        // Upstream transmutes the coefficients to complex pairs in place.
        for (pair, c) in self.coeffs.chunks_exact(2).zip(self.scratch.iter_mut()) {
            *c = FFTComplex { re: pair[0], im: pair[1] };
        }
        self.transform.do_rdft_inplace(&mut self.scratch);
        for (pair, c) in self.coeffs.chunks_exact_mut(2).zip(self.scratch.iter()) {
            pair[0] = c.re;
            pair[1] = c.im;
        }
        for i in (0..self.coeffs.len()).step_by(2) {
            self.coeffs.swap(i, i + 1);
        }
        if self.channels == 1 {
            let overlap_len = if self.first_frm { 0 } else { self.len >> 4 };
            let out = if chno == 0 { &mut dst[off0..] } else { &mut dst[off1..] };
            overlap(&self.delay[chno], &self.coeffs, out, overlap_len, 1);
            let out2 = &mut out[overlap_len..self.duration];
            out2.copy_from_slice(&self.coeffs[overlap_len..self.duration]);
            for i in 0..(self.len >> 4) {
                self.delay[chno][i] = self.coeffs[self.duration + i];
            }
        } else {
            let overlap_len = if self.first_frm { 0 } else { self.len >> 8 };
            overlap(&self.delay[0], &self.coeffs[0..], &mut dst[off0..], overlap_len, 2);
            overlap(&self.delay[1], &self.coeffs[1..], &mut dst[off1..], overlap_len, 2);
            for i in overlap_len..self.duration {
                dst[i + off0] = self.coeffs[i * 2 + 0];
                dst[i + off1] = self.coeffs[i * 2 + 1];
            }
            for i in 0..(self.len >> 8) {
                self.delay[0][i] = self.coeffs[self.duration * 2 + i * 2 + 0];
                self.delay[1][i] = self.coeffs[self.duration * 2 + i * 2 + 1];
            }
        }
    }

    /// Decodes one frame's audio packet, appending its samples to `out`,
    /// interleaved by channel. Upstream outputs planar channels.
    pub(crate) fn decode(&mut self, pktbuf: &[u8], out: &mut Vec<f32>) -> DecoderResult<()> {
        validate!(pktbuf.len() > 1);
        let mut br = BitReader::new(pktbuf);
        let nsamples = br.read(32)? as usize;
        // Each subframe reads two 29-bit floats at least: a count the
        // packet cannot hold is refused before its samples are allocated.
        let num_subframes = nsamples / self.duration / self.channels / 2;
        validate!(num_subframes * MIN_SUBFRAME_BITS <= pktbuf.len().saturating_sub(4) * 8);

        // Planar, as upstream's buffer: channel 0, then channel 1.
        let per_channel = nsamples / self.channels / 2;
        let mut dst = vec![0.0f32; per_channel * self.channels];
        let mut off0 = 0;
        let mut off1 = per_channel;

        for _subfrm in 0..num_subframes {
            self.decode_block(&mut br)?;
            self.output(&mut dst[0..], off0, off1, 0);
            self.first_frm = false;
            let left = br.left() & 31;
            if left != 0 {
                br.skip(left as u32)?;
            }
            off0 += self.duration;
            off1 += self.duration;
        }

        // Upstream's channel 0 is ffmpeg's right: the two orders agree
        // only with channel 1 first (Majesty's MV02 compared).
        for i in 0..per_channel {
            for ch in (0..self.channels).rev() {
                out.push(dst[ch * per_channel + i]);
            }
        }
        Ok(())
    }
}
