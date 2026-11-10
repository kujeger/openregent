//! A game's state as a save holds it: openregent's own format, not the
//! original's (`.gmp`).
//!
//! The model is plain data, apart from the simulation's types, so those
//! may change freely; this module is the one place the format and its
//! [`VERSION`] live. A model of another version is refused, not migrated.
//!
//! ```text
//!  Game + Vm + World ──Game::snapshot──> Snapshot ──serde──> any format
//!  Snapshot ──World::restore, Game::resumed──> the same game, going on
//! ```
//!
//! Each simulation type converts by [`Saved`], next to its own fields.
//! So the model sits below `gpl` and `sim` and names none of their types:
//! their impls reach private fields without widening them. Rebuildable data stays out: descriptors, the program, caches,
//! drawing. A snapshot is of a game with no log (`Source::Free`).

mod game;
mod vm;
mod walk;
mod world;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;
use std::hash::{BuildHasher, Hash};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::base::fixed::Fixed;
use crate::data::fourcc::FourCc;

pub use game::*;
pub use vm::*;
pub use walk::*;
pub use world::*;

/// The format's version. A save of any other is refused: no migration
/// while the format is young.
pub const VERSION: u32 = 1;

/// A whole game at a frame's end: enough to go on exactly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub vm: Vm,
    pub world: World,
    pub game: Game,
}

/// Why a game will not save, or a model will not load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Saving: a frame stopped inside this agent's threads, mid-run.
    MidRun(u32),
    /// A model of another format version: the one it holds.
    Version(u32),
    /// A number this build cannot hold.
    Number(std::num::TryFromIntError),
    /// An array of another length.
    Length,
    /// A model of another program.
    Program,
    /// An activation of a definition of another length: its number.
    Definition(u32),
    /// Something the model names that this build lacks: "builtin X".
    Missing(String),
    /// A list node read outside a loading numbering.
    Numbering,
    /// An input line that does not read: why.
    Input(String),
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refused::MidRun(id) => write!(f, "mid-run, in agent {id}'s threads"),
            Refused::Version(found) => {
                write!(f, "save format version {found}, this build reads {VERSION}")
            }
            Refused::Number(e) => write!(f, "{e}"),
            Refused::Length => f.write_str("an array of another length"),
            Refused::Program => f.write_str("saved with another program"),
            Refused::Definition(def) => write!(f, "definition {def}: another length"),
            Refused::Missing(what) => write!(f, "no {what}"),
            Refused::Numbering => f.write_str("list node numbering is not loading"),
            Refused::Input(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for Refused {}

impl Snapshot {
    /// `self` if its version is [`VERSION`].
    ///
    /// # Errors
    ///
    /// Another version.
    pub fn checked(self) -> Result<Snapshot, Refused> {
        if self.version == VERSION {
            Ok(self)
        } else {
            Err(Refused::Version(self.version))
        }
    }
}

/// A simulation type's save model, and the way back.
pub trait Saved: Sized {
    type Model;

    fn save(&self) -> Self::Model;

    /// # Errors
    ///
    /// A model naming what this build lacks.
    fn load(model: Self::Model) -> Result<Self, Refused>;
}

/// Plain values: the model is the value.
macro_rules! same {
    ($($t:ty),*) => {$(
        impl Saved for $t {
            type Model = $t;

            fn save(&self) -> $t {
                self.clone()
            }

            fn load(model: $t) -> Result<$t, Refused> {
                Ok(model)
            }
        }
    )*};
}

same!(bool, u8, u16, u32, u64, i8, i16, i32, i64, f32, String);

impl Saved for usize {
    type Model = u64;

    fn save(&self) -> u64 {
        *self as u64
    }

    fn load(model: u64) -> Result<usize, Refused> {
        usize::try_from(model).map_err(Refused::Number)
    }
}

impl<T: Saved> Saved for Option<T> {
    type Model = Option<T::Model>;

    fn save(&self) -> Self::Model {
        self.as_ref().map(Saved::save)
    }

    fn load(model: Self::Model) -> Result<Self, Refused> {
        model.map(T::load).transpose()
    }
}

impl<T: Saved> Saved for Box<T> {
    type Model = T::Model;

    fn save(&self) -> T::Model {
        (**self).save()
    }

    fn load(model: T::Model) -> Result<Self, Refused> {
        T::load(model).map(Box::new)
    }
}

/// Shared: saved as what it points to, loaded unshared.
impl<T: Saved> Saved for std::sync::Arc<T> {
    type Model = T::Model;

    fn save(&self) -> T::Model {
        (**self).save()
    }

    fn load(model: T::Model) -> Result<Self, Refused> {
        T::load(model).map(std::sync::Arc::new)
    }
}

impl<T: Saved> Saved for Vec<T> {
    type Model = Vec<T::Model>;

    fn save(&self) -> Self::Model {
        self.iter().map(Saved::save).collect()
    }

    fn load(model: Self::Model) -> Result<Self, Refused> {
        model.into_iter().map(T::load).collect()
    }
}

impl<T: Saved> Saved for VecDeque<T> {
    type Model = Vec<T::Model>;

    fn save(&self) -> Self::Model {
        self.iter().map(Saved::save).collect()
    }

    fn load(model: Self::Model) -> Result<Self, Refused> {
        model.into_iter().map(T::load).collect()
    }
}

impl<T: Saved, const N: usize> Saved for [T; N] {
    type Model = [T::Model; N];

    fn save(&self) -> Self::Model {
        self.each_ref().map(Saved::save)
    }

    fn load(model: Self::Model) -> Result<Self, Refused> {
        let loaded: Vec<T> = model.into_iter().map(T::load).collect::<Result<_, _>>()?;

        loaded.try_into().map_err(|_| Refused::Length)
    }
}

impl<A: Saved, B: Saved> Saved for (A, B) {
    type Model = (A::Model, B::Model);

    fn save(&self) -> Self::Model {
        (self.0.save(), self.1.save())
    }

    fn load((a, b): Self::Model) -> Result<Self, Refused> {
        Ok((A::load(a)?, B::load(b)?))
    }
}

impl<A: Saved, B: Saved, C: Saved> Saved for (A, B, C) {
    type Model = (A::Model, B::Model, C::Model);

    fn save(&self) -> Self::Model {
        (self.0.save(), self.1.save(), self.2.save())
    }

    fn load((a, b, c): Self::Model) -> Result<Self, Refused> {
        Ok((A::load(a)?, B::load(b)?, C::load(c)?))
    }
}

/// A map's entries in key order: values saved in that order too, so a
/// snapshot of one state is one snapshot (`gpl::ops::node`).
fn entries<'m, K: Saved + 'm, V: Saved + 'm>(
    map: impl Iterator<Item = (&'m K, &'m V)>,
) -> BTreeMap<K::Model, V::Model>
where
    K::Model: Ord,
{
    let sorted: BTreeMap<K::Model, &V> = map.map(|(k, v)| (k.save(), v)).collect();

    sorted.into_iter().map(|(k, v)| (k, v.save())).collect()
}

impl<K: Saved + Ord, V: Saved> Saved for BTreeMap<K, V>
where
    K::Model: Ord,
{
    type Model = BTreeMap<K::Model, V::Model>;

    fn save(&self) -> Self::Model {
        entries(self.iter())
    }

    fn load(model: Self::Model) -> Result<Self, Refused> {
        model
            .into_iter()
            .map(|(k, v)| Ok((K::load(k)?, V::load(v)?)))
            .collect()
    }
}

impl<K: Saved + Eq + Hash, V: Saved, S: BuildHasher + Default> Saved for HashMap<K, V, S>
where
    K::Model: Ord,
{
    type Model = BTreeMap<K::Model, V::Model>;

    fn save(&self) -> Self::Model {
        entries(self.iter())
    }

    fn load(model: Self::Model) -> Result<Self, Refused> {
        model
            .into_iter()
            .map(|(k, v)| Ok((K::load(k)?, V::load(v)?)))
            .collect()
    }
}

impl<T: Saved + Ord> Saved for BTreeSet<T>
where
    T::Model: Ord,
{
    type Model = BTreeSet<T::Model>;

    fn save(&self) -> Self::Model {
        self.iter().map(Saved::save).collect()
    }

    fn load(model: Self::Model) -> Result<Self, Refused> {
        model.into_iter().map(T::load).collect()
    }
}

impl<T: Saved + Eq + Hash, S: BuildHasher + Default> Saved for HashSet<T, S>
where
    T::Model: Ord,
{
    type Model = BTreeSet<T::Model>;

    fn save(&self) -> Self::Model {
        self.iter().map(Saved::save).collect()
    }

    fn load(model: Self::Model) -> Result<Self, Refused> {
        model.into_iter().map(T::load).collect()
    }
}

/// A four-character code as text, a byte outside printable ASCII or a
/// backslash as `\xNN`: `"Gld1"`, `"CUT "`, `"\x00\x00\x00\x00"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Code(pub [u8; 4]);

/// The escape before a byte's two hex digits.
const ESCAPE: &str = "\\x";

impl Code {
    fn text(self) -> String {
        self.0
            .iter()
            .map(|&b| {
                if (b.is_ascii_graphic() && b != b'\\') || b == b' ' {
                    char::from(b).to_string()
                } else {
                    format!("{ESCAPE}{b:02x}")
                }
            })
            .collect()
    }

    fn parse(text: &str) -> Option<Code> {
        let mut bytes = Vec::new();
        let mut rest = text;

        while let Some(c) = rest.chars().next() {
            if let Some(hex) = rest.strip_prefix(ESCAPE) {
                bytes.push(u8::from_str_radix(hex.get(..2)?, 16).ok()?);
                rest = &hex[2..];
            } else {
                bytes.push(u8::try_from(c).ok()?);
                rest = &rest[c.len_utf8()..];
            }
        }

        bytes.try_into().ok().map(Code)
    }
}

impl Serialize for Code {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.text())
    }
}

impl<'de> Deserialize<'de> for Code {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Code, D::Error> {
        let text = String::deserialize(d)?;

        Code::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("not a code: {text}")))
    }
}

/// 16.16 as its raw word.
impl Saved for Fixed {
    type Model = i32;

    fn save(&self) -> i32 {
        self.to_bits()
    }

    fn load(model: i32) -> Result<Fixed, Refused> {
        Ok(Fixed::from_bits(model))
    }
}

impl Saved for FourCc {
    type Model = Code;

    fn save(&self) -> Code {
        Code(self.0)
    }

    fn load(model: Code) -> Result<FourCc, Refused> {
        Ok(FourCc(model.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_reads_back_from_its_text() {
        let codes = [*b"Gld1", *b"CUT ", [0, 1, 0xff, b'\\'], *b"x\\x4"];

        for code in codes {
            let text = Code(code).text();

            assert_eq!(Code::parse(&text), Some(Code(code)), "{text}");
        }

        assert_eq!(Code(*b"Gld1").text(), "Gld1");
        assert_eq!(Code([0, b'a', b'\\', 0x7f]).text(), "\\x00a\\x5c\\x7f");
    }
}
