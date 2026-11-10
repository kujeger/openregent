//! Records by agent id, found by index, not by hash or tree: ids only
//! grow and are never reused, so the table pages them, and keeps the
//! records in a store whose slots a removal frees for the next insert.
//!
//! ```text
//!  id ──> pages[id >> 10] ──> slot + 1 (0: none) ──> store[slot]
//!          1,024 ids a page,                          records, reused
//!          dropped once empty                         through `free`
//! ```
//!
//! Memory follows the records alive, not the ids ever given: a page
//! whose ids are all gone is dropped, so a long game keeps at most a
//! page per live record. Pages and the store's chunks are shared by a
//! clone and copied only where a write lands ([`Chunks`]).
//!
//! A GPL id is an `i32`: one at or above [`FAR`] as a `u32` is negative,
//! a script's own (`build::PREVIEW_ID`, -6), never issued. Those few are
//! kept apart, so the pages stay as many as the ids issued need.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::chunks::Chunks;

/// Ids a page: a power of two, so an id splits by shift and mask.
const SHIFT: u32 = 10;
const PAGE: usize = 1 << SHIFT;
const MASK: u32 = (1 << SHIFT) - 1;

/// The first id kept apart from the pages: `i32` -1 down to `i32::MIN`.
const FAR: u32 = 1 << 31;

/// One page's ids: each its store slot plus one, 0 for none.
#[derive(Clone, Debug)]
struct Page {
    slots: [u32; PAGE],
    /// Ids on the page with a record.
    live: usize,
}

impl Page {
    const EMPTY: Page = Page {
        slots: [0; PAGE],
        live: 0,
    };
}

/// Records of `T` by id, iterated in id order.
#[derive(Clone, Debug)]
pub(crate) struct IdTable<T> {
    pages: Vec<Option<Arc<Page>>>,
    /// Store slots of ids at or above [`FAR`].
    far: BTreeMap<u32, u32>,
    store: Chunks<Option<T>>,
    /// Store slots a removal freed, the last freed reused first.
    free: Vec<u32>,
}

/// Empty, whatever `T`: a derive would ask `T: Default`.
impl<T> Default for IdTable<T> {
    fn default() -> IdTable<T> {
        IdTable {
            pages: Vec::new(),
            far: BTreeMap::new(),
            store: Chunks::default(),
            free: Vec::new(),
        }
    }
}

/// Id `id`'s page and place on it.
const fn split(id: u32) -> (usize, usize) {
    ((id >> SHIFT) as usize, (id & MASK) as usize)
}

impl<T: Clone> IdTable<T> {
    /// Id `id`'s store slot, if it has a record.
    fn slot(&self, id: u32) -> Option<usize> {
        if id >= FAR {
            self.far.get(&id).map(|s| *s as usize)
        } else {
            let (page, at) = split(id);
            let held = self.pages.get(page)?.as_ref()?.slots[at];

            held.checked_sub(1).map(|s| s as usize)
        }
    }

    /// Id `id`'s record.
    pub(crate) fn get(&self, id: u32) -> Option<&T> {
        self.slot(id).and_then(|s| self.store.get(s).as_ref())
    }

    /// Id `id`'s record, to change.
    pub(crate) fn get_mut(&mut self, id: u32) -> Option<&mut T> {
        self.slot(id).and_then(|s| self.store.get_mut(s).as_mut())
    }

    /// Id `id`'s record is `record`: the one before, if any.
    pub(crate) fn insert(&mut self, id: u32, record: T) -> Option<T> {
        if let Some(s) = self.slot(id) {
            self.store.get_mut(s).replace(record)
        } else {
            self.add(id, record);
            None
        }
    }

    /// Id `id`, with no record, holds `record`: its store slot.
    fn add(&mut self, id: u32, record: T) -> usize {
        let slot = self.take_slot(record);
        let (page, at) = split(id);

        if id >= FAR {
            self.far.insert(id, slot);
        } else {
            if self.pages.len() <= page {
                self.pages.resize(page + 1, None);
            }

            let page = self.pages[page].get_or_insert_with(|| Arc::new(Page::EMPTY));
            let page = Arc::make_mut(page);

            page.slots[at] = slot + 1;
            page.live += 1;
        }

        slot as usize
    }

    /// A store slot holding `record`: a freed one, else a new one.
    fn take_slot(&mut self, record: T) -> u32 {
        if let Some(slot) = self.free.pop() {
            *self.store.get_mut(slot as usize) = Some(record);
            slot
        } else {
            let slot = u32::try_from(self.store.len()).expect("fewer records than ids");

            self.store.push(Some(record));
            slot
        }
    }

    /// Id `id`'s record, made with `Default` where it has none.
    pub(crate) fn entry(&mut self, id: u32) -> &mut T
    where
        T: Default,
    {
        let slot = match self.slot(id) {
            Some(s) => s,
            None => self.add(id, T::default()),
        };

        self.store.get_mut(slot).get_or_insert_with(T::default)
    }

    /// Id `id`'s record gone: what it was.
    pub(crate) fn remove(&mut self, id: u32) -> Option<T> {
        let slot = self.slot(id)?;
        let (page, at) = split(id);

        // The page dropped once its last id goes.
        if id >= FAR {
            self.far.remove(&id);
        } else if let Some(held) = self.pages[page].as_mut() {
            let p = Arc::make_mut(held);

            p.slots[at] = 0;
            p.live -= 1;

            if p.live == 0 {
                self.pages[page] = None;
            }
        }

        self.free.push(u32::try_from(slot).expect("a slot"));
        self.store.get_mut(slot).take()
    }

    /// Each id with a record and its store slot, in id order.
    fn slots(&self) -> impl Iterator<Item = (u32, usize)> + '_ {
        let far = self.far.iter().map(|(id, s)| (*id, *s as usize));

        self.pages
            .iter()
            .zip(0u32..)
            .filter_map(|(page, p)| page.as_ref().map(|page| (p, page)))
            .flat_map(|(p, page)| {
                page.slots
                    .iter()
                    .zip(0u32..)
                    .filter(|(held, _)| **held != 0)
                    .map(move |(held, at)| ((p << SHIFT) | at, *held as usize - 1))
            })
            .chain(far)
    }

    /// Each id with a record, in order.
    pub(crate) fn keys(&self) -> impl Iterator<Item = u32> + '_ {
        self.slots().map(|(id, _)| id)
    }

    /// Each id and its record, in id order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (u32, &T)> + '_ {
        self.slots()
            .filter_map(|(id, s)| self.store.get(s).as_ref().map(|r| (id, r)))
    }

    /// `change` on each record, in id order.
    pub(crate) fn each_mut(&mut self, mut change: impl FnMut(&mut T)) {
        let slots: Vec<usize> = self.slots().map(|(_, s)| s).collect();

        for s in slots {
            if let Some(r) = self.store.get_mut(s).as_mut() {
                change(r);
            }
        }
    }
}

impl<T: Clone> FromIterator<(u32, T)> for IdTable<T> {
    fn from_iter<I: IntoIterator<Item = (u32, T)>>(records: I) -> IdTable<T> {
        let mut table = IdTable::default();

        for (id, record) in records {
            table.insert(id, record);
        }

        table
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records come back by id, in id order, across pages.
    #[test]
    fn records_are_found_by_id_in_id_order() {
        let mut table: IdTable<&str> = [(2000, "c"), (5, "a"), (1023, "b")].into_iter().collect();

        assert_eq!(table.get(5), Some(&"a"));
        assert_eq!(table.get(6), None);
        assert_eq!(table.get(1_000_000), None);
        assert_eq!(table.insert(5, "A"), Some("a"));
        assert_eq!(
            table.iter().collect::<Vec<_>>(),
            vec![(5, &"A"), (1023, &"b"), (2000, &"c")]
        );
    }

    /// A removed id's slot holds the next record; a page whose ids all
    /// went is dropped.
    #[test]
    fn a_removal_frees_its_slot_and_an_empty_page() {
        let mut table: IdTable<u32> = [(1, 10), (1500, 20)].into_iter().collect();

        assert_eq!(table.remove(1500), Some(20));
        assert_eq!(table.remove(1500), None);
        assert!(table.pages[1].is_none());

        table.insert(3000, 30);

        assert_eq!(table.store.len(), 2);
        assert_eq!(table.keys().collect::<Vec<_>>(), vec![1, 3000]);
    }

    /// A clone shares, and a write to one leaves the other as it was.
    #[test]
    fn a_clone_is_apart_from_its_original() {
        let mut table: IdTable<u32> = [(7, 1)].into_iter().collect();
        let copy = table.clone();

        *table.entry(7) += 1;
        *table.entry(8) += 5;

        assert_eq!(copy.iter().collect::<Vec<_>>(), vec![(7, &1)]);
        assert_eq!(table.iter().collect::<Vec<_>>(), vec![(7, &2), (8, &5)]);
    }

    /// Many ids come and go, one alive at a time: the table keeps a page
    /// and a slot, not one for each id ever given.
    #[test]
    fn a_long_run_of_ids_keeps_what_is_alive() {
        let mut table = IdTable::default();

        for id in 0..100_000 {
            table.insert(id, id);

            if id > 0 {
                table.remove(id - 1);
            }
        }

        assert_eq!(table.store.len(), 2);
        assert_eq!(table.pages.iter().flatten().count(), 1);
    }

    /// A negative GPL id (the build preview's, -6) makes no page: the
    /// pages stay as many as the issued ids need, and it comes last.
    #[test]
    fn a_negative_id_is_kept_apart() {
        let preview = (-6i32).cast_unsigned();
        let mut table: IdTable<u32> = [(preview, 1), (3, 2)].into_iter().collect();

        assert_eq!(table.pages.len(), 1);
        assert_eq!(table.get(preview), Some(&1));
        assert_eq!(table.keys().collect::<Vec<_>>(), vec![3, preview]);
        assert_eq!(table.remove(preview), Some(1));
        assert_eq!(table.get(preview), None);
    }
}
