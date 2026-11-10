//! Names: `AP:1`, `NameComponents`, which a building or hero takes as it
//! is made. **Binary** unless marked.
//!
//! ```text
//!  name table singleton      ── 17 generators, each four
//!     │                          STRT tables: NM01 HN01..HN04, .., NM16
//!     │                          HN61..HN64; NM17 BN01..BN04, buildings
//!  a generator
//!     list     every pick of one string a table, table 0 fastest; an
//!              empty table counts 1; a pick packs as
//!              c0 << 24 | c1 << 16 | c2 << 8 | c3
//!     shuffle  i = 0..n: swap i with rng_next(n), on its own stream
//!     next     list[drawn], drawn + 1; at the end a new seed,
//!              rng_next(0x0fffffff), and a new list
//!  seed        the session word at a new game;
//!              NMGM saves seed and drawn per generator, in key order
//! ```
//!
//! `BuildingRec` and `VehicleRec`
//! draw from the descriptor's `NameGenType` when `AP:1` is not set yet.

use std::collections::BTreeMap;

use super::{Answer, Found, World, agent_ref};
use crate::data::attrib::{EXPERIENCE_LEVEL, Key};
use crate::data::fourcc::FourCc;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};

/// `AP:1`, `NameComponents`.
pub const NAME: Key = Key::new(*b"AP\0", 1);

/// The generators and their four `STRT` tables, as name manager ctor makes
/// them: `NM01` to `NM16` take `HN01` to `HN64` four by four, `NM17`
/// `BN01` to `BN04`.
#[must_use]
pub fn generators() -> Vec<(FourCc, [FourCc; 4])> {
    let id = |prefix: &[u8; 2], n: usize| {
        let d = format!("{n:02}");
        let d = d.as_bytes();

        FourCc([prefix[0], prefix[1], d[0], d[1]])
    };
    let heroes = (0..16).map(|g| (id(b"NM", g + 1), [1, 2, 3, 4].map(|t| id(b"HN", 4 * g + t))));
    let buildings = (id(b"NM", 17), [1, 2, 3, 4].map(|t| id(b"BN", t)));

    heroes.chain([buildings]).collect()
}

/// The generator `AP:1` is spelled with: `table`, `APU:4`'s value, else the
/// descriptor's `NameGenType` (`BuildingRec` `BuildingRec`,
/// `VehicleRec` `VehicleRec`, the default of attr get).
#[must_use]
pub fn generator(desc: &crate::data::desc::Unit<'_>, table: Option<i32>) -> Option<FourCc> {
    let own = || {
        desc.building()
            .and_then(|b| b.name_gen_type())
            .or_else(|| desc.character().and_then(|c| c.name_gen_type()))
    };

    match table.filter(|t| *t != 0) {
        Some(t) => Some(FourCc::from_word(t.cast_unsigned())),
        None => own(),
    }
}

/// `AP:1` as text: each of the generator's tables with strings gives the
/// string its byte picks, table 0 the top byte, joined as they stand.
/// `HN02`'s strings carry their own leading space.
#[must_use]
pub fn spelled(text: &crate::data::text::Text<'_>, generator: FourCc, packed: u32) -> String {
    let tables = generators()
        .into_iter()
        .find(|(g, _)| *g == generator)
        .map(|(_, t)| t.to_vec())
        .unwrap_or_default();

    tables
        .iter()
        .zip(packed.to_be_bytes())
        .filter_map(|(id, pick)| text.at(&id.0, usize::from(pick)))
        .collect()
}

/// A unit's own name, so it stands before `AP:1`
/// spelled and a number.
///
/// ```
/// use regent_core::data::fourcc::FourCc;
/// use regent_core::sim::world::naming::Given;
///
/// let given = Given::Text { index: Some(4), text: "4".to_owned() };
/// let none = |_: FourCc| None;
///
/// assert_eq!(given.name(|n| (n == 4).then(|| "Going home".to_owned()), none), "Going home");
/// assert_eq!(given.name(|_| None, none), "4");
///
/// let clone = Given::Illusion(FourCc(*b"CVa1"));
/// let title = |_: FourCc| Some("Illusionary Warrior".to_owned());
///
/// assert_eq!(clone.name(|_| None, title), "Illusionary Warrior");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Given {
    /// What `SpecifyName` gave as a string (gpl specify name,
    /// gpl specify name), or a rename: `AITX` at the string's message number,
    /// else the XML
    /// strings by that number, else,
    /// that text empty, the string itself.
    Text {
        /// The message number, the string's id
        /// ([`crate::data::fourcc::string_id`]): a Text id names its
        /// string, The Wild's `"REDINN"` "Hooligan's Haughty Hangout".
        /// `None` for a rename.
        index: Option<i32>,
        text: String,
    },
    /// Illusionary Hero's clone: [`ILLUSION_TEXT`] over the `HEAD` title
    /// of the descriptor it took, this code.
    Illusion(FourCc),
}

/// `GMTX` 71, `Illusionary %s`: an illusion's name (`0x47`,
/// player spell apply).
pub const ILLUSION_TEXT: usize = 0x47;

impl Given {
    /// The name, `activity` giving `AITX` text by index, `illusion` a
    /// [`Given::Illusion`]'s text by code.
    #[must_use]
    pub fn name(
        &self,
        activity: impl Fn(i32) -> Option<String>,
        illusion: impl Fn(FourCc) -> Option<String>,
    ) -> String {
        match self {
            Given::Text { index, text } => index
                .and_then(activity)
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| text.clone()),
            Given::Illusion(code) => illusion(*code).unwrap_or_default(),
        }
    }
}

/// The level a hero needs before its panel's name box edits
/// (panel rename mode, the expression at `"#HeroNameEditingLevel"`).
pub const RENAME_LEVEL: &str = "#HeroNameEditingLevel";

/// `GMTX` 192, `%d` the level: what a press on the name box of a hero
/// below [`RENAME_LEVEL`] says.
pub const RENAME_REFUSED: usize = 192;

/// The longest name a rename carries, bytes. **Inferred**: the edit
/// box's own limit was not read.
pub const RENAME_MAX: usize = 31;

/// `SET_UNIT_NAME_MESSAGE` (`0x3f4`): unit `unit`, by agent, named
/// anew. Its name is kept in place, so the input stays `Copy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rename {
    pub unit: u32,
    name: [u8; RENAME_MAX],
    len: u8,
}

impl Rename {
    /// `name` cut to [`RENAME_MAX`] bytes on a character's edge.
    #[must_use]
    pub fn new(unit: u32, name: &str) -> Rename {
        let end = name
            .char_indices()
            .map(|(i, c)| i + c.len_utf8())
            .take_while(|end| *end <= RENAME_MAX)
            .last()
            .unwrap_or(0);
        let mut bytes = [0; RENAME_MAX];

        bytes[..end].copy_from_slice(&name.as_bytes()[..end]);

        Rename {
            unit,
            name: bytes,
            len: u8::try_from(end).unwrap_or_default(),
        }
    }

    /// The new name.
    #[must_use]
    pub fn name(&self) -> &str {
        std::str::from_utf8(&self.name[..usize::from(self.len)]).unwrap_or_default()
    }
}

/// Whether a unit's name box edits on a press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameBox {
    /// The box edits; Enter sends [`Rename`].
    Open,
    /// A hero below the level: [`RENAME_REFUSED`] of it, no edit.
    TooLow(i32),
}

/// A draw on a generator's own stream: [`crate::base::rng::draw`].
const fn draw(state: &mut u32, bound: u32) -> u32 {
    crate::base::rng::draw(state, bound.cast_signed()).cast_unsigned()
}

/// Rng next's bound for a generator's next seed.
const RESEED_BOUND: u32 = 0x0fff_ffff;

/// One `NameGeneratorIMP`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generator {
    /// Strings in each table, at least 1.
    counts: [u32; 4],
    /// The seed its list took,.
    pub seed: u32,
    /// Names given from this list,.
    pub drawn: u32,
    state: u32,
    list: Vec<u32>,
}

impl Generator {
    /// A generator over tables of `counts` strings, seeded with `seed`.
    #[must_use]
    pub fn new(counts: [u32; 4], seed: u32) -> Generator {
        let mut g = Generator {
            counts: counts.map(|c| c.max(1)),
            seed,
            drawn: 0,
            state: seed,
            list: vec![],
        };

        g.reseed(seed);
        g
    }

    /// Name reseed: the seed, a new list, shuffled.
    pub fn reseed(&mut self, seed: u32) {
        let [n0, n1, n2, n3] = self.counts;

        self.seed = seed;
        self.state = seed;
        self.drawn = 0;
        self.list = (0..n3)
            .flat_map(|c3| (0..n2).map(move |c2| (c2, c3)))
            .flat_map(|(c2, c3)| (0..n1).map(move |c1| (c1, c2, c3)))
            .flat_map(|(c1, c2, c3)| (0..n0).map(move |c0| c0 << 24 | c1 << 16 | c2 << 8 | c3))
            .collect();

        let n = u32::try_from(self.list.len()).unwrap_or(u32::MAX);

        for i in 0..self.list.len() {
            let j = draw(&mut self.state, n) as usize;

            self.list.swap(i, j);
        }
    }

    /// The next name.
    pub fn take(&mut self) -> u32 {
        let name = self.list.get(self.drawn as usize).copied().unwrap_or(0);

        self.drawn = self.drawn.wrapping_add(1);

        if self.drawn as usize >= self.list.len() {
            let seed = draw(&mut self.state, RESEED_BOUND);

            self.reseed(seed);
        }

        name
    }
}

/// The name generators, where given, and the start pieces still to name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Naming {
    /// The 17 name generators, where given.
    generators: Option<Generators>,
    /// Start pieces whose descriptor names them, in creation order: named
    /// as the generators are seeded ([`World::seed_names`]).
    unnamed: Vec<u32>,
}

impl Naming {
    /// The next name of generator `which`; `None` without generators.
    pub(super) fn next(&mut self, which: FourCc) -> Option<u32> {
        self.generators.as_mut()?.next(which)
    }

    /// Every generator reseeded with `seed`.
    fn reseed(&mut self, seed: u32) {
        if let Some(n) = self.generators.as_mut() {
            n.reseed(seed);
        }
    }
}

/// The 17 generators by key: `NMGM`'s order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Generators(BTreeMap<u32, Generator>);

impl Generators {
    /// Every generator of [`generators`], its tables' string counts from
    /// `count`, seeded with `seed`.
    #[must_use]
    pub fn new(count: impl Fn(FourCc) -> u32, seed: u32) -> Generators {
        Generators(
            generators()
                .into_iter()
                .map(|(k, tables)| (key_of(k), Generator::new(tables.map(&count), seed)))
                .collect(),
        )
    }

    /// Name seed all: every generator reseeded.
    pub fn reseed(&mut self, seed: u32) {
        for g in self.0.values_mut() {
            g.reseed(seed);
        }
    }

    /// Each generator's seed and names drawn, in `NMGM`'s order, as
    /// name load takes them: the seed reseeds and
    /// rebuilds the list, the count is kept, so the next name is the one
    /// the saved game would have given.
    pub fn restore(&mut self, saved: impl IntoIterator<Item = (u32, u32)>) {
        for (g, (seed, drawn)) in self.0.values_mut().zip(saved) {
            g.reseed(seed);
            g.drawn = drawn;
        }
    }

    /// Each generator's seed and names drawn, in `NMGM`'s order: the
    /// manager's map, by key as a `u32`.
    #[must_use]
    pub fn saved(&self) -> Vec<(u32, u32)> {
        self.0.values().map(|g| (g.seed, g.drawn)).collect()
    }

    /// The next name of generator `which`.
    pub fn next(&mut self, which: FourCc) -> Option<u32> {
        self.0.get_mut(&key_of(which)).map(Generator::take)
    }
}

/// The manager's map key: the id's bytes as a little-endian `u32`.
const fn key_of(id: FourCc) -> u32 {
    id.word()
}

impl World {
    /// `SpecifyName(unit, name)`, gpl specify name: no unit throws
    /// `SpecifyName() unit is null`; a string becomes the unit's own name
    /// ([`Given`]), replacing a number. An integer looks up the `STRT` at
    /// gpl ai by strt string, its name only if found.
    /// **Inferred**: nothing; only one function writes that table, with
    /// null, and no `STRT` has id -1. Returns nothing.
    pub(super) fn specify_name(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Gone => Err(Fault::throw("SpecifyName() unit is null"))?,
            Found::Unsure => Ok(Answer::Unknown),
            Found::Piece(_) => {
                if let Value::String(text) = vm.object(vm.arg(args, 2)?) {
                    let given = Given::Text {
                        index: super::message::number(&Value::String(text.clone())),
                        text: text.clone(),
                    };

                    let u = self.units.entry(id);

                    u.given = Some(given);
                    u.number = None;
                }

                Ok(Answer::Given)
            }
        }
    }

    /// The name `SpecifyName` gave agent `id`, if any.
    #[must_use]
    pub fn given_name(&self, id: u32) -> Option<&Given> {
        self.units.get(id).and_then(|u| u.given.as_ref())
    }

    /// `SET_UNIT_NAME_MESSAGE` decoded: the unit of the
    /// message's handle takes the name as its own (`GSUnit`, as
    /// `SpecifyName`'s string), its number gone; then its view is redrawn
    /// and its panel told (event `0xd435358`). `AP:1` stays. No
    /// dispatch.
    pub fn rename(&mut self, r: &Rename) {
        if self.pieces.contains(r.unit) {
            let given = Given::Text {
                index: None,
                text: r.name().to_owned(),
            };

            let u = self.units.entry(r.unit);

            u.given = Some(given);
            u.number = None;
        }
    }

    /// `owner`'s unit `id`'s name box on a press:
    /// a hero below [`RENAME_LEVEL`] by `APV:11` is
    /// refused; any other of the owner's units edits. `None` for
    /// another's.
    #[must_use]
    pub fn naming(
        &self,
        program: &crate::gpl::load::Program,
        owner: u32,
        id: u32,
    ) -> Option<NameBox> {
        let piece = self.pieces.get(id).filter(|p| p.owner == owner)?;
        let needed = match program.constant(RENAME_LEVEL) {
            Some(crate::gpl::exec::Literal::Int(v)) => v,
            _ => 0,
        };
        let level = piece.get(EXPERIENCE_LEVEL).unwrap_or_default();

        if self.is_hero(id) && level < needed {
            Some(NameBox::TooLow(needed))
        } else {
            Some(NameBox::Open)
        }
    }

    /// Name generators from now on.
    #[must_use]
    pub fn with_names(mut self, names: Generators) -> World {
        self.naming.generators = Some(names);
        self
    }

    /// The two special events' scripts, `GetSpecialEvent1Script`'s and
    /// `2`'s answers ([`super::event_script`]).
    #[must_use]
    pub fn with_events(mut self, events: [String; 2]) -> World {
        self.level.events = events;
        self
    }

    /// Name seed all with the session word: a new game's start.
    /// Game start seeds the world RNG with it too.
    pub fn seed_names(&mut self, session: u32) {
        self.rngs.seed(session);

        self.naming.reseed(session);
        self.name_started();
    }

    /// Keep start piece `id` to name as the generators are seeded, where
    /// its descriptor has a `NameGenType`.
    pub(super) fn name_later(&mut self, id: u32, body: &super::bodies::Body) {
        if body.names.is_some() {
            self.naming.unnamed.push(id);
        }
    }

    /// The start's names. Freestyle start net seeds the generators with the
    /// session word before it makes the map;
    /// each start unit's
    /// agent animate reaches method through `GSUnit`, drawing
    /// its name in creation order. **Corpus**: no freestyle start save
    /// holds a named piece; only the Broken Alliance's inn would be.
    fn name_started(&mut self) {
        let named = NAME.packed();

        for id in std::mem::take(&mut self.naming.unnamed) {
            let which = self
                .pieces
                .unbound()
                .get(&id)
                .filter(|p| !p.attributes.contains_key(&named))
                .and_then(|p| self.descriptors.body(p.code))
                .and_then(|b| b.names);
            let name = which.and_then(|w| self.naming.next(w));

            if let (Some(name), Some(p)) = (name, self.pieces.unbound_mut(id)) {
                p.attributes.insert(named, name.cast_signed());
            }
        }
    }

    /// The name generators, where given.
    #[must_use]
    pub const fn names(&self) -> Option<&Generators> {
        self.naming.generators.as_ref()
    }

    /// Name piece `id` as its descriptor's method does: `AP:1` from its
    /// `NameGenType`'s generator, unless set; with no generator, a
    /// building's number (`super::numbered`).
    pub fn name(&mut self, id: u32) {
        let named = NAME.packed();
        let unnamed = self
            .pieces
            .get(id)
            .filter(|p| {
                !p.attributes.contains_key(&named)
                    && self.units.get(id).is_none_or(|u| u.number.is_none())
            })
            .and_then(|p| self.descriptors.body(p.code));
        let which = unnamed.and_then(|b| b.names);
        let numbered = unnamed.is_some() && which.is_none();
        let name = which.and_then(|w| self.naming.next(w));

        if let (Some(name), Some(p)) = (name, self.pieces.get_mut(id)) {
            p.attributes.insert(named, name.cast_signed());
        }

        if numbered {
            self.number(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The inn trace's session word names its inn `BN01` string 42: a list
    /// of 43, shuffled on that seed, puts 42 first.
    #[test]
    fn a_shuffled_list_of_forty_three_puts_forty_two_first_on_the_inn_seed() {
        let mut g = Generator::new([43, 0, 0, 0], 0x0b60_9e2c);

        assert_eq!(g.take(), 42 << 24);
        assert_eq!((g.seed, g.drawn), (0x0b60_9e2c, 1));
    }

    #[test]
    fn nmgm_order_is_the_keys_as_words() {
        let order: Vec<FourCc> = {
            let mut ids: Vec<FourCc> = generators().into_iter().map(|(k, _)| k).collect();

            ids.sort_by_key(|k| key_of(*k));
            ids
        };

        assert_eq!(order[0], FourCc(*b"NM10"));
        assert_eq!(order[14], FourCc(*b"NM17"));
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Generator {
        type Model = m::Generator;

        fn save(&self) -> m::Generator {
            let Generator {
                counts,
                seed,
                drawn,
                state,
                list,
            } = self;

            m::Generator {
                counts: *counts,
                seed: *seed,
                drawn: *drawn,
                state: *state,
                list: list.clone(),
            }
        }

        fn load(model: m::Generator) -> Result<Generator, Refused> {
            Ok(Generator {
                counts: model.counts,
                seed: model.seed,
                drawn: model.drawn,
                state: model.state,
                list: model.list,
            })
        }
    }

    impl Saved for Naming {
        type Model = m::Naming;

        fn save(&self) -> m::Naming {
            m::Naming {
                generators: self.generators.save(),
                unnamed: self.unnamed.clone(),
            }
        }

        fn load(model: m::Naming) -> Result<Naming, Refused> {
            Ok(Naming {
                generators: Option::load(model.generators)?,
                unnamed: model.unnamed,
            })
        }
    }

    impl Saved for Generators {
        type Model = BTreeMap<u32, m::Generator>;

        fn save(&self) -> Self::Model {
            self.0.save()
        }

        fn load(model: Self::Model) -> Result<Generators, Refused> {
            BTreeMap::load(model).map(Generators)
        }
    }

    impl Saved for Given {
        type Model = m::Given;

        fn save(&self) -> m::Given {
            match self {
                Given::Text { index, text } => m::Given::Text {
                    index: *index,
                    text: text.clone(),
                },
                Given::Illusion(code) => m::Given::Illusion(code.save()),
            }
        }

        fn load(model: m::Given) -> Result<Given, Refused> {
            match model {
                m::Given::Text { index, text } => Ok(Given::Text { index, text }),
                m::Given::Illusion(code) => Ok(Given::Illusion(FourCc::load(code)?)),
            }
        }
    }
}
