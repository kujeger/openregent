//! A vector kept in shared chunks: a clone shares every chunk, and a
//! write copies only the chunk it lands in, if shared.
//!
//! ```text
//!  world        [c0][c1][c2][c3]          a clone: four Arcs counted up
//!  its copy      │   │   │   │
//!               [c0][c1'][c2][c3]         a write to c1: c1 copied first
//! ```
//!
//! The collision grid's: 1M heads and 130k occupants on a 512-cell map,
//! 7 MB copied whole each time the drawing took the world; a frame's
//! moves change about a dozen of its chunks. The explored ground's too:
//! 263,000 flags, a reveal a few chunks.

use std::sync::Arc;

/// Items a chunk: a power of two, so an index splits by shift and mask.
const SHIFT: u32 = 10;
const CHUNK: usize = 1 << SHIFT;
const MASK: usize = CHUNK - 1;

/// A vector of `T` in shared chunks of [`CHUNK`]: each full but the last.
#[derive(Clone, Debug)]
pub(crate) struct Chunks<T> {
    chunks: Vec<Arc<Vec<T>>>,
    len: usize,
}

/// Empty, whatever `T`: a derive would ask `T: Default`.
impl<T> Default for Chunks<T> {
    fn default() -> Chunks<T> {
        Chunks {
            chunks: Vec::new(),
            len: 0,
        }
    }
}

impl<T: Clone> Chunks<T> {
    /// `len` copies of `item`.
    pub(crate) fn filled(item: T, len: usize) -> Chunks<T> {
        let chunks = (0..len.div_ceil(CHUNK))
            .map(|c| Arc::new(vec![item.clone(); CHUNK.min(len - c * CHUNK)]))
            .collect();

        Chunks { chunks, len }
    }

    /// `items`, in chunks.
    pub(crate) fn of(items: &[T]) -> Chunks<T> {
        let chunks = items.chunks(CHUNK).map(|c| Arc::new(c.to_vec())).collect();

        Chunks {
            chunks,
            len: items.len(),
        }
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
    }

    /// Item `i`.
    ///
    /// # Panics
    ///
    /// `i` past the end.
    pub(crate) fn get(&self, i: usize) -> &T {
        &self.chunks[i >> SHIFT][i & MASK]
    }

    /// Item `i`, to change: its chunk copied first if shared.
    ///
    /// # Panics
    ///
    /// `i` past the end.
    pub(crate) fn get_mut(&mut self, i: usize) -> &mut T {
        &mut Arc::make_mut(&mut self.chunks[i >> SHIFT])[i & MASK]
    }

    /// `item` at the end.
    pub(crate) fn push(&mut self, item: T) {
        if self.len & MASK == 0 {
            self.chunks.push(Arc::new(Vec::with_capacity(CHUNK)));
        }

        if let Some(last) = self.chunks.last_mut() {
            Arc::make_mut(last).push(item);
        }

        self.len += 1;
    }

    /// Each item, in order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.chunks.iter().flat_map(|c| c.iter())
    }
}

impl<T: PartialEq> Chunks<T> {
    /// Each chunk that differs from `was`'s at its place: its first
    /// index, then `was`'s and this one's. A chunk both still share is
    /// passed over unread; past the shorter's end, nothing.
    pub(crate) fn differing<'a>(
        &'a self,
        was: &'a Chunks<T>,
    ) -> impl Iterator<Item = (usize, &'a [T], &'a [T])> {
        (0..)
            .step_by(CHUNK)
            .zip(was.chunks.iter().zip(&self.chunks))
            .filter(|(_, (a, b))| !Arc::ptr_eq(a, b) && a != b)
            .map(|(first, (a, b))| (first, a.as_slice(), b.as_slice()))
    }
}

/// Equal items, chunks shared or not.
impl<T: PartialEq> PartialEq for Chunks<T> {
    fn eq(&self, other: &Chunks<T>) -> bool {
        self.len == other.len && self.differing(other).next().is_none()
    }
}

impl<T: Eq> Eq for Chunks<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clone shares every chunk; a write to either side copies the
    /// chunk it lands in and leaves the other side as it was.
    #[test]
    fn a_write_after_a_clone_leaves_the_clone_alone() {
        let mut kept = Chunks::filled(0_u32, CHUNK * 2 + 5);
        let copy = kept.clone();

        assert!(
            kept.chunks
                .iter()
                .zip(&copy.chunks)
                .all(|(a, b)| Arc::ptr_eq(a, b))
        );

        *kept.get_mut(CHUNK + 3) = 7;

        assert_eq!((*kept.get(CHUNK + 3), *copy.get(CHUNK + 3)), (7, 0));
        assert!(!Arc::ptr_eq(&kept.chunks[1], &copy.chunks[1]), "copied");
        assert!(
            Arc::ptr_eq(&kept.chunks[0], &copy.chunks[0]),
            "still shared"
        );
        assert!(
            Arc::ptr_eq(&kept.chunks[2], &copy.chunks[2]),
            "still shared"
        );
    }

    /// Pushes fill the last chunk, then begin another; a clone taken
    /// between keeps its own length and items.
    #[test]
    fn pushes_fill_chunks_in_order() {
        let mut list = Chunks::default();

        for i in 0..=CHUNK {
            list.push(i);
        }

        let copy = list.clone();

        list.push(CHUNK + 1);
        *list.get_mut(0) = 99;

        assert_eq!((list.len(), list.chunks.len()), (CHUNK + 2, 2));
        assert_eq!(copy.len(), CHUNK + 1);
        assert!(copy.iter().copied().eq(0..=CHUNK));
        assert_eq!(*list.get(CHUNK + 1), CHUNK + 1);
        assert_eq!(*list.get(0), 99);
    }

    /// Only the chunk written differs; shared chunks are passed over,
    /// and a copied chunk back to its old items differs no more.
    #[test]
    fn a_write_makes_only_its_chunk_differ() {
        let was = Chunks::of(&[0_u8; CHUNK * 3]);
        let mut now = was.clone();

        *now.get_mut(CHUNK + 5) = 1;

        let differ: Vec<usize> = now.differing(&was).map(|(first, ..)| first).collect();

        assert_eq!(differ, [CHUNK]);
        assert_ne!(now, was);

        *now.get_mut(CHUNK + 5) = 0;

        assert_eq!(now.differing(&was).count(), 0);
        assert_eq!(now, was);
        assert!(Chunks::of(&[1_u8, 2]).iter().copied().eq([1, 2]));
    }
}
