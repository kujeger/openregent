# bink

A Bink 1 decoder, revisions `f` to `k`: video, and audio in its RDFT
flavour. What Majesty's 39 videos use (`BIKf`, `BIKh`, `BIKi`; six with
sound).

## Origin

The decoder is NihAV's, by Kostya Shishkov.

| | |
|---|---|
| Project | NihAV, <https://nihav.org> |
| Repository | `git://git.nihav.org/nihav.git` |
| Commit | `13f31633cf325b0036d697af7bf05c611b4b8967` (late September 2026 bundle) |
| Author | Kostya Shishkov <kostya.shishkov@gmail.com> |
| Licence | GNU Affero General Public License version 3, in [LICENSE](LICENSE) (NihAV's `COPYING`) |

Upstream files carry no copyright header; each vendored file here
names its upstream file, author and licence at its top.

| Here | Upstream |
|---|---|
| `src/binkvid.rs` | nihav-rad/src/codecs/binkvid.rs |
| `src/binkviddata.rs` | nihav-rad/src/codecs/binkviddata.rs |
| `src/demux.rs` | nihav-rad/src/demuxers/bink.rs |
| `src/bitreader.rs` | nihav-core/src/io/bitreader.rs |
| `src/codebook.rs` | nihav-core/src/io/codebook.rs |
| `src/binkaud.rs` | nihav-rad/src/codecs/binkaud.rs |
| `src/binkauddata.rs` | nihav-rad/src/codecs/binkauddata.rs |
| `src/fft.rs` | nihav-codec-support/src/dsp/fft.rs |
| `src/lib.rs` | this project's own: the API (`Video`, `Frame`, `Header`, `Audio`) and RGB conversion |

## Changes

- Trimmed to Bink 1: no Bink 2, no revision `b`, no alpha or grey
  planes, no DCT audio, no encoder, no seeking.
- The FFT keeps only the power-of-two split-radix inverse transform
  and the inverse RDFT that Bink audio runs.
- Audio comes out interleaved, stereo with the channels swapped against
  upstream: upstream's order is the reverse of ffmpeg's (checked on
  Majesty's `MV02`). Mono samples are upstream's bit for bit.
- NihAV's framework (`NADecoder`, demuxer registry, `NAFrame` buffers,
  `ByteIO`, options) replaced by plain structs over a byte slice; the
  previous frame is passed in, not held by `IPShuffler`.
- The bit reader keeps only LSB-first little-endian mode; the codebook
  only LSB mode, with no escape tables (Bink's codes are at most 7 bits).
- Both `unsafe` blocks replaced: `MaybeUninit` building the 16 Huffman
  tables by `std::array::from_fn`; the audio coefficients transmuted to
  complex pairs by a copy.
- `validate!` no longer prints. Upstream tests (they read upstream's
  sample files) removed; this project tests on Majesty's videos.
- Damaged input is refused, not panicked on or allocated for: a tree
  description repeating a symbol, a frame count past the file, an audio
  sample count past its packet.
- Visibility narrowed to the crate. The video decoding logic is
  unchanged: it decodes all 39 Majesty videos bit-exact with ffmpeg.
