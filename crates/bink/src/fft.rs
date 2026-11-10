// Vendored from NihAV, nihav-codec-support/src/dsp/fft.rs; author Kostya
// Shishkov; GNU AGPL version 3. Source, commit and changes: ../README.md.
// Changed here: only what Bink audio's inverse RDFT runs is kept: the
// complex type with the operators it uses, the split-radix inverse FFT
// for power-of-two sizes, its permutation, and the inverse RDFT. The
// generic and 15-point FFTs, twiddles for mixed sizes, the forward
// transforms, `generic_fft`, `Display` and the tests removed; a size
// that is not a power of two is refused; `pub` narrowed to `pub(crate)`.

// Upstream formatting kept.
#![cfg_attr(rustfmt, rustfmt::skip)]

//! FFT and RDFT implementation.
use std::f32::consts;
use std::ops::{Not, Add, Sub, Mul};

/// Complex number.
#[derive(Debug,Clone,Copy,PartialEq)]
pub(crate) struct FFTComplex {
    /// Real part of the numner.
    pub(crate) re: f32,
    /// Complex part of the number.
    pub(crate) im: f32,
}

impl FFTComplex {
    /// Calculates `exp(i * val)`.
    pub(crate) fn exp(val: f32) -> Self {
        FFTComplex { re: val.cos(), im: val.sin() }
    }
    /// Returns `-Im + i * Re`.
    pub(crate) fn rotate(self) -> Self {
        FFTComplex { re: -self.im, im: self.re }
    }
    /// Multiplies complex number by scalar.
    pub(crate) fn scale(self, scale: f32) -> Self {
        FFTComplex { re: self.re * scale, im: self.im * scale }
    }
}

impl Not for FFTComplex {
    type Output = FFTComplex;
    fn not(self) -> Self::Output {
        FFTComplex { re: self.re, im: -self.im }
    }
}

impl Add for FFTComplex {
    type Output = FFTComplex;
    fn add(self, other: Self) -> Self::Output {
        FFTComplex { re: self.re + other.re, im: self.im + other.im }
    }
}

impl Sub for FFTComplex {
    type Output = FFTComplex;
    fn sub(self, other: Self) -> Self::Output {
        FFTComplex { re: self.re - other.re, im: self.im - other.im }
    }
}

impl Mul for FFTComplex {
    type Output = FFTComplex;
    fn mul(self, other: Self) -> Self::Output {
        FFTComplex { re: self.re * other.re - self.im * other.im,
                     im: self.im * other.re + self.re * other.im }
    }
}

pub(crate) const FFTC_ZERO: FFTComplex = FFTComplex { re: 0.0, im: 0.0 };

struct FFTData {
    table:      Vec<FFTComplex>,
}

struct FFTSplitRadix {}

impl FFTSplitRadix {
    fn new_data(bits: u8, _forward: bool) -> FFTData {
        let size = 1 << bits;
        let mut table = Vec::with_capacity(size);
        for _ in 0..4 { table.push(FFTC_ZERO); }
        for b in 3..=bits {
            let qsize = (1 << (b - 2)) as usize;
            let base = -consts::PI / ((qsize * 2) as f32);
            for k in 0..qsize {
                table.push(FFTComplex::exp(base * ((k * 1) as f32)));
                table.push(FFTComplex::exp(base * ((k * 3) as f32)));
            }
        }
        FFTData { table }
    }
    fn ifft(fftdata: &mut FFTData, bits: u8, data: &mut [FFTComplex]) {
        if bits == 0 { return; }
        if bits == 1 {
            let sum01 = data[0] + data[1];
            let dif01 = data[0] - data[1];
            data[0] = sum01;
            data[1] = dif01;
            return;
        }
        if bits == 2 {
            let sum01 = data[0] + data[2];
            let dif01 = data[0] - data[2];
            let sum23 = data[1] + data[3];
            let dif23 = data[1] - data[3];
            data[0] = sum01 + sum23;
            data[1] = dif01 + dif23.rotate();
            data[2] = sum01 - sum23;
            data[3] = dif01 - dif23.rotate();
            return;
        }
        let qsize = (1 << (bits - 2)) as usize;
        let hsize = (1 << (bits - 1)) as usize;
        let q3size = qsize + hsize;

        Self::ifft(fftdata, bits - 1, &mut data[0     ..hsize]);
        Self::ifft(fftdata, bits - 2, &mut data[hsize ..q3size]);
        Self::ifft(fftdata, bits - 2, &mut data[q3size..]);
        let off = hsize;
        {
            let t3 =  data[0 + hsize] + data[0 + q3size];
            let t4 = (data[0 + hsize] - data[0 + q3size]).rotate();
            let e1 = data[0];
            let e2 = data[0 + qsize];
            data[0]          = e1 + t3;
            data[0 + qsize]  = e2 + t4;
            data[0 + hsize]  = e1 - t3;
            data[0 + q3size] = e2 - t4;
        }
        for k in 1..qsize {
            let t1 = !fftdata.table[off + k * 2 + 0] * data[k + hsize];
            let t2 = !fftdata.table[off + k * 2 + 1] * data[k + q3size];
            let t3 =  t1 + t2;
            let t4 = (t1 - t2).rotate();
            let e1 = data[k];
            let e2 = data[k + qsize];
            data[k]             = e1 + t3;
            data[k + qsize]     = e2 + t4;
            data[k + hsize]     = e1 - t3;
            data[k + qsize * 3] = e2 - t4;
        }
    }
}

/// FFT working context: a power-of-two inverse FFT.
pub(crate) struct FFT {
    swaps:  Vec<usize>,
    bits:   u8,
    data:   FFTData,
}

impl FFT {
    /// Performs inplace inverse FFT.
    pub(crate) fn do_ifft_inplace(&mut self, data: &mut [FFTComplex]) {
        for idx in 0..self.swaps.len() {
            let nidx = self.swaps[idx];
            if idx != nidx {
                data.swap(nidx, idx);
            }
        }
        FFTSplitRadix::ifft(&mut self.data, self.bits, data);
    }
}

fn gen_sr_perms(swaps: &mut [usize], size: usize) {
    if size <= 4 { return; }
    let mut evec:  Vec<usize> = Vec::with_capacity(size / 2);
    let mut ovec1: Vec<usize> = Vec::with_capacity(size / 4);
    let mut ovec2: Vec<usize> = Vec::with_capacity(size / 4);
    for k in 0..size/4 {
        evec.push (swaps[k * 4 + 0]);
        ovec1.push(swaps[k * 4 + 1]);
        evec.push (swaps[k * 4 + 2]);
        ovec2.push(swaps[k * 4 + 3]);
    }
    for k in 0..size/2 { swaps[k]            = evec[k]; }
    for k in 0..size/4 { swaps[k +   size/2] = ovec1[k]; }
    for k in 0..size/4 { swaps[k + 3*size/4] = ovec2[k]; }
    gen_sr_perms(&mut swaps[0..size/2],        size/2);
    gen_sr_perms(&mut swaps[size/2..3*size/4], size/4);
    gen_sr_perms(&mut swaps[3*size/4..],       size/4);
}

fn gen_swaps_for_perm(swaps: &mut Vec<usize>, perms: &[usize]) {
    let mut idx_arr: Vec<usize> = Vec::with_capacity(perms.len());
    for i in 0..perms.len() { idx_arr.push(i); }
    let mut run_size = 0;
    let mut run_pos  = 0;
    for idx in 0..perms.len() {
        if perms[idx] == idx_arr[idx] {
            if run_size == 0 { run_pos = idx; }
            run_size += 1;
        } else {
            for i in 0..run_size {
                swaps.push(run_pos + i);
            }
            run_size = 0;
            let mut spos = idx + 1;
            while idx_arr[spos] != perms[idx] { spos += 1; }
            idx_arr[spos] = idx_arr[idx];
            idx_arr[idx]  = perms[idx];
            swaps.push(spos);
        }
    }
}

/// Constructs a new inverse `FFT` context; `None` unless `size` is a
/// power of two (upstream's `FFTBuilder::new_fft`).
fn new_fft(size: usize) -> Option<FFT> {
    if !size.is_power_of_two() { return None; }
    let bits = size.trailing_zeros() as u8;
    let data = FFTSplitRadix::new_data(bits, false);
    let mut perms: Vec<usize> = (0..size).collect();
    let mut swaps: Vec<usize> = Vec::with_capacity(size);
    gen_sr_perms(&mut perms, size);
    gen_swaps_for_perm(&mut swaps, perms.as_slice());
    Some(FFT { swaps, bits, data })
}

/// RDFT working context: the inverse transform with an inverse FFT.
pub(crate) struct RDFT {
    table:  Vec<FFTComplex>,
    fft:    FFT,
    size:   usize,
}

fn crossadd(a: FFTComplex, b: FFTComplex) -> FFTComplex {
    FFTComplex { re: a.re + b.re, im: a.im - b.im }
}

impl RDFT {
    /// Calculates inplace inverse RDFT.
    pub(crate) fn do_rdft_inplace(&mut self, buf: &mut [FFTComplex]) {
        for n in 0..self.size/2 {
            let in0 = buf[n + 1];
            let in1 = buf[self.size - n - 1];

            let t0 = crossadd(in0, in1);
            let t1 = FFTComplex { re: in1.im + in0.im, im: in1.re - in0.re };
            let tab = self.table[n];
            let t2 = FFTComplex { re: t1.im * tab.im + t1.re * tab.re, im: t1.im * tab.re - t1.re * tab.im };

            buf[n + 1] = FFTComplex { re: t0.im - t2.im, im: t0.re - t2.re }; // (t0 - t2).conj().rotate()
            buf[self.size - n - 1] = (t0 + t2).rotate();
        }
        let a = buf[0].re;
        let b = buf[0].im;
        buf[0].re = a - b;
        buf[0].im = a + b;
        self.fft.do_ifft_inplace(buf);
        for n in 0..self.size {
            buf[n] = FFTComplex{ re: buf[n].im, im: buf[n].re };
        }
    }
}

/// Constructs a new inverse `RDFT` context of `size` complex values;
/// `None` unless a power of two (upstream's
/// `RDFTBuilder::new_rdft(size, false, false)`).
pub(crate) fn new_inverse_rdft(size: usize) -> Option<RDFT> {
    let mut table: Vec<FFTComplex> = Vec::with_capacity(size / 4);
    let (base, scale) = (-consts::PI / (size as f32), 1.0);
    for i in 0..size/2 {
        table.push(FFTComplex::exp(base * ((i + 1) as f32)).scale(scale));
    }
    let fft = new_fft(size)?;
    Some(RDFT { table, fft, size })
}
