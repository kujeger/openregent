//! Damaged or crafted `.bik` bytes: each must give an error, never a
//! panic or an allocation out of all proportion to the file.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Records the largest single allocation asked for.
struct Largest;

static LARGEST: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Largest {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        LARGEST.fetch_max(size, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOC: Largest = Largest;

/// No crafted file here is near this size; the decoder's own buffers
/// for a 16×16 picture are far below it.
const ALLOC_LIMIT: usize = 64 << 20;

const HEADER_LEN: usize = 44;
const WIDTH: u32 = 16;
const HEIGHT: u32 = 16;
const MONO_22K: u32 = 22_050;

/// A Bink of revision `h` (no leading video word), 16×16, stating
/// `frames` frames and holding `packets`, with one mono RDFT audio
/// track when `audio` is set. Each packet is a frame's whole data.
fn bik(frames: u32, audio: Option<u32>, packets: &[Vec<u8>]) -> Vec<u8> {
    let tracks = u32::from(audio.is_some());
    let index_at = HEADER_LEN + 3 * 4 * tracks as usize;
    let data_at = index_at + 4 * (packets.len() + 1);
    let data_len: usize = packets.iter().map(Vec::len).sum();
    let size = u32::try_from(data_at + data_len).unwrap();

    let mut out = b"BIKh".to_vec();
    let words = [size - 8, frames, 1, frames, WIDTH, HEIGHT, 15, 1, 0, tracks];

    out.extend(words.iter().flat_map(|w| w.to_le_bytes()));

    // Its largest frame, its rate and flags, its id.
    if let Some(rate) = audio {
        out.extend([0u32, rate, 0].iter().flat_map(|w| w.to_le_bytes()));
    }

    let mut at = data_at;
    for packet in packets {
        out.extend(u32::try_from(at).unwrap().to_le_bytes());
        at += packet.len();
    }
    out.extend(u32::try_from(at).unwrap().to_le_bytes());

    out.extend(packets.concat());
    out
}

/// Asserts the largest allocation so far stayed under the limit.
fn modest() {
    let largest = LARGEST.load(Ordering::Relaxed);
    assert!(largest < ALLOC_LIMIT, "allocated {largest} bytes");
}

#[test]
fn a_frame_count_past_the_file_is_an_error_without_a_huge_index() {
    let file = bik(u32::MAX, None, &[vec![0; 16]]);

    assert!(bink::Video::open(file.as_slice()).is_err());
    modest();
}

#[test]
fn a_tree_with_repeated_symbols_is_an_error() {
    // The first tree: id 1, an explicit list (1) of len 7 (0b111), then
    // eight symbols all 0. Fifteen symbols are then missing from it, past
    // its sixteen slots.
    let mut packet = vec![0xF1];
    packet.resize(16, 0);
    let file = bik(1, None, &[packet]);

    let mut video = bink::Video::open(file.as_slice()).unwrap();
    assert!(video.next_frame().unwrap().is_err());
}

#[test]
fn an_audio_packet_stating_huge_samples_is_an_error_without_allocating_them() {
    // The frame: the audio packet's size, then its sample count, all
    // ones, then a few bytes for it and for the video.
    let mut frame = 12u32.to_le_bytes().to_vec();
    frame.extend(u32::MAX.to_le_bytes());
    frame.resize(32, 0);
    let file = bik(1, Some(MONO_22K), &[frame]);

    let mut audio = bink::Audio::open(file.as_slice()).unwrap();
    assert!(audio.next_packet().unwrap().is_err());
    modest();
}
