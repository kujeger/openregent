//! XML descriptions, as mods and quests write them, made into the binary
//! records the readers parse.
//!
//! HD loads each descriptor set from the archives, then from every
//! `<Descriptions>` file: each
//! `<Description>` child of `<Majesty>` whose `type` is the set's.
//! **Binary**, `MajestyHD.exe`:
//!
//! ```text
//!  dataset apply
//!   pass 1 (pass flag set)        pass 2
//!   FACT TERN TPRT                      SNDP SNDG SND
//!   SNDP SNDG SND                       WEAP MOVE EXPL SNDP SNDG SND UNIT
//!   WEAP MOVE EXPL SNDP SNDG SND UNIT   ACTN
//!   ACTN
//!
//!  each set: its archive records, then its XML (desc set load xml)
//!   pass 1: id and names alone, every record registered
//!   pass 2: every field read, each record registered again
//!  register (desc register, flag 1): an id already there is replaced
//! ```
//!
//! So an XML record replaces an archive one of its id, whole: fields it
//! leaves out are the constructor's, zero, not the archive's. Pass 1 is
//! why a reference by name finds a record defined later, or in another
//! set: `UpgradeTo value="Guardhouse2"` resolves to `ABE2`.
//!
//! The made records are the archive's own layout, so every reader works
//! on them unchanged; [`super::Origin::Xml`] marks them, for the one rule
//! the archive loader adds (a zero `CanUse` filled from the id).
//!
//! # References
//!
//! A reference by name (desc xml reference and kin) takes the first record of
//! its set, in ascending id order (the set's `std::map`), whose name
//! matches without case; else `default` gives 0; else the text's id
//! ([`string_id`]). A record's own `ID` is read by [`string_id`]: four
//! bytes a code, a number its value, anything else a CRC-32. 983 of the
//! Workshop's descriptions carry such an id (`MBPortal`).
//!
//! # What HD drops
//!
//! - A `subType` its set's table lacks: the whole description.
//!   The table is by name, so the SDK example's
//!   `subType="4"` and `"1"` load nothing, nor does `General`: the unit
//!   set spells it `Generic`.
//! - `Script type` and `cProc`: action script read writes 0 to
//!   both whatever the file says.
//! - `AllowedSpells`' `ID` attribute: a spell's index is the list's
//!   count so far.
//! - `GPLType` and a character's `Attributes`: read into fields the
//!   archive format does not hold, so not made here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{BuildingFlag, CharacterFlag, EmitterFlag, EmitterShape, Info, PhaseFlag, User};
use crate::data::catalogue::Catalogue;
use crate::data::fourcc::{FourCc, string_id};
use crate::data::xml::Element;

// --------------------------------------------------------------------------
// Sets
// --------------------------------------------------------------------------

/// The descriptor sets a `<Description>` may add to, by its `type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Set {
    Unit,
    Action,
    Sound,
}

impl Set {
    /// Pass 2's order: sounds, then
    /// units, then actions.
    pub const ORDER: [Set; 3] = [Set::Sound, Set::Unit, Set::Action];

    /// The `type` attribute: the set's registered name (`"Unit"` in
    /// unit engine names, `"Action"` in action list ctor, `"Sound"`).
    #[must_use]
    pub fn of(name: &str) -> Option<Set> {
        match name {
            "Unit" => Some(Set::Unit),
            "Action" => Some(Set::Action),
            "Sound" => Some(Set::Sound),
            _ => None,
        }
    }

    /// The archive chunk its records sit in.
    #[must_use]
    pub const fn tag(self) -> [u8; 4] {
        match self {
            Set::Unit => *b"DUNT",
            Set::Action => *b"DACT",
            Set::Sound => *b"DSND",
        }
    }

    /// The `subType` table (xml name attr on the set's). Names
    /// match without case.
    const fn sub_types(self) -> &'static [(&'static str, u32)] {
        match self {
            Set::Unit => &UNIT_SUBTYPES,
            Set::Action | Set::Sound => &[("Standard", STANDARD)],
        }
    }
}

/// The unit set's `subType`s.
/// `Command`, 6, is no kind the readers know, so it is refused.
const UNIT_SUBTYPES: [(&str, u32); 8] = [
    ("Generic", 0),
    ("Projectile", 1),
    ("Building", 2),
    ("Vehicle", 3),
    ("Character", 3),
    ("Overlay", 4),
    ("ParticleSystem", 5),
    ("Command", COMMAND),
];

/// The action and sound sets' one `subType`, `Standard`.
const STANDARD: u32 = 1;

/// The unit kind `Command`, which no shipped record is.
const COMMAND: u32 = 6;

/// Unit kinds, `HEAD + 4`.
const PROJECTILE: u32 = 1;
const BUILDING: u32 = 2;
const CHARACTER: u32 = 3;
const OVERLAY: u32 = 4;
const PARTICLE: u32 = 5;

// --------------------------------------------------------------------------
// Name tables
// --------------------------------------------------------------------------

/// `Data/ImageSetIDXRef.xml`: an action's `ImageSet` by name, as
/// the dataset's startup loads it into the desc service.
#[derive(Debug, Clone, Default)]
pub struct ImageSets(Vec<(String, u32)>);

impl ImageSets {
    /// The table in the file at `path`; none where it will not read.
    #[must_use]
    pub fn read(path: &Path) -> ImageSets {
        let text = std::fs::read(path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();

        crate::data::xml::parse(&text)
            .map_or_else(|_| ImageSets::default(), |r| ImageSets::parse(&r))
    }

    /// The `NameToIDTable tableName="ImageSet"`'s entries.
    #[must_use]
    pub fn parse(root: &Element<'_>) -> ImageSets {
        let entries = root
            .named("NameToIDTable")
            .filter(|t| t.attribute("tableName") == Some("ImageSet"))
            .flat_map(|t| t.named("Entry"))
            .filter_map(|e| Some((e.attribute("name")?.to_owned(), atoi(e.attribute("ID")?))))
            .collect();

        ImageSets(entries)
    }

    /// The first entry named `name`, any case. **Inferred**: a repeated
    /// name keeps its first id, as a `std::map` insert does.
    fn find(&self, name: &str) -> Option<u32> {
        self.0
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, id)| *id)
    }
}

/// A flag table: names, any case, to bits.
type Table<'a> = &'a [(&'a str, u32)];

/// `CanUse`.
const CAN_USE: [(&str, u32); 3] = [
    (User::HumanPlayer.name(), 1 << User::HumanPlayer as u32),
    (User::Monster.name(), 1 << User::Monster as u32),
    (User::AiPlayer.name(), 1 << User::AiPlayer as u32),
];

/// `Category`: `None` alone.
const CATEGORY: [(&str, u32); 1] = [("None", 0)];

/// `ShadowType`.
const SHADOW_TYPE: [(&str, u32); 3] = [("None", 0), ("Static", 1), ("Dynamic", 2)];

/// `ShadowOptions`.
const SHADOW_OPTIONS: [(&str, u32); 2] = [("Least", 0), ("Max", 0xff)];

/// An overlay's `Flags`.
const OVERLAY_FLAGS: [(&str, u32); 1] = [("TransparentToMouse", 1)];

/// An action's `Info`.
const ACTION_INFO: [(&str, u32); 1] = [("NoTarget", 1)];

/// An action's `Flags`.
const ACTION_FLAGS: [(&str, u32); 1] = [("IsSpell", 1)];

/// An action's `SpellType`: values, though read as flags.
/// The Workshop's `Support`, `Passive` and the rest are not in it.
const SPELL_TYPE: [(&str, u32); 4] = [
    ("None", 0),
    ("Attack", 1),
    ("CombatUtility", 2),
    ("Other", 3),
];

/// A phase's `Flags`.
const PHASE_FLAGS: [(&str, u32); 4] = [
    ("Streamed", 1),
    ("ExcludeFromVariations", 0x20),
    (PhaseFlag::Looped.name(), 1 << PhaseFlag::Looped as u32),
    (PhaseFlag::CanQueue.name(), 1 << PhaseFlag::CanQueue as u32),
];

/// A movement attachment's `type`.
const MOVEMENT: [(&str, u32); 5] = [
    ("None", 0),
    ("Walk", 1),
    ("Crawl", 2),
    ("Run", 3),
    ("Fly", 4),
];

/// `AllowedWeapon`.
const WEAPONS: [(&str, u32); 10] = [
    ("None", 0),
    ("Chaos", 1),
    ("Crossbow", 2),
    ("Dagger", 3),
    ("Hammer", 4),
    ("Longbow", 5),
    ("Longsword", 6),
    ("Mace", 7),
    ("Staff", 8),
    ("Axeclub", 9),
];

/// `AllowedArmor`.
const ARMOUR: [(&str, u32); 5] = [
    ("None", 0),
    ("Leather", 1),
    ("Chain", 2),
    ("Plate", 3),
    ("Chaos", 4),
];

fn info_table() -> Vec<(&'static str, u32)> {
    Info::ALL
        .iter()
        .map(|i| (i.name(), 1 << *i as u32))
        .collect()
}

fn building_flags() -> Vec<(&'static str, u32)> {
    BuildingFlag::ALL
        .iter()
        .map(|f| (f.name(), 1 << *f as u32))
        .collect()
}

fn character_flags() -> Vec<(&'static str, u32)> {
    CharacterFlag::ALL
        .iter()
        .map(|f| (f.name(), 1 << *f as u32))
        .collect()
}

fn emitter_types() -> Vec<(&'static str, u32)> {
    EmitterShape::ALL
        .iter()
        .map(|s| (s.name(), 1 << *s as u32))
        .collect()
}

fn emitter_flags() -> Vec<(&'static str, u32)> {
    EmitterFlag::ALL
        .iter()
        .map(|f| (f.name(), 1 << *f as u32))
        .collect()
}

// --------------------------------------------------------------------------
// Registries
// --------------------------------------------------------------------------

/// The descriptor sets a reference resolves through: each by id, with its
/// `HEAD` name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Registry {
    Unit,
    Action,
    Sound,
    /// `DSDP`, a sound phase.
    Phase,
    /// `DSDG`, a sound group.
    Group,
    /// `DMOV`, a movement class.
    Movement,
}

impl Registry {
    const ALL: [Registry; 6] = [
        Registry::Unit,
        Registry::Action,
        Registry::Sound,
        Registry::Phase,
        Registry::Group,
        Registry::Movement,
    ];

    const fn tag(self) -> [u8; 4] {
        match self {
            Registry::Unit => *b"DUNT",
            Registry::Action => *b"DACT",
            Registry::Sound => *b"DSND",
            Registry::Phase => *b"DSDP",
            Registry::Group => *b"DSDG",
            Registry::Movement => *b"DMOV",
        }
    }

    const fn of(set: Set) -> Registry {
        match set {
            Set::Unit => Registry::Unit,
            Set::Action => Registry::Action,
            Set::Sound => Registry::Sound,
        }
    }
}

/// Every registry's names by id, ascending as the engine's `std::map`
/// (keys compared unsigned).
#[derive(Debug, Clone, Default)]
struct Names(BTreeMap<Registry, BTreeMap<u32, String>>);

impl Names {
    /// Each registry's archive records, the last loaded of an id winning.
    fn of(catalogue: &Catalogue<'_>) -> Names {
        let mut out = Names::default();

        for registry in Registry::ALL {
            let names = out.0.entry(registry).or_default();

            for (_, record) in catalogue.tagged(&registry.tag()) {
                let name = super::Part::parse(record.data)
                    .ok()
                    .and_then(|p| Some(super::Head::parse(p.find(b"HEAD")?.body).ok()?.name))
                    .map(|n| String::from_utf8_lossy(n).into_owned())
                    .unwrap_or_default();

                names.insert(record.id.word(), name);
            }
        }

        out
    }

    fn set(&mut self, registry: Registry, id: u32, name: &str) {
        self.0
            .entry(registry)
            .or_default()
            .insert(id, name.to_owned());
    }

    /// The first id named `name`, any case (desc find name,
    /// desc find name): an empty name matches nothing.
    fn find(&self, registry: Registry, name: &str) -> Option<u32> {
        self.0.get(&registry).and_then(|m| {
            m.iter()
                .find(|(_, n)| !n.is_empty() && n.eq_ignore_ascii_case(name))
                .map(|(id, _)| *id)
        })
    }

    /// A reference by name: the record's id, else `default` 0, else the
    /// text's own id.
    fn resolve(&self, registry: Registry, text: &str) -> u32 {
        self.find(registry, text).unwrap_or_else(|| {
            if text.eq_ignore_ascii_case(DEFAULT) {
                0
            } else {
                string_id(text).cast_unsigned()
            }
        })
    }
}

/// The text a reference takes for "none" (`"default"`).
const DEFAULT: &str = "default";

// --------------------------------------------------------------------------
// Compiling
// --------------------------------------------------------------------------

/// One record made from XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub set: Set,
    pub id: FourCc,
    /// The `Name` attribute, the directory's name too.
    pub name: String,
    /// The payload, in the archive's layout.
    pub payload: Vec<u8>,
}

/// A description HD would not load, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// Which source, by its index.
    pub source: usize,
    pub id: Option<String>,
    pub why: Why,
}

/// Why a description loads nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// A `type` no set reads (HD has others, `Movement` among them; no
    /// mod writes one).
    Type,
    /// A `subType` not in its set's table.
    SubType,
    /// No `ID` attribute. **Inferred**: HD would use an unset word.
    NoId,
    /// A kind the readers do not know: `Command`.
    Kind,
}

/// What XML sources make: each set's records in their final form, in
/// first-made order, and what loads nothing.
#[derive(Debug, Clone, Default)]
pub struct Compiled {
    pub records: Vec<Record>,
    pub refused: Vec<Refused>,
}

impl Compiled {
    /// The records as an archive, one chunk per set, for a catalogue to
    /// take after every real one.
    #[must_use]
    pub fn archive(&self) -> Vec<u8> {
        let chunks: Vec<crate::data::cam::Written<'_>> = Set::ORDER
            .iter()
            .map(|set| crate::data::cam::Written {
                tag: FourCc(set.tag()),
                flag: crate::data::cam::ChunkFlag::Clear,
                records: self
                    .records
                    .iter()
                    .filter(|r| r.set == *set)
                    .map(|r| (r.id, r.name.as_str(), r.payload.as_slice()))
                    .collect(),
            })
            .filter(|c| !c.records.is_empty())
            .collect();

        crate::data::cam::write(&chunks)
    }
}

/// XML files read, in load order: the text each tree borrows.
#[derive(Debug, Clone, Default)]
pub struct Sources(Vec<(PathBuf, String)>);

impl Sources {
    /// Each of `paths` read; one that will not read is left out, as HD's
    /// read fails and loads nothing. Not UTF-8, read lossily.
    #[must_use]
    pub fn read(paths: &[PathBuf]) -> Sources {
        Sources(
            paths
                .iter()
                .filter_map(|p| {
                    let bytes = std::fs::read(p).ok()?;

                    Some((p.clone(), String::from_utf8_lossy(&bytes).into_owned()))
                })
                .collect(),
        )
    }

    /// Each file's tree, or the files that will not parse, which load
    /// nothing.
    #[must_use]
    pub fn parsed(&self) -> (Vec<Element<'_>>, Vec<&Path>) {
        let mut trees = Vec::new();
        let mut unread = Vec::new();

        for (path, text) in &self.0 {
            match crate::data::xml::parse(text) {
                Ok(tree) => trees.push(tree),
                Err(_) => unread.push(path.as_path()),
            }
        }

        (trees, unread)
    }

    /// Every description, made over `catalogue`'s ([`compile`]).
    #[must_use]
    pub fn compile(&self, catalogue: &Catalogue<'_>, sets: &ImageSets) -> Compiled {
        let (trees, _) = self.parsed();
        let roots: Vec<&Element<'_>> = trees.iter().collect();

        compile(&roots, catalogue, sets)
    }

    /// Every `<Strings>` entry, later files over earlier.
    #[must_use]
    pub fn strings(&self) -> crate::data::text::Strings {
        let (trees, _) = self.parsed();
        let mut out = crate::data::text::Strings::default();

        for tree in &trees {
            out.add(tree);
        }

        out
    }
}

/// A description's header, as pass 1 reads it.
struct Header<'x> {
    set: Set,
    sub: u32,
    id: u32,
    name: String,
    description: String,
    element: &'x Element<'x>,
}

/// Every `<Description>` of `sources`, in order, made into records over
/// `catalogue`'s, as HD's two passes make them.
#[must_use]
pub fn compile(sources: &[&Element<'_>], catalogue: &Catalogue<'_>, sets: &ImageSets) -> Compiled {
    let mut out = Compiled::default();
    let mut headers = Vec::new();

    for (source, root) in sources.iter().enumerate() {
        // desc set load xml: the document's `Majesty`, its
        // `Description` children.
        let descriptions = std::iter::once(*root)
            .filter(|r| r.name == "Majesty")
            .flat_map(|r| r.named("Description"));

        for element in descriptions {
            match header(element) {
                Ok(h) => headers.push(h),
                Err(why) => out.refused.push(Refused {
                    source,
                    id: element.attribute("ID").map(str::to_owned),
                    why,
                }),
            }
        }
    }

    let archive = Names::of(catalogue);
    let mut names = archive.clone();

    // Pass 1: every record's id and names.
    for h in &headers {
        names.set(Registry::of(h.set), h.id, &h.name);
    }

    // Pass 2, set by set: the archive's records again, then each XML
    // record read whole and registered over them.
    let mut made: BTreeMap<(Set, u32), Record> = BTreeMap::new();
    let mut order: Vec<(Set, u32)> = Vec::new();

    for set in Set::ORDER {
        let registry = Registry::of(set);

        if let Some(own) = archive.0.get(&registry) {
            for (id, name) in own {
                names.set(registry, *id, name);
            }
        }

        for h in headers.iter().filter(|h| h.set == set) {
            let record = Record {
                set,
                id: FourCc::from_word(h.id),
                name: h.name.clone(),
                payload: encode(h, &names, sets),
            };

            names.set(registry, h.id, &h.name);

            if made.insert((set, h.id), record).is_none() {
                order.push((set, h.id));
            }
        }
    }

    out.records = order
        .into_iter()
        .filter_map(|key| made.remove(&key))
        .collect();
    out
}

/// A description's set, subtype, id and names, or why HD skips it.
fn header<'x>(element: &'x Element<'x>) -> Result<Header<'x>, Why> {
    let set = element
        .attribute("type")
        .and_then(Set::of)
        .ok_or(Why::Type)?;
    let sub = element
        .attribute("subType")
        .and_then(|s| {
            set.sub_types()
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(s))
        })
        .map(|(_, v)| *v)
        .ok_or(Why::SubType)?;
    let id = element.attribute("ID").ok_or(Why::NoId)?;

    if set == Set::Unit && sub == COMMAND {
        Err(Why::Kind)
    } else {
        Ok(Header {
            set,
            sub,
            id: string_id(id).cast_unsigned(),
            name: element.attribute("Name").unwrap_or_default().to_owned(),
            description: element
                .attribute("Description")
                .unwrap_or_default()
                .to_owned(),
            element,
        })
    }
}

// --------------------------------------------------------------------------
// Reading a section's fields, as HD's helpers do
// --------------------------------------------------------------------------

/// C `atoi`: white space, a sign, digits; 0 without any; wrapping.
fn atoi(text: &str) -> u32 {
    let t = text.trim_start();
    let (negative, digits) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let n = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0u32, |n, d| {
            n.wrapping_mul(10).wrapping_add(u32::from(d - b'0'))
        });

    if negative { n.wrapping_neg() } else { n }
}

/// C `atof` as an `f32`: the longest number prefix, else 0.
fn atof(text: &str) -> f32 {
    let t = text.trim_start();
    let end = t
        .char_indices()
        .take_while(|(i, c)| {
            c.is_ascii_digit()
                || matches!(c, '.' | 'e' | 'E')
                || (matches!(c, '+' | '-') && { *i == 0 || t[..*i].ends_with(['e', 'E']) })
        })
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);

    (0..=end)
        .rev()
        .find_map(|n| t[..n].parse::<f32>().ok())
        .unwrap_or(0.0)
}

/// A section's view: the first child of each name, as `method(name, 0)`.
#[derive(Clone, Copy)]
struct Section<'s, 'x>(Option<&'s Element<'x>>);

impl<'s, 'x> Section<'s, 'x> {
    fn of(element: &'s Element<'x>, name: &str) -> Section<'s, 'x> {
        Section(element.child(name))
    }

    /// The first child named `name`.
    fn get(self, name: &str) -> Option<&'s Element<'x>> {
        self.0?.child(name)
    }

    /// Every child named `name`.
    fn all(self, name: &'s str) -> impl Iterator<Item = &'s Element<'x>> + 's {
        self.0.into_iter().flat_map(move |s| s.named(name))
    }

    /// `value` of the first `name`: empty if absent.
    fn value(self, name: &str) -> Option<&'s str> {
        self.get(name)
            .map(|e| e.attribute("value").unwrap_or_default())
    }

    /// An integer field (xml int field, `atoi`); 0 when absent, the
    /// constructor's.
    fn int(self, name: &str) -> u32 {
        self.value(name).map_or(0, atoi)
    }

    /// A float field (xml float field, `atof`).
    fn float(self, name: &str) -> f32 {
        self.value(name).map_or(0.0, atof)
    }

    /// An id field (xml id field, [`string_id`]).
    fn id(self, name: &str) -> u32 {
        self.value(name).map_or(0, |v| string_id(v).cast_unsigned())
    }

    /// A flag field: every `name`, a number its value,
    /// a name its table's bits, others nothing; all `OR`ed.
    fn flags(self, name: &'s str, table: Table<'_>) -> u32 {
        self.matched(name, table).unwrap_or(0)
    }

    /// [`Section::flags`], `None` when no element was a number or a known
    /// name: what the helper returns, which `AllowedWeapon` tests.
    fn matched(self, name: &'s str, table: Table<'_>) -> Option<u32> {
        self.all(name)
            .filter_map(|e| {
                let v = e.attribute("value").unwrap_or_default();

                if v.starts_with(|c: char| c.is_ascii_digit()) {
                    Some(atoi(v))
                } else {
                    table
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case(v))
                        .map(|(_, bits)| *bits)
                }
            })
            .reduce(|a, b| a | b)
    }

    /// A reference by name into `registry`.
    fn reference(self, name: &str, registry: Registry, names: &Names) -> u32 {
        self.value(name).map_or(0, |v| names.resolve(registry, v))
    }

    /// Two integer attributes of `name`.
    fn pair(self, name: &str, keys: [&str; 2]) -> [u32; 2] {
        let element = self.get(name);

        keys.map(|k| element.and_then(|e| e.attribute(k)).map_or(0, atoi))
    }

    /// Two float attributes of `name`.
    fn floats(self, name: &str, keys: [&str; 2]) -> [f32; 2] {
        let element = self.get(name);

        keys.map(|k| element.and_then(|e| e.attribute(k)).map_or(0.0, atof))
    }

    /// `N` comma-separated floats of `value` (three or four). **Inferred**: a missing part is 0.
    fn vector<const N: usize>(self, name: &str) -> [f32; N] {
        vector(self.value(name).unwrap_or_default())
    }
}

/// `N` comma-separated floats.
fn vector<const N: usize>(text: &str) -> [f32; N] {
    let mut parts = text.split(',');

    [(); N].map(|()| parts.next().map_or(0.0, atof))
}

// --------------------------------------------------------------------------
// Writing the archive's layout
// --------------------------------------------------------------------------

/// A section version (`DUNT`'s `PRIM` 4, a particle system's 5, ...).
type Version = u32;

/// A unit's `PRIM` version: 4, all its fields (unit prim read common
/// reads 1 to 4).
const UNIT_PRIM: Version = 4;

/// A particle system's `PRIM` version.
const PARTICLE_PRIM: Version = 5;

/// A building's, a character's and an overlay's `SECD` versions, as every
/// shipped record has them.
const BUILDING_SECD: Version = 5;
const CHARACTER_SECD: Version = 8;
const OVERLAY_SECD: Version = 3;

/// An action's `PRIM` and `SECD` versions.
const ACTION_PRIM: Version = 1;
const ACTION_SECD: Version = 3;

/// A sound's `PRIM` version: 0x3c-byte phases.
const SOUND_PRIM: Version = 3;

/// The word a list opens with, before its count.
const LIST_VERSION: u32 = 1;

/// An action `PRIM`'s declared length: its fixed words; the script block
/// trails it, as in every shipped record.
const ACTION_DECLARED: usize = 0x40;

/// A sound `PRIM`'s declared length: the phase count and phases trail.
const SOUND_DECLARED: usize = 8;

/// Slots in each attachment table: `EXPL`, `MOVE`, `WEAP`.
const EXPLOSION_SLOTS: usize = 4;
const MOVEMENT_SLOTS: usize = 8;
const WEAPON_SLOTS: usize = 2;

/// Little-endian bytes.
#[derive(Default)]
struct Bytes(Vec<u8>);

impl Bytes {
    fn word(&mut self, w: u32) -> &mut Self {
        self.0.extend_from_slice(&w.to_le_bytes());
        self
    }

    fn float(&mut self, f: f32) -> &mut Self {
        self.word(f.to_bits())
    }

    fn floats(&mut self, fs: &[f32]) -> &mut Self {
        for f in fs {
            self.float(*f);
        }
        self
    }

    fn cstr(&mut self, s: &str) -> &mut Self {
        self.0.extend_from_slice(s.as_bytes());
        self.0.push(0);
        self
    }

    fn raw(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend_from_slice(b);
        self
    }

    /// A list: version, count, then `entries` as word pairs.
    fn list(&mut self, entries: &[(u32, u32)]) -> &mut Self {
        self.word(LIST_VERSION).word(len(entries.len()));

        for (k, v) in entries {
            self.word(*k).word(*v);
        }
        self
    }
}

/// A count as a word.
fn len(n: usize) -> u32 {
    u32::try_from(n).expect("a count under 4 G")
}

/// A part: tag, declared length, version, child count, body.
fn part(tag: [u8; 4], version: Version, children: usize, body: &[u8], declared: usize) -> Vec<u8> {
    let mut out = Bytes::default();

    out.raw(&tag)
        .word(len(declared))
        .word(version)
        .word(len(children))
        .raw(body);
    out.0
}

/// A leaf whose length is its body.
fn leaf(tag: [u8; 4], version: Version, body: &[u8]) -> Vec<u8> {
    part(tag, version, 0, body, body.len())
}

/// The record: root, `DATA`, its parts.
fn record(set: Set, parts: &[Vec<u8>]) -> Vec<u8> {
    let inner = parts.concat();
    let data = part(*b"DATA", 0, parts.len(), &inner, inner.len());

    part(set.tag(), super::XML_ORIGIN, 1, &data, data.len())
}

/// `HEAD`: id, subtype, both names.
fn head(h: &Header<'_>) -> Vec<u8> {
    let mut b = Bytes::default();

    b.word(h.id).word(h.sub).cstr(&h.name).cstr(&h.description);
    leaf(*b"HEAD", 0, &b.0)
}

/// The record a header's description makes.
fn encode(h: &Header<'_>, names: &Names, sets: &ImageSets) -> Vec<u8> {
    match h.set {
        Set::Unit => unit(h, names),
        Set::Action => action(h, names, sets),
        Set::Sound => sound(h, names),
    }
}

// --------------------------------------------------------------------------
// Units
// --------------------------------------------------------------------------

/// A `DUNT` record. **Binary**: the engine part by kind
/// (unit xml read: unit engine xml read,
/// projectile projectile xml read, overlay overlay xml read, particle system
/// particle xml read), the game part for buildings, characters and overlays.
/// The layout is unit prim read common's:
///
/// ```text
/// PRIM  Info Category CanUse Menu ImageIDBase
///       kind's block: AssociatedUnit (ExplosionID on a projectile)
///                     overlay: ExplosionID AttachmentPointID
///                     particle system: its 264 bytes
///       Script type, cProc, GPLFunction
///       ATCH EXPL MOVE [WEAP]          kinds 0 to 3; no WEAP on a projectile
///       SoundOverride list, DefaultSound, ShadowType, ShadowOptions
/// ```
fn unit(h: &Header<'_>, names: &Names) -> Vec<u8> {
    let engine = Section::of(h.element, "Engine");
    let game = Section::of(h.element, "Game");
    let mut prim = Bytes::default();

    prim.word(engine.flags("Info", &info_table()))
        .word(engine.flags("Category", &CATEGORY))
        .word(engine.flags("CanUse", &CAN_USE))
        .word(engine.int("Menu"))
        .word(engine.id("ImageIDBase"));

    match h.sub {
        OVERLAY => {
            prim.word(engine.reference("ExplosionID", Registry::Unit, names))
                .word(engine.id("AttachmentPointID"));
        }
        PARTICLE => {
            prim.raw(&particle(engine));
        }
        PROJECTILE => {
            prim.word(engine.reference("ExplosionID", Registry::Unit, names));
        }
        _ => {
            prim.word(engine.reference("AssociatedUnit", Registry::Unit, names));
        }
    }

    // Action script read: type and cProc always 0.
    let function = engine
        .get("Script")
        .and_then(|s| s.attribute("GPLFunction"))
        .unwrap_or_default();

    prim.word(0).word(0).cstr(function);

    if h.sub < OVERLAY {
        prim.raw(&attachments(engine, h.sub, names));
    }

    prim.list(&overrides(engine, names))
        .word(engine.reference("DefaultSound", Registry::Sound, names))
        .word(engine.flags("ShadowType", &SHADOW_TYPE))
        .word(engine.flags("ShadowOptions", &SHADOW_OPTIONS));

    let version = if h.sub == PARTICLE {
        PARTICLE_PRIM
    } else {
        UNIT_PRIM
    };
    let mut parts = vec![head(h), leaf(*b"PRIM", version, &prim.0)];

    match h.sub {
        BUILDING => parts.push(leaf(*b"SECD", BUILDING_SECD, &building(game, names))),
        CHARACTER => parts.push(leaf(*b"SECD", CHARACTER_SECD, &character(game, names))),
        OVERLAY => parts.push(leaf(*b"SECD", OVERLAY_SECD, &overlay(game))),
        _ => {}
    }

    record(Set::Unit, &parts)
}

/// The `ATCH` tree:
/// `EXPL`, `MOVE`, and `WEAP` but on a projectile. Only `Movement`
/// attachments fill a slot here; **Inferred**: `Explosion` and `Weapon`
/// ones fill theirs alike, and no XML has one.
fn attachments(engine: Section<'_, '_>, kind: u32, names: &Names) -> Vec<u8> {
    let movement: Vec<(u32, u32)> = engine
        .all("Attachment")
        .filter(|a| a.attribute("kind") == Some("Movement"))
        .filter_map(|a| {
            let kind = MOVEMENT
                .iter()
                .find(|(n, _)| {
                    a.attribute("type")
                        .is_some_and(|t| n.eq_ignore_ascii_case(t))
                })?
                .1;

            Some((kind, names.resolve(Registry::Movement, a.attribute("ID")?)))
        })
        .take(MOVEMENT_SLOTS)
        .collect();

    let table = |tag: [u8; 4], slots: usize, filled: &[(u32, u32)]| {
        let mut b = Bytes::default();

        for i in 0..slots {
            let (k, id) = filled.get(i).copied().unwrap_or_default();

            b.word(k).word(id);
        }
        part(tag, 0, slots, &b.0, b.0.len())
    };

    let mut tables = vec![
        table(*b"EXPL", EXPLOSION_SLOTS, &[]),
        table(*b"MOVE", MOVEMENT_SLOTS, &movement),
    ];

    if kind != PROJECTILE {
        tables.push(table(*b"WEAP", WEAPON_SLOTS, &[]));
    }

    let inner = tables.concat();

    part(*b"ATCH", 0, tables.len(), &inner, inner.len())
}

/// `SoundOverride type= ID=`: action and sound by name,
/// into an attribute map, so sorted by action id.
fn overrides(engine: Section<'_, '_>, names: &Names) -> Vec<(u32, u32)> {
    let map: BTreeMap<u32, u32> = engine
        .all("SoundOverride")
        .filter_map(|o| {
            Some((
                names.resolve(Registry::Action, o.attribute("type")?),
                names.resolve(Registry::Sound, o.attribute("ID")?),
            ))
        })
        .collect();

    map.into_iter().collect()
}

/// A particle system's 264-byte block, the layout
/// [`super::ParticleSystem`] reads.
fn particle(engine: Section<'_, '_>) -> Vec<u8> {
    let emitter = Section(engine.get("Emitter"));
    let p = Section(engine.get("Particle"));
    let bounds = engine.get("Bounds");
    let corner = |key: &str| vector::<3>(bounds.and_then(|b| b.attribute(key)).unwrap_or_default());
    let mut b = Bytes::default();

    b.word(engine.id("AttachmentPointID"))
        .word(engine.int("InitialParticles"))
        .word(engine.int("MaxParticles"))
        .floats(&corner("min"))
        .floats(&corner("max"));

    b.word(emitter.flags("Type", &emitter_types()))
        .floats(&emitter.vector::<3>("Velocity"))
        .floats(&emitter.vector::<3>("Acceleration"))
        .floats(&emitter.vector::<3>("Offset"))
        .floats(&emitter.vector::<3>("SurfaceNormal"))
        .float(emitter.float("Radius"))
        .float(emitter.float("RadiusVariance"))
        .float(emitter.float("DirectionVariance"))
        .float(emitter.float("Rate"))
        .word(emitter.int("Lifespan"))
        .word(emitter.int("Recycle"))
        .word(emitter.flags("Flags", &emitter_flags()));

    b.word(p.int("Lifespan"))
        .float(p.float("LifespanVariance"))
        .float(p.float("InitialSpeed"))
        .float(p.float("InitialSpeedVariance"))
        .floats(&p.vector::<3>("Gravity"))
        .floats(&p.vector::<4>("BirthColor"))
        .floats(&p.vector::<4>("MidlifeColor"))
        .floats(&p.vector::<4>("DeathColor"))
        .float(p.float("BirthSize"))
        .float(p.float("MaxSize"))
        .float(p.float("DeathSize"))
        .floats(&p.floats("SizeTime", ["in", "out"]))
        .floats(&p.floats("FadeTime", ["in", "out"]))
        .floats(&p.vector::<3>("JitterOrientation"))
        .floats(&p.vector::<3>("JitterDisplacement"))
        .float(p.float("BirthSpinRate"))
        .float(p.float("MidlifeSpinRate"))
        .float(p.float("DeathSpinRate"))
        .floats(&p.floats("SpinTime", ["in", "out"]));

    b.0
}

/// A building's `SECD`: the
/// in-memory `BuildingDesc`, then `Inventory` and
/// `Produces`.
fn building(game: Section<'_, '_>, names: &Names) -> Vec<u8> {
    let mut b = Bytes::default();

    b.word(game.id("DialogID"))
        .word(game.int("Cost"))
        .word(game.reference("UpgradeTo", Registry::Unit, names))
        .word(game.reference("UpgradeFrom", Registry::Unit, names))
        .float(game.float("Multiplier"))
        .word(game.int("IncomeType"))
        .word(game.int("IncomeAmount"))
        .word(game.int("MaxHP"))
        .word(game.int("MaxGuildMembers"))
        .word(game.int("SightRange"))
        .word(game.flags("Flags", &building_flags()))
        .word(game.int("Gold"))
        .word(game.id("NameGenType"))
        .word(game.id("HelpID"))
        .list(&inventory(game));

    // `Produces`: `Unit ID=` by name, one word each.
    let produced: Vec<u32> = game
        .get("Produces")
        .into_iter()
        .flat_map(|p| p.named("Unit"))
        .filter_map(|u| u.attribute("ID"))
        .filter(|id| !id.is_empty())
        .map(|id| names.resolve(Registry::Unit, id))
        .collect();

    b.word(LIST_VERSION).word(len(produced.len()));

    for id in produced {
        b.word(id);
    }

    b.0
}

/// `Inventory`'s `Item ID= Value=`: an attribute map,
/// key the id, value its number, sorted by key.
fn inventory(game: Section<'_, '_>) -> Vec<(u32, u32)> {
    let map: BTreeMap<u32, u32> = game
        .get("Inventory")
        .into_iter()
        .flat_map(|i| i.named("Item"))
        .filter_map(|item| {
            let value = atoi(item.attribute("Value")?);
            let key = item.attribute("ID")?;

            Some((string_id(key).cast_unsigned(), value))
        })
        .collect();

    map.into_iter().collect()
}

/// A character's `SECD`: the
/// in-memory `VehicleDesc`, then its first four lists:
/// `Inventory`, `AllowedWeapon`, `AllowedArmor`, `AllowedSpells`.
fn character(game: Section<'_, '_>, names: &Names) -> Vec<u8> {
    let [range_min, range_max] = game.pair("AttackRange", ["min", "max"]);
    let mut b = Bytes::default();

    b.word(game.id("DialogID"))
        .word(game.int("Cost"))
        .word(game.int("Experience"))
        .word(game.int("MaxHP"))
        .word(game.int("SightRange"))
        .word(game.int("Speed"))
        .word(range_min)
        .word(range_max)
        .word(game.int("Vitality"))
        .word(game.int("Charisma"))
        .word(game.int("Artifice"))
        .word(game.int("WillPower"))
        .word(game.int("Intelligence"))
        .word(game.int("Strength"))
        .word(game.int("MagicResistance"))
        .word(game.int("Attack"))
        .word(game.int("RangedAttack"))
        .word(game.int("Parry"))
        .word(game.int("Dodge"))
        .word(game.int("WeaponBasicDamage"))
        .word(game.int("ArmorBasicDamage"))
        .word(game.int("Gold"))
        .word(game.int("RecruitDelay"))
        .word(game.id("NameGenType"))
        .word(game.flags("Flags", &character_flags()))
        .word(game.id("HelpID"))
        .word(game.int("PrimaryStat"))
        .list(&inventory(game));

    // One entry each when the helper found a value, its index the list's
    // count: 0.
    let gear = |name: &'static str, table: Table<'_>| -> Vec<(u32, u32)> {
        game.matched(name, table)
            .map(|v| vec![(0, v)])
            .unwrap_or_default()
    };

    b.list(&gear("AllowedWeapon", &WEAPONS))
        .list(&gear("AllowedArmor", &ARMOUR));

    // `Spell Value=` by name, else `default` skipped, else its id; 0
    // skipped; the index the count so far.
    let spells: Vec<u32> = game
        .get("AllowedSpells")
        .into_iter()
        .flat_map(|s| s.named("Spell"))
        .filter_map(|s| s.attribute("Value"))
        .filter_map(|v| match names.find(Registry::Action, v) {
            Some(id) => Some(id),
            None if v.eq_ignore_ascii_case(DEFAULT) => None,
            None => Some(string_id(v).cast_unsigned()),
        })
        .filter(|id| *id != 0)
        .collect();
    let indexed: Vec<(u32, u32)> = (0..).zip(spells).collect();

    b.list(&indexed);
    b.0
}

/// An overlay's `SECD`: `DialogID`, `StackPriority`,
/// `Flags`.
fn overlay(game: Section<'_, '_>) -> Vec<u8> {
    let mut b = Bytes::default();

    b.word(game.id("DialogID"))
        .word(game.int("StackPriority"))
        .word(game.flags("Flags", &OVERLAY_FLAGS));
    b.0
}

// --------------------------------------------------------------------------
// Actions and sounds
// --------------------------------------------------------------------------

/// A `DACT` record (action xml read, then the game
/// part action game xml read). `PRIM` is `ActionDesc_Std_Engine`;
/// the script block trails its declared length.
fn action(h: &Header<'_>, names: &Names, sets: &ImageSets) -> Vec<u8> {
    let engine = Section::of(h.element, "Engine");
    let game = Section::of(h.element, "Game");

    // Xml image set: a set's name, else the text's id.
    let image_set = |name: &str| {
        engine.value(name).map_or(0, |v| {
            sets.find(v).unwrap_or_else(|| string_id(v).cast_unsigned())
        })
    };

    // `SoundPhase begin= peak= end=`: phases by name.
    let phases = engine.get("SoundPhase");
    let phase = |key: &str| {
        phases
            .and_then(|p| p.attribute(key))
            .map_or(0, |v| names.resolve(Registry::Phase, v))
    };
    let [distance_min, distance_max] = engine.pair("Distance", ["min", "max"]);
    let [rate_min, rate_max] = engine.pair("Rate", ["min", "max"]);
    let function = engine
        .get("Script")
        .and_then(|s| s.attribute("GPLFunction"))
        .unwrap_or_default();
    let mut prim = Bytes::default();

    prim.word(engine.id("ImageVariation"))
        .word(image_set("ImageSet"))
        .word(engine.reference("Sound", Registry::Sound, names))
        .word(engine.reference("Projectile", Registry::Unit, names))
        .word(engine.reference("Overlay", Registry::Unit, names))
        .word(engine.flags("Info", &ACTION_INFO))
        .word(distance_min)
        .word(distance_max)
        .word(rate_min)
        .word(rate_max)
        .word(phase("begin"))
        .word(phase("peak"))
        .word(phase("end"))
        .word(image_set("CompletionImageSet"))
        .word(0)
        .word(0)
        .word(0)
        .word(0)
        .cstr(function);

    let mut secd = Bytes::default();

    secd.word(game.flags("Flags", &ACTION_FLAGS))
        .word(game.int("EffectorDuration"))
        .word(game.int("TimeoutDuration"))
        .word(game.flags("SpellType", &SPELL_TYPE))
        .word(game.int("CharacterLevel"))
        .word(game.int("SpellRank"))
        .word(game.int("Cost"))
        .cstr(game.value("ValidationScript").unwrap_or_default());

    let parts = [
        head(h),
        part(*b"PRIM", ACTION_PRIM, 0, &prim.0, ACTION_DECLARED),
        leaf(*b"SECD", ACTION_SECD, &secd.0),
    ];

    record(Set::Action, &parts)
}

/// A `DSND` record: category, info, then each `Phase`.
/// **Inferred**: every `Phase` in order; the loop as read fetches the
/// first again, which would not end, so its walk is not
/// understood.
fn sound(h: &Header<'_>, names: &Names) -> Vec<u8> {
    let engine = Section::of(h.element, "Engine");
    let mut prim = Bytes::default();
    let phases: Vec<_> = engine.all("Phase").collect();

    prim.word(engine.id("Category"))
        .word(0)
        .word(len(phases.len()));

    for p in phases {
        let s = Section(Some(p));
        let mut flags = s.flags("Flags", &PHASE_FLAGS);

        // An override present sets its flag bit.
        let set = |name: &str, bit: u32, flags: &mut u32| {
            s.get(name).map_or(0, |_| {
                *flags |= bit;
                s.int(name)
            })
        };
        let volume = set("VolumeOverride", 1 << 1, &mut flags);
        let frequency = set("FrequencyOverride", 1 << 2, &mut flags);
        let pan = set("PanOverride", 1 << 3, &mut flags);
        let [volume_up, volume_down] = s.pair("VolumeVariation", ["up", "down"]);
        let [rate_up, rate_down] = s.pair("FrequencyVariation", ["up", "down"]);
        let [pan_left, pan_right] = s.pair("PanVariation", ["left", "right"]);
        let id = p
            .attribute("ID")
            .map_or(0, |v| names.resolve(Registry::Phase, v));

        prim.word(flags)
            .word(id)
            .word(s.id("Wave"))
            .word(s.reference("Group", Registry::Group, names))
            .word(volume)
            .word(volume_up)
            .word(volume_down)
            .word(frequency)
            .word(rate_up)
            .word(rate_down)
            .word(pan)
            .word(pan_left)
            .word(pan_right)
            .word(s.int("LoopCount"))
            .float(s.float("DistanceModifier"));
    }

    let parts = [
        head(h),
        part(*b"PRIM", SOUND_PRIM, 0, &prim.0, SOUND_DECLARED),
    ];

    record(Set::Sound, &parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atoi_and_atof_read_as_c_does() {
        assert_eq!(atoi(" -12x"), (-12i32).cast_unsigned());
        assert_eq!(atoi("x"), 0);
        assert!((atof("1.25") - 1.25).abs() < f32::EPSILON);
        assert!((atof("2.5e1abc") - 25.0).abs() < f32::EPSILON);
        assert!(atof("abc").abs() < f32::EPSILON);
        assert_eq!(
            vector::<3>("0.2, 0.5").map(f32::to_bits),
            [0.2f32, 0.5, 0.0].map(f32::to_bits)
        );
    }
}
