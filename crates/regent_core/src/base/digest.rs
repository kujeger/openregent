//! Hashing that gives the same answer on every run and machine: the maps
//! play keeps, and a digest of a game's state.
//!
//! ```text
//!  Map, Set ── HashMap, HashSet on Steady: iteration order fixed by
//!              keys and history, never by the run
//!  Fold     ── the hash: integers as values, so endianness is moot
//!  digest   ── any serde value folded into one u64 (a Snapshot's parts)
//! ```
//!
//! `std`'s `RandomState` keys every map afresh, so a map's iteration
//! order differs between runs and between peers. Play must not depend on
//! it; [`Steady`] makes sure it cannot. To find a place that does depend
//! on it, set [`SHUFFLE`] (or call [`shuffle`] on a test's thread): each
//! map then takes random keys again, and a run-twice test tells.

use std::cell::Cell;
use std::collections::hash_map::RandomState;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{BuildHasher, Hasher};

use serde::Serialize;
use serde::ser::{self, Serializer};

use super::switch::Switch;

/// A hash map whose order does not change between runs.
pub type Map<K, V> = HashMap<K, V, Steady>;

/// A hash set whose order does not change between runs.
pub type Set<T> = HashSet<T, Steady>;

/// Set, every map takes random keys, as `std`'s do: order dependence
/// shows as a run that differs from the last.
pub static SHUFFLE: Switch = Switch::new("MAJESTY_SHUFFLE_MAPS");

thread_local! {
    /// Whether this thread's new maps take random keys ([`shuffle`]).
    static SHUFFLING: Cell<bool> = const { Cell::new(false) };
}

/// From now on, maps made on this thread take random keys: a run-twice
/// test's, so it keeps catching order dependence.
pub fn shuffle() {
    SHUFFLING.set(true);
}

/// Whether a new map takes random keys.
fn shuffling() -> bool {
    SHUFFLING.get() || SHUFFLE.on()
}

/// The map keys: zero, or random where `shuffling`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Steady(u64);

impl Default for Steady {
    fn default() -> Steady {
        if shuffling() {
            Steady(RandomState::new().hash_one(0u8))
        } else {
            Steady(0)
        }
    }
}

impl BuildHasher for Steady {
    type Hasher = Fold;

    fn build_hasher(&self) -> Fold {
        Fold(self.0)
    }
}

/// `FxHash`'s multiplier (`rustc-hash` 2).
const FOLD_PRIME: u64 = 0xf135_7aea_2e62_a9c5;

/// `MurmurHash3`'s 64-bit finaliser constants: they spread every input
/// bit over the output, so the table's low bits are good too.
const MIX: [u64; 2] = [0xff51_afd7_ed55_8ccd, 0xc4ce_b9fe_1a85_ec53];

/// A word at a time: `(state ^ word) * prime`, mixed at the end. Not for
/// adversarial keys; play's are ids and cells.
///
/// Integers are taken as values and bytes as little-endian words, so a
/// big-endian machine hashes alike.
#[derive(Clone, Copy, Debug, Default)]
pub struct Fold(u64);

impl Fold {
    /// A fold from `seed`: 0 for a digest.
    #[must_use]
    pub const fn new(seed: u64) -> Fold {
        Fold(seed)
    }

    const fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(FOLD_PRIME);
    }
}

impl Hasher for Fold {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0; 8];

            word[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }

    fn write_u16(&mut self, n: u16) {
        self.add(u64::from(n));
    }

    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }

    fn write_i8(&mut self, n: i8) {
        self.write_u8(n.cast_unsigned());
    }

    fn write_i16(&mut self, n: i16) {
        self.write_u16(n.cast_unsigned());
    }

    fn write_i32(&mut self, n: i32) {
        self.write_u32(n.cast_unsigned());
    }

    fn write_i64(&mut self, n: i64) {
        self.write_u64(n.cast_unsigned());
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "a 64-bit target's isize is i64"
    )]
    fn write_isize(&mut self, n: isize) {
        self.write_i64(n as i64);
    }

    fn finish(&self) -> u64 {
        let mut h = self.0;

        h ^= h >> 33;
        h = h.wrapping_mul(MIX[0]);
        h ^= h >> 33;
        h = h.wrapping_mul(MIX[1]);
        h ^ (h >> 33)
    }
}

/// `value` folded into one word: every number, string, length, variant
/// and `None` it serialises, in order. The same on every machine.
///
/// # Panics
///
/// A `Serialize` impl that fails: the save model's never do.
#[must_use]
pub fn digest<T: Serialize + ?Sized>(value: &T) -> u64 {
    let mut fold = Fold::new(0);

    value
        .serialize(&mut fold)
        .expect("the save model serialises");
    fold.finish()
}

/// What a digest cannot fold: a custom `Serialize` that failed.
#[derive(Debug)]
pub struct Unfolded(String);

impl fmt::Display for Unfolded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Unfolded {}

impl ser::Error for Unfolded {
    fn custom<T: fmt::Display>(msg: T) -> Unfolded {
        Unfolded(msg.to_string())
    }
}

/// Marks that keep `Some(0)` from `0`, a sequence from its neighbour.
#[derive(Clone, Copy)]
#[repr(u8)]
enum Mark {
    None,
    Some,
    Unit,
    Seq,
}

impl Fold {
    fn mark(&mut self, mark: Mark) {
        self.write_u8(mark as u8);
    }
}

impl<'a> Serializer for &'a mut Fold {
    type Ok = ();
    type Error = Unfolded;
    type SerializeSeq = &'a mut Fold;
    type SerializeTuple = &'a mut Fold;
    type SerializeTupleStruct = &'a mut Fold;
    type SerializeTupleVariant = &'a mut Fold;
    type SerializeMap = &'a mut Fold;
    type SerializeStruct = &'a mut Fold;
    type SerializeStructVariant = &'a mut Fold;

    fn serialize_bool(self, v: bool) -> Result<(), Unfolded> {
        self.write_u8(u8::from(v));
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<(), Unfolded> {
        self.write_i8(v);
        Ok(())
    }

    fn serialize_i16(self, v: i16) -> Result<(), Unfolded> {
        self.write_i16(v);
        Ok(())
    }

    fn serialize_i32(self, v: i32) -> Result<(), Unfolded> {
        self.write_i32(v);
        Ok(())
    }

    fn serialize_i64(self, v: i64) -> Result<(), Unfolded> {
        self.write_i64(v);
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<(), Unfolded> {
        self.write_u8(v);
        Ok(())
    }

    fn serialize_u16(self, v: u16) -> Result<(), Unfolded> {
        self.write_u16(v);
        Ok(())
    }

    fn serialize_u32(self, v: u32) -> Result<(), Unfolded> {
        self.write_u32(v);
        Ok(())
    }

    fn serialize_u64(self, v: u64) -> Result<(), Unfolded> {
        self.write_u64(v);
        Ok(())
    }

    // Floats by their bits: equal bits, equal state.
    fn serialize_f32(self, v: f32) -> Result<(), Unfolded> {
        self.write_u32(v.to_bits());
        Ok(())
    }

    fn serialize_f64(self, v: f64) -> Result<(), Unfolded> {
        self.write_u64(v.to_bits());
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<(), Unfolded> {
        self.write_u32(u32::from(v));
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<(), Unfolded> {
        self.serialize_bytes(v.as_bytes())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<(), Unfolded> {
        self.write_usize(v.len());
        self.write(v);
        Ok(())
    }

    fn serialize_none(self) -> Result<(), Unfolded> {
        self.mark(Mark::None);
        Ok(())
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Unfolded> {
        self.mark(Mark::Some);
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<(), Unfolded> {
        self.mark(Mark::Unit);
        Ok(())
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Unfolded> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        index: u32,
        _: &'static str,
    ) -> Result<(), Unfolded> {
        self.write_u32(index);
        Ok(())
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Unfolded> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        index: u32,
        _: &'static str,
        value: &T,
    ) -> Result<(), Unfolded> {
        self.write_u32(index);
        value.serialize(self)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self, Unfolded> {
        self.mark(Mark::Seq);
        self.write_usize(len.unwrap_or(usize::MAX));
        Ok(self)
    }

    fn serialize_tuple(self, _: usize) -> Result<Self, Unfolded> {
        Ok(self)
    }

    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self, Unfolded> {
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        index: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self, Unfolded> {
        self.write_u32(index);
        Ok(self)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self, Unfolded> {
        self.serialize_seq(len)
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self, Unfolded> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        index: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self, Unfolded> {
        self.write_u32(index);
        Ok(self)
    }
}

/// The compound parts: each element folded in turn, nothing at the end.
macro_rules! elements {
    ($($trait:ident :: $method:ident),* $(,)?) => {$(
        impl ser::$trait for &mut Fold {
            type Ok = ();
            type Error = Unfolded;

            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Unfolded> {
                value.serialize(&mut **self)
            }

            fn end(self) -> Result<(), Unfolded> {
                Ok(())
            }
        }
    )*};
}

elements!(
    SerializeSeq::serialize_element,
    SerializeTuple::serialize_element,
    SerializeTupleStruct::serialize_field,
    SerializeTupleVariant::serialize_field,
);

impl ser::SerializeMap for &mut Fold {
    type Ok = ();
    type Error = Unfolded;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Unfolded> {
        key.serialize(&mut **self)
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Unfolded> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<(), Unfolded> {
        Ok(())
    }
}

/// Struct fields by order, not name: the struct's shape is the build's.
macro_rules! fields {
    ($($trait:ident),* $(,)?) => {$(
        impl ser::$trait for &mut Fold {
            type Ok = ();
            type Error = Unfolded;

            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                _: &'static str,
                value: &T,
            ) -> Result<(), Unfolded> {
                value.serialize(&mut **self)
            }

            fn end(self) -> Result<(), Unfolded> {
                Ok(())
            }
        }
    )*};
}

fields!(SerializeStruct, SerializeStructVariant);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::hash::Hash;

    /// The hash of `value` under `Steady(0)`.
    fn steady<T: Hash>(value: T) -> u64 {
        Steady(0).hash_one(value)
    }

    /// The hash is fixed: these words hold on every machine and build. A
    /// change here changes every map's order, and so may change play.
    #[test]
    fn the_steady_hash_is_these_words() {
        assert_eq!(steady(0u32), 0);
        assert_eq!(steady(1u32), 0x8017_18d4_3ef4_a768);
        assert_eq!(steady((3i32, -4i32)), 0x1687_5459_0ed6_0446);
        assert_eq!(steady("Gld1"), 0x06a7_7074_5203_c196);
    }

    /// A map built twice the same way iterates in one order, run after
    /// run: no random keys.
    #[test]
    fn a_map_iterates_in_one_order() {
        let order = || {
            (0u32..1000)
                .map(|i| (i.wrapping_mul(2_654_435_761), i))
                .collect::<Map<u32, u32>>()
                .into_values()
                .collect::<Vec<u32>>()
        };

        assert_eq!(order(), order());
    }

    /// A shuffling thread's maps take keys of their own.
    #[test]
    fn a_shuffling_thread_keys_each_map_afresh() {
        let keys = std::thread::spawn(|| {
            shuffle();
            (Steady::default(), Steady::default())
        })
        .join()
        .expect("thread");

        assert_ne!(keys.0, keys.1);
    }

    /// A digest tells `None` from `Some(0)` and one split of a list from
    /// another, and is the same for equal values.
    #[test]
    fn a_digest_tells_shapes_apart() {
        let none: Option<u32> = None;
        let lists = |a: &[u32], b: &[u32]| digest(&(a, b));
        let map: BTreeMap<u32, &str> = [(1, "a"), (2, "b")].into();

        assert_ne!(digest(&none), digest(&Some(0u32)));
        assert_ne!(lists(&[1], &[2, 3]), lists(&[1, 2], &[3]));
        assert_eq!(digest(&map), digest(&map.clone()));
    }
}
