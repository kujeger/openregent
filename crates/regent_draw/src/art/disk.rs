//! The disk cache: upscaled pictures as LZ4-compressed RGBA, keyed by the
//! picture's own pixels, so a changed archive, tone or compositing makes a
//! new file; [`VERSION`] changes with the method or the format.
//!
//! ```text
//!  <dir>/xbrz-v1/<hash>-<w>x<h>-<edge>-x<factor>.lz4   (`Art::cached`'s dir)
//!
//!  file   "MJAC" version width height  edge factor 0 0  parts
//!         per part: x y width height bytes
//!         per part: its LZ4 block
//!         u32 little-endian, but edge and factor: u8
//!  block  the part's pixels as egui holds them, premultiplied RGBA:
//!         a picture reads back as it was made
//! ```
//!
//! A picture is stored in parts of at most [`SIDE`] a side, read in
//! parallel; the quest map's levels as their tiles and lit patches
//! (`levels`).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy_egui::egui::{Color32, ColorImage, Rect};
use regent_core::data::fnv;

use super::scale::{self, Edge};
use super::shrink;

/// The method's version, in the cache's path and each file: bump on a
/// change to the padding, the alpha handling, the crate or the format.
pub const VERSION: u32 = 1;

/// A file's first bytes.
const MAGIC: [u8; 4] = *b"MJAC";

/// A part's most pixels a side.
pub const SIDE: usize = 1024;

/// Bytes before the parts' table: magic, version, width, height, edge,
/// factor, two zero bytes, the parts' count.
const HEAD: usize = 24;

/// Bytes of a part's entry in the table: x, y, width, height, block
/// bytes.
const ENTRY: usize = 20;

/// Files read from the cache, and written to it, this run.
static READ: AtomicUsize = AtomicUsize::new(0);
static WRITTEN: AtomicUsize = AtomicUsize::new(0);

/// Files read from the cache and written to it so far: the pictures
/// read, the pictures and lit tables written.
pub fn counts() -> [usize; 2] {
    [
        READ.load(Ordering::Relaxed),
        WRITTEN.load(Ordering::Relaxed),
    ]
}

/// A picture as read, and its pixels' hash.
#[derive(Clone)]
pub struct Original {
    pub image: Arc<ColorImage>,
    pub hash: u64,
}

impl Original {
    #[must_use]
    pub fn new(image: ColorImage) -> Original {
        let hash = hash(&image);

        Original {
            image: Arc::new(image),
            hash,
        }
    }
}

/// `image`'s size and pixels hashed, FNV-1a.
pub fn hash(image: &ColorImage) -> u64 {
    let size = image.size.iter().flat_map(|n| n.to_le_bytes());
    let pixels = image.pixels.iter().flat_map(Color32::to_array);

    fnv::fold(fnv::START, size.chain(pixels))
}

/// One upscaled picture's file in the cache at `dir`.
///
/// ```text
///  <dir>/xbrz-v1/<hash>-<w>x<h>-<edge>-x<factor>.lz4
/// ```
#[must_use]
pub fn file(dir: &Path, original: &Original, edge: Edge, factor: usize) -> PathBuf {
    let [w, h] = original.image.size;
    let name = format!(
        "{:016x}-{w}x{h}-{}-x{factor}.lz4",
        original.hash,
        edge.name()
    );

    folder(dir).join(name)
}

/// This [`VERSION`]'s folder in the cache at `dir`: another version's
/// files are never read.
#[must_use]
pub fn folder(dir: &Path) -> PathBuf {
    dir.join(format!("xbrz-v{VERSION}"))
}

/// The tests' cache: beside the test binary, not the user's.
///
/// # Panics
///
/// With no path to the test binary.
#[cfg(test)]
pub fn tests() -> PathBuf {
    std::env::current_exe()
        .expect("the test binary")
        .with_file_name("art-cache")
}

/// What a file says of its picture besides its parts: checked on reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// The whole picture's size.
    pub size: [usize; 2],
    pub edge: Edge,
    /// Its xBRZ factor; 1 none, 0 shrunk.
    pub factor: usize,
}

impl Header {
    /// Edge and factor as the file's two bytes.
    fn way(self) -> Option<[u8; 2]> {
        let edge = match self.edge {
            Edge::Clamp => 0,
            Edge::Repeat => 1,
        };

        Some([edge, u8::try_from(self.factor).ok()?])
    }
}

/// A piece of a picture: its corner in the whole, and its pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub at: [usize; 2],
    pub image: ColorImage,
}

/// `image` in parts of at most `side` a side, rows first.
#[must_use]
pub fn split(image: &ColorImage, side: usize) -> Vec<Part> {
    let starts = |n: usize| {
        (0..n)
            .step_by(side.max(1))
            .map(move |a| (a, side.min(n - a)))
    };
    let [w, h] = image.size;

    starts(h)
        .flat_map(|(y, ph)| starts(w).map(move |(x, pw)| ([x, y], [pw, ph])))
        .map(|(at, size)| Part {
            at,
            image: scale::cut(image, at, size),
        })
        .collect()
}

/// A `size` picture put together from `parts`; one part covering it is
/// taken as it is.
#[must_use]
pub fn whole(size: [usize; 2], mut parts: Vec<Part>) -> ColorImage {
    let one = parts.len() == 1 && parts[0].at == [0, 0] && parts[0].image.size == size;

    if one {
        parts.swap_remove(0).image
    } else {
        let mut image = ColorImage::filled(size, Color32::TRANSPARENT);

        for part in &parts {
            let [pw, ph] = part.image.size;

            for (y, row) in part
                .image
                .pixels
                .chunks_exact(pw.max(1))
                .take(ph)
                .enumerate()
            {
                let to = (part.at[1] + y) * size[0] + part.at[0];

                image.pixels[to..to + pw].copy_from_slice(row);
            }
        }

        image
    }
}

/// `f` of each of `items`, in order, spread over the machine's threads.
pub fn parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let chunk = items.len().div_ceil(threads).max(1);

    if items.len() <= 1 {
        items.iter().map(f).collect()
    } else {
        std::thread::scope(|s| {
            let f = &f;
            let handles: Vec<_> = items
                .chunks(chunk)
                .map(|c| s.spawn(move || c.iter().map(f).collect::<Vec<R>>()))
                .collect();

            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
                .collect()
        })
    }
}

/// A count or size as a file's `u32`.
fn word(n: usize) -> [u8; 4] {
    u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes()
}

/// `header` and `parts` as a file's bytes, the parts compressed in
/// parallel; `None` with a factor past a byte.
#[must_use]
pub fn encode(header: Header, parts: &[Part]) -> Option<Vec<u8>> {
    let way = header.way()?;
    let blocks = parallel(parts, |p| {
        lz4_flex::block::compress(bytemuck::cast_slice(&p.image.pixels))
    });
    let head = [MAGIC, VERSION.to_le_bytes()]
        .into_iter()
        .chain(header.size.map(word))
        .flatten()
        .chain(way)
        .chain([0, 0])
        .chain(word(parts.len()));
    let table = parts.iter().zip(&blocks).flat_map(|(p, b)| {
        [p.at[0], p.at[1], p.image.size[0], p.image.size[1], b.len()]
            .into_iter()
            .flat_map(word)
    });

    let mut bytes: Vec<u8> = head.chain(table).collect();

    for block in &blocks {
        bytes.extend_from_slice(block);
    }

    Some(bytes)
}

/// The parts of a file's `bytes`, if it is this [`VERSION`]'s and its
/// header is `header`; decompressed in parallel.
#[must_use]
pub fn decode(bytes: &[u8], header: Header) -> Option<Vec<Part>> {
    let at = |i: usize| -> Option<usize> {
        let word: [u8; 4] = bytes.get(i..i + 4)?.try_into().ok()?;

        usize::try_from(u32::from_le_bytes(word)).ok()
    };
    let way = header.way()?;
    let fits = bytes.get(..4) == Some(&MAGIC)
        && bytes.get(4..8) == Some(&VERSION.to_le_bytes())
        && [at(8), at(12)] == header.size.map(Some)
        && bytes.get(16..20) == Some(&[way[0], way[1], 0, 0]);
    let count = at(20).filter(|_| fits)?;
    let entries = (0..count)
        .map(|i| {
            let e = HEAD + i * ENTRY;

            Some([at(e)?, at(e + 4)?, at(e + 8)?, at(e + 12)?, at(e + 16)?])
        })
        .collect::<Option<Vec<_>>>()?;

    // Each part inside the picture: `whole` copies it there unchecked.
    let inside = |e: &[usize; 5]| {
        let ends = [e[0].checked_add(e[2]), e[1].checked_add(e[3])];

        ends.iter()
            .zip(header.size)
            .all(|(end, side)| end.is_some_and(|end| end <= side))
    };
    let entries = Some(entries).filter(|e| e.iter().all(inside))?;
    let blocks = entries
        .iter()
        .scan(HEAD + count * ENTRY, |start, e| {
            let block = bytes.get(*start..*start + e[4]);

            *start += e[4];
            Some(block)
        })
        .collect::<Option<Vec<&[u8]>>>()?;
    let jobs: Vec<_> = entries.iter().zip(blocks).collect();

    parallel(&jobs, |(e, block)| {
        let size = [e[2], e[3]];
        let mut pixels = vec![Color32::TRANSPARENT; size[0] * size[1]];
        let rgba: &mut [u8] = bytemuck::cast_slice_mut(&mut pixels);
        let n = lz4_flex::block::decompress_into(block, rgba).ok()?;

        (n == rgba.len()).then(|| Part {
            at: [e[0], e[1]],
            image: ColorImage::new(size, pixels),
        })
    })
    .into_iter()
    .collect()
}

/// The parts of the file at `path`, if it reads and is `header`'s.
pub fn read(path: &Path, header: Header) -> Option<Vec<Part>> {
    let parts = decode(&std::fs::read(path).ok()?, header)?;

    READ.fetch_add(1, Ordering::Relaxed);
    Some(parts)
}

/// `parts` and `header` written to `path`; a failure logged.
pub fn write(path: &Path, header: Header, parts: &[Part]) {
    let stored = encode(header, parts)
        .ok_or_else(|| format!("{}: factor {}", path.display(), header.factor))
        .and_then(|bytes| store(path, &bytes));

    if let Err(e) = stored {
        tracing::warn!("upscale: {e}");
    }
}

/// `original` upscaled `(edge, factor)` as cached at `path`, if it reads.
fn load(path: &Path, original: &Original, (edge, factor): (Edge, usize)) -> Option<ColorImage> {
    let header = Header {
        size: original.image.size.map(|n| n * factor),
        edge,
        factor,
    };

    read(path, header).map(|parts| whole(header.size, parts))
}

/// `bytes` written to `path` by way of a file beside it, so a reader
/// never sees half.
///
/// # Errors
///
/// A directory, file or rename that fails, said with the path.
pub fn store(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let shown = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let part = path.with_extension(format!("{}.part", std::process::id()));

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| shown(&e))?;
    }

    std::fs::write(&part, bytes).map_err(|e| shown(&e))?;
    std::fs::rename(&part, path).map_err(|e| shown(&e))?;
    WRITTEN.fetch_add(1, Ordering::Relaxed);

    Ok(())
}

/// `original` upscaled `factor` times, from the cache at `dir` or made
/// and put there.
pub fn cached(
    original: &Original,
    (edge, factor): (Edge, usize),
    dir: Option<&Path>,
) -> ColorImage {
    let path = dir.map(|d| file(d, original, edge, factor));

    path.as_deref()
        .and_then(|p| load(p, original, (edge, factor)))
        .unwrap_or_else(|| {
            let big = scale::xbrz(&original.image, factor, edge);
            let header = Header {
                size: big.size,
                edge,
                factor,
            };

            if let Some(p) = &path {
                write(p, header, &split(&big, SIDE));
            }

            big
        })
}

/// `original` upscaled `factor` times, from the cache at `dir` or made
/// and put there, then its part `cut` (its own pixels) shrunk to `pixels`.
pub fn made(
    original: &Original,
    (edge, factor): (Edge, usize),
    (pixels, cut): ([usize; 2], Rect),
    dir: Option<&Path>,
) -> ColorImage {
    sized(
        &cached(original, (edge, factor), dir),
        factor,
        (pixels, cut),
    )
}

/// `big`, a picture upscaled `factor` times, its part `cut` (the
/// picture's own pixels) shrunk to `pixels`.
fn sized(big: &ColorImage, factor: usize, (pixels, cut): ([usize; 2], Rect)) -> ColorImage {
    shrink::part(big, scaled(cut, factor), pixels)
}

/// `cut`, a picture's own pixels, in the picture upscaled `factor` times.
#[must_use]
pub fn scaled(cut: Rect, factor: usize) -> Rect {
    #[allow(clippy::cast_precision_loss, reason = "a factor")]
    let f = factor as f32;

    Rect::from_min_max(cut.min * f, cut.max * f)
}

/// Whether another size of a picture is drawn while a new one is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Other {
    Ready,
    Missing,
}

/// `original` upscaled `factor` times now, to shrink ([`scaled`]), where
/// the cache at `dir` holds it and no `other` size stands in: a page's
/// first frame. `None`: make it in a task, as on a resize or a first
/// upscale.
pub fn at_once(
    original: &Original,
    (edge, factor): (Edge, usize),
    (dir, other): (Option<&Path>, Other),
) -> Option<ColorImage> {
    let path = dir
        .map(|d| file(d, original, edge, factor))
        .filter(|p| other == Other::Missing && p.is_file());

    path.as_deref()
        .and_then(|p| load(p, original, (edge, factor)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::scale::tests::sample;

    #[test]
    fn the_cache_file_changes_with_pixels_edge_factor_and_version() {
        let dir = Path::new("cache");
        let one = Original::new(sample(4, 4));
        let mut changed = sample(4, 4);

        changed.pixels[5] = Color32::BLACK;

        let two = Original::new(changed);
        let path = file(dir, &one, Edge::Clamp, 2);

        assert_eq!(
            path,
            file(dir, &Original::new(sample(4, 4)), Edge::Clamp, 2)
        );
        assert_ne!(path, file(dir, &two, Edge::Clamp, 2));
        assert_ne!(path, file(dir, &one, Edge::Repeat, 2));
        assert_ne!(path, file(dir, &one, Edge::Clamp, 3));
        assert!(
            path.starts_with(format!("cache/xbrz-v{VERSION}")),
            "{path:?}"
        );
    }

    /// A page's first frame: a cached picture with no other size to stand
    /// in is ready at once; otherwise it waits for a task.
    #[test]
    fn a_cached_picture_with_no_stand_in_is_ready_at_once() {
        let dir = std::env::current_exe()
            .expect("the test binary")
            .with_file_name("upscale-at-once");
        let original = Original::new(sample(6, 5));
        let way = (Edge::Clamp, 2);
        let now = |other| at_once(&original, way, (Some(&dir), other));

        let _ = std::fs::remove_dir_all(&dir);
        assert!(now(Other::Missing).is_none(), "not cached");

        let big = cached(&original, way, Some(&dir));

        assert_eq!(now(Other::Missing), Some(big));
        assert!(now(Other::Ready).is_none(), "another size stands in");
    }

    /// A see-through pixel and a part-transparent one included: the parts
    /// come back as made.
    #[test]
    fn a_cached_picture_reads_back_as_written() {
        let mut image = sample(5, 3);

        image.pixels[0] = Color32::TRANSPARENT;
        image.pixels[1] = Color32::from_rgba_unmultiplied(200, 100, 50, 77);

        let header = Header {
            size: image.size,
            edge: Edge::Repeat,
            factor: 3,
        };
        let parts = split(&image, 2);
        let bytes = encode(header, &parts).expect("encodes");
        let read = decode(&bytes, header).expect("decodes");

        assert_eq!(&bytes[..4], b"MJAC");
        assert_eq!(read, parts);
        assert_eq!(whole(image.size, read), image);
    }

    /// A file whose part lies past the picture is not read: `whole`
    /// would copy it past the pixels.
    #[test]
    fn a_part_past_the_picture_is_ignored() {
        let image = sample(4, 4);
        let header = Header {
            size: image.size,
            edge: Edge::Clamp,
            factor: 2,
        };
        let mut parts = split(&image, 2);

        parts[3].at = [3, 2];

        let bytes = encode(header, &parts).expect("encodes");

        assert!(decode(&bytes, header).is_none());
    }

    /// Another version's file is not read, nor one of another size, edge
    /// or factor, nor a cut one.
    #[test]
    fn a_file_of_another_version_or_header_is_ignored() {
        let image = sample(6, 4);
        let header = Header {
            size: image.size,
            edge: Edge::Clamp,
            factor: 2,
        };
        let bytes = encode(header, &split(&image, SIDE)).expect("encodes");
        let mut old = bytes.clone();

        old[4..8].copy_from_slice(&(VERSION + 1).to_le_bytes());

        assert!(decode(&bytes, header).is_some());
        assert!(decode(&old, header).is_none(), "another version");
        assert!(decode(&bytes[..bytes.len() - 1], header).is_none(), "cut");

        let others = [
            Header {
                size: [6, 5],
                ..header
            },
            Header {
                edge: Edge::Repeat,
                ..header
            },
            Header {
                factor: 4,
                ..header
            },
        ];

        for other in others {
            assert!(decode(&bytes, other).is_none(), "{other:?}");
        }
    }

    /// An odd-sized picture's parts put back together are the picture;
    /// each at most the side asked, rows first.
    #[test]
    fn parts_put_together_are_the_whole_picture() {
        let image = sample(7, 5);

        for side in [1, 2, 3, 7, SIDE] {
            let parts = split(&image, side);

            assert!(
                parts
                    .iter()
                    .all(|p| p.image.size.iter().all(|&n| n <= side))
            );
            assert_eq!(whole(image.size, parts), image, "{side}");
        }

        assert_eq!(split(&image, 3)[1].at, [3, 0]);
    }
}
