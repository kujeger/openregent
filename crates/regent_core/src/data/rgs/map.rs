//! A quest map: the recipe a map is generated from.
//!
//! No tile grid or heightfield: map size, terrain regions, faction starts and
//! lair contents. The ground is generated at quest start from region patterns
//! pointing into a chart ([`super::chart`]).
//!
//! # Sequential, and version-dependent
//!
//! No offsets; fields follow each other, some only from a given version. A
//! correct reader consumes the file exactly (test
//! `every_shipped_quest_reads_to_its_last_byte`).
//!
//! ```text
//! [16-byte framing]
//! cstr  quest             `DAY_OF_RECKONING`, the id `.qdd` also uses
//! char[4] code            `Dayx`
//! char[4] pattern
//! u32 u32                 a word, then the map number
//! u32 u32                 the map's size
//! u32 n  ( u32 key  spawner ) x n
//! u32 n  ( u32 key  slot ) x n
//! u32 u32 u32             three words, one of them a quest-type code
//! u32 n  ( region ) x n
//! u32 n  ( force ) x n
//! unit pattern set x 2
//! char[4]                 `Rel@`, and only in the later versions
//! ```
//!
//! # Where the field layout came from
//!
//! `BrandonWill`'s `QuestMapGenerator`, decompiled from `RGSeditor.exe`. A
//! hypothesis source, confirmed by this reader consuming every byte of all 36
//! shipped quests across three versions.
//!
//! Field names are theirs unless better known. A `word` has no justified name.

use crate::data::cursor::Cursor;
use crate::data::error::{Error, Result};
use crate::data::fourcc::FourCc;

/// Which optional fields a format version carries.
///
/// From `QuestMapGenerator`'s version map, narrowed to the three shipped. Other
/// tags are refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Versions {
    force: u32,
    spawner: u32,
    item: u32,
}

impl Versions {
    /// `None` for a version this reader has never seen.
    const fn of(tag: u8) -> Option<Self> {
        match tag {
            b'6' => Some(Versions {
                force: 7,
                spawner: 2,
                item: 2,
            }),
            b'9' | b'a' => Some(Versions {
                force: 8,
                spawner: 3,
                item: 3,
            }),
            _ => None,
        }
    }
}

/// The force versions that add a unit pattern's sizes, trailer and flag.
///
/// **Binary** — layout pattern read's `jb` gates against `RGC5` at
/// layout pattern read, `RGC6` and `RGC8`.
const FORCE_SIZES: u32 = 5;
const FORCE_TRAILER: u32 = 6;
const FORCE_FLAG: u32 = 8;

/// One thing a lair may release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnItem {
    pub id: FourCc,
    /// Named `spawn_level` by the reference.
    pub level: u32,
    pub words: [u32; 2],
    /// Present from item version 2.
    pub active: Option<u32>,
    /// Present from item version 3.
    pub resource: Option<u32>,
}

/// A team: a faction's name and what it spawns.
#[derive(Debug, Clone)]
pub struct Team<'a> {
    pub code: FourCc,
    pub name: &'a [u8],
    pub active: u32,
    pub expanding: u32,
    pub items: Vec<SpawnItem>,
}

/// A lair's settings, and the team behind it.
///
/// Per the reference, unchecked: max hit points, base spawn rate (ms),
/// dispersion (pixels), one labelled "not implemented", then a hit-rate
/// reduction. Hence unnamed in the struct.
#[derive(Debug, Clone)]
pub struct Spawner<'a> {
    pub words: [u32; 4],
    /// Present from spawner version 2.
    pub extra: Option<u32>,
    pub team: Team<'a>,
    /// Present from spawner version 3.
    pub names: Vec<FourCc>,
}

/// A player or AI slot.
#[derive(Debug, Clone)]
pub struct Slot<'a> {
    pub key: u32,
    pub name: &'a [u8],
    pub active: u32,
    /// The reference calls this starting gold.
    pub gold: u32,
    pub word: u32,
    pub items: Vec<u32>,
}

/// One region of the map: which chart pattern lays its ground down.
///
/// `NONE` marks an unused slot, as most are.
#[derive(Debug, Clone)]
pub struct Region<'a> {
    pub code: FourCc,
    /// Absent when the code is `NONE`.
    pub team: Option<Team<'a>>,
}

/// One zone of the map, and the chart patterns that dress it.
///
/// A zone names three chart patterns:
///
/// ```text
/// "Day " "Day " "Day #1\0"    code, code, name -- the chart's record header
/// "xDay"                      a landscape pattern: what to scatter
/// "#Arr"                      a surface pattern:   what ground to lay
/// "Bump"                      a height profile:    what shape to give it
/// ```
///
/// `xDay` is `#Day_Arid`, `#Arr` `#Arrid_Mixed_Surface`, `Bump` `Bumpy
/// Flatlands`.
///
/// The reference calls these force entries (editor naming); they carry
/// terrain, so zones here.
#[derive(Debug, Clone)]
pub struct Zone<'a> {
    pub code: FourCc,
    pub name: &'a [u8],
    /// A landscape pattern: the objects scattered over this zone.
    pub landscape: FourCc,
    /// A surface pattern: the ground itself.
    pub surface: FourCc,
    /// A height profile: `Bump`, `Moun`, `FS01`.
    pub height: FourCc,
}

/// One unit instance placed by a pattern.
#[derive(Debug, Clone)]
pub struct Placement<'a> {
    pub id: FourCc,
    pub word: u32,
    pub description: &'a [u8],
    /// Which cells the generator may put it in, as letters `A` to `Y`.
    ///
    /// A letter names a block of the five by five grid in reading order: `A`
    /// `(0, 0)`, `E` top right, `M` centre, `Y` bottom right. Several letters
    /// mean the generator picks one.
    ///
    /// Block of letter turns
    /// `((c - 'A') % 5, (c - 'A') / 5)` by the map's [`Orientation`] before
    /// scaling by block size.
    ///
    /// A bare byte 1, 3, 5 or 7 instead is a map edge: see
    /// [`Placement::edge`].
    pub cells: &'a [u8],
}

/// The map edge a Wandering Monsters placement's monsters walk in from.
///
/// **Binary** — bare placement materialise takes a
/// placement whose first cell byte `c` has `c - 1 < 8`, and creates its
/// block's buildings off the map, at `x = -1000 - (c - 1) / 2`, `y = -1000`,
/// in thirty-seconds of a cell (switch; even `c` leaves `x`
/// unset). They skip `BuildingRec`: no pad, no flatten. `SpawnUnit`
/// in gpl spawn unit spawns from such a lair on an edge instead, by its `x`:
///
/// ```text
///   c   lair x   spawn point           edge
///   1   -1000    (rng(w - 1), 0)       north
///   3   -1001    (w - 1, rng(h - 1))   east
///   5   -1002    (rng(w - 1), h - 1)   south
///   7   -1003    (0, rng(h - 1))       west
/// ```
///
/// Clockwise from north, not the edge midpoints of a three by three grid.
/// **Corpus**: all 130 such cells in the two freestyle quests are one of the
/// four, in Wandering Monsters records only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    North,
    East,
    South,
    West,
}

impl Edge {
    /// All four, by cell byte.
    pub const ALL: [(u8, Edge); 4] = [
        (1, Edge::North),
        (3, Edge::East),
        (5, Edge::South),
        (7, Edge::West),
    ];

    /// The off-map `x` the lair stands at, thirty-seconds of a cell.
    ///
    /// **Binary** — `-1000` to `-1003`, the switch arms in bare placement materialise to
    /// bare placement materialise; `SpawnUnit` tests `x < -999` and `x > -1004` first.
    #[must_use]
    pub const fn lair_x(self) -> i32 {
        OFF_MAP - self as i32
    }
}

/// Where an off-map lair stands, thirty-seconds of a cell: its `y`, and its
/// `x` for [`Edge::North`].
///
/// **Binary** — bare placement materialise writes `0xfffffc18`.
pub const OFF_MAP: i32 = -1000;

/// The map is divided into this many blocks each way for placement.
///
/// World generate charts builds `RGSForceChart` with a literal `5, 5`
/// at every map size; region seed place divides by it too.
pub const BLOCKS: u32 = 5;

impl Placement<'_> {
    /// Blocks this placement may use, as `(column, row)` before orientation;
    /// apply [`Orientation::turn`] for the real map.
    ///
    /// Letters outside `A..=Y` are skipped, as the force chart skips a
    /// placement whose first cell is outside them; a bare
    /// number is an [`Edge`].
    pub fn blocks(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.cells
            .iter()
            .filter(|c| c.is_ascii_uppercase())
            .map(|c| u32::from(c - b'A'))
            .filter(|i| *i < BLOCKS * BLOCKS)
            .map(|i| (i % BLOCKS, i / BLOCKS))
    }

    /// The edge an off-map placement's monsters enter by, from its first
    /// cell byte; `None` for a lettered one.
    #[must_use]
    pub fn edge(&self) -> Option<Edge> {
        let first = *self.cells.first()?;

        Edge::ALL
            .iter()
            .find(|(byte, _)| *byte == first)
            .map(|(_, edge)| *edge)
    }

    /// Where a block starts and ends on a map of `side` cells.
    ///
    /// Integer division, as in the engine: at 512 cells block 2 spans 204 to
    /// 306.
    #[must_use]
    pub const fn span(block: u32, side: u32) -> (u32, u32) {
        let step = side / BLOCKS;

        (block * step, (block + 1) * step)
    }
}

/// A named group of units, with the lairs among them.
///
/// **Binary** — the engine's `RGSLayoutPattern`. The pattern set reader
/// reads a count, then per record a `u32` map key and the pattern, which
/// fills the object in file order. So each field is placed by the reader,
/// not matched by name:
///
/// ```text
/// u32     faction      the map key
/// char[4] code
/// cstr    name
/// u32     alternate    kept as a byte: word == 1
/// u32     words[0]     the pattern set kind, kept if <= 10
/// char[4] ground
/// u32     words[1]
/// placements           0x30 bytes each
/// lairs
/// u32     sizes        from RGC5, else 3
/// u32 x 3 trailer      from RGC6; four more words become bytes
/// u32     flag         from RGC8, kept as a byte: word == 1
/// ```
///
/// The gates compare the force version the map's tag selects, stored by
/// the quest file reader: `RGC8` for `RGM9`
/// and `RGMa`, `RGC7` for `RGM6` to `RGM8`.
#[derive(Debug, Clone)]
pub struct UnitPattern<'a> {
    pub faction: FourCc,
    /// The record's own code again, `RGSPattern`.
    ///
    /// Not the ground ([`UnitPattern::ground`]). Equals
    /// [`UnitPattern::faction`] in all 220 of `mx_freestyle.q`'s unit patterns.
    pub code: FourCc,
    pub name: &'a [u8],
    /// Whether a placed block stamps [`UnitPattern::ground`]; see
    /// [`UnitPattern::stamps`].
    pub alternate: u32,
    /// What the record is for, and how many players it is for.
    ///
    /// `words[0]` is the player count on a Player Forces entry, 1 to 4:
    /// "Frontier Settlement" 1, "Frontier Towns" 4, "Fortified Palace" 1,
    /// "Fortified Palaces" 2. Single-player offers only the 1s.
    ///
    /// 9 on Enemy Forces, 10 on Wandering Monsters. Pattern kind
    /// is the getter; freestyle dialog refresh
    /// buckets the dialog's three force lists on these. Lands are
    /// regions, picked by another field. See [`UnitPattern::players`] and
    /// [`crate::sim::freestyle::Setup::monsters`].
    ///
    /// `words[1]` is the placement grid step,.
    pub words: [u32; 2],
    /// The ground a placed block stands on, where the record carries
    /// [`UnitPattern::alternate`];.
    ///
    /// **Binary** — region seed place's block phase
    /// reads the pattern's ground, writes that code over
    /// the block, adds it to the zone table and seeds a region.
    ///
    /// A zone or chart pattern, set 0 only. `Icex` "Ice Cave" reads `$Dta`
    /// (`$Dtar_Snow`): every Wild Expanse map grows snow over `Icex`'s block
    /// (see [`Placement::cells`]). `Fron` reads `Ferb`, a chart pattern. Set 1
    /// records (dialog entries) and set 0's `Anci` have `NONE`, which the block
    /// phase tests.
    ///
    /// May name a zone outside the chosen land: zone of
    /// searches all 108 of the quest's zones.
    pub ground: FourCc,
    pub placements: Vec<Placement<'a>>,
    /// Keyed `1000 * placement + unit`: the unit-th unit of the placement-th
    /// placement's block.
    ///
    /// **Binary** — bare placement materialise looks a
    /// lair up by that sum. **Corpus**: `Harb` places `Ania`, three dens, six
    /// times; its keys run 0, 1, 2, 1000, 1001, 1002 and on.
    pub spawners: Vec<(u32, Spawner<'a>)>,
    /// The map sizes a scenario is for,; see [`UnitPattern::side`].
    ///
    /// Present from force version 5; the engine takes [`SIZES_DEFAULT`]
    /// below it.
    pub sizes: Option<u32>,
    /// Present from force version 6: three words then four flags.
    pub trailer: Option<[u32; 7]>,
    /// Present from force version 8;, a byte: word == 1.
    /// **Binary**: freestyle setup's Random picks skip a pattern
    /// with it; a quest never runs that, so
    /// it plays no part in a quest's map.
    pub flag: Option<u32>,
}

/// How the five by five placement grid is turned on one map.
///
/// **Binary** — block of letter switches on it,
/// over the square's eight symmetries: identity, two mirrors, half turn, two
/// diagonal flips, two quarter turns. Force chart build sets it with
/// `rng_next(8)`, the first draw after the map number seeds the generator, so
/// it depends on the map number alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Orientation(pub u8);

impl Orientation {
    /// All eight, in the switch's own order.
    pub const ALL: [Orientation; 8] = [
        Orientation(0),
        Orientation(1),
        Orientation(2),
        Orientation(3),
        Orientation(4),
        Orientation(5),
        Orientation(6),
        Orientation(7),
    ];

    /// Where a block of the lettered grid lands under this orientation.
    ///
    /// From the switch; outside 0 to 7 is identity, as its
    /// default.
    #[must_use]
    pub const fn turn(self, at: (u32, u32)) -> (u32, u32) {
        self.on_span(at, BLOCKS - 1)
    }

    /// The same eight symmetries on any square grid, not just the blocks.
    ///
    /// Block `b` spans cells `b*side/5` to `(b+1)*side/5`, and `last - b` its
    /// mirror, so a grid symmetry is a map symmetry. `last` is the highest
    /// coordinate: `side` for cells, and `side` for vertices too, since there
    /// are `side + 1` of them.
    #[must_use]
    pub const fn on_span(self, (x, y): (u32, u32), last: u32) -> (u32, u32) {
        match self.0 {
            1 => (last - x, y),
            2 => (x, last - y),
            3 => (last - y, last - x),
            4 => (y, x),
            5 => (last - y, x),
            6 => (last - x, last - y),
            7 => (y, last - x),
            _ => (x, y),
        }
    }

    /// The orientation that undoes this one.
    ///
    /// Six are self-inverse; 5 and 7 (quarter turns) invert each other. Found
    /// by search, so it agrees with [`Orientation::on_span`].
    #[must_use]
    pub fn inverse(self) -> Orientation {
        Self::ALL
            .into_iter()
            .find(|candidate| self.after(*candidate) == Orientation(0))
            .unwrap_or(Orientation(0))
    }

    /// The single orientation that is `first` and then `self`.
    ///
    /// Always exists (the group is closed). Found with two probe points no
    /// symmetry fixes together.
    #[must_use]
    pub fn after(self, first: Orientation) -> Orientation {
        const PROBES: [(u32, u32); 2] = [(1, 2), (3, 7)];
        const LAST: u32 = 10;

        Self::ALL
            .into_iter()
            .find(|candidate| {
                PROBES.into_iter().all(|at| {
                    candidate.on_span(at, LAST) == self.on_span(first.on_span(at, LAST), LAST)
                })
            })
            .unwrap_or(Orientation(0))
    }
}

/// The file word the reader turns into a set flag.
///
/// **Binary** — layout pattern read stores `word == 1` for
/// `alternate`, the four player flags and `flag` (a compare with `0x01` at
/// layout pattern read). Any other value reads as clear.
pub const FLAG_SET: u32 = 1;

/// [`UnitPattern::sizes`] where the file has none.
///
/// **Binary** — layout pattern read writes 3, below `RGC5`.
pub const SIZES_DEFAULT: u32 = 3;

/// The two [`UnitPattern::sizes`] bits the engine tests, and the map side
/// each gives.
///
/// **Binary** — freestyle setup tests bit `0x02`, then
/// `0x04` in freestyle setup, on the scenario, loading the side pairs for
/// 256 and 512; neither set loads map side pairs (128). The first set bit wins.
pub const SIZE_BITS: [(u32, u32); 2] = [(0x02, 256), (0x04, 512)];

/// The side a scenario with neither [`SIZE_BITS`] bit gets.
pub const SIZE_SMALL: u32 = 128;

impl UnitPattern<'_> {
    /// How many players this Player Forces entry is for, if it is one.
    ///
    /// `None` for Enemy Forces entries and lands, where the field is a kind.
    #[must_use]
    pub const fn players(&self) -> Option<u32> {
        match self.words[0] {
            n @ 1..=4 => Some(n),
            _ => None,
        }
    }

    /// Whether placing this block stamps its [`UnitPattern::ground`].
    ///
    /// **Binary** — the block phase of region seed place
    /// gates on a byte the reader sets from
    /// `alternate == 1`, and on the ground not being `NONE`.
    #[must_use]
    pub fn stamps(&self) -> bool {
        self.alternate == FLAG_SET && self.ground != b"NONE"
    }

    /// The map side this record gives as the scenario.
    ///
    /// **Binary** — see [`SIZE_BITS`]. Not the name's size letter: two
    /// expansion scenarios named `- S` carry 3, so the engine gives them 256.
    #[must_use]
    pub fn side(&self) -> u32 {
        let sizes = self.sizes.unwrap_or(SIZES_DEFAULT);

        SIZE_BITS
            .iter()
            .find(|(bit, _)| sizes & bit != 0)
            .map_or(SIZE_SMALL, |(_, side)| *side)
    }
}

/// A parsed quest map.
#[derive(Debug, Clone)]
pub struct Map<'a> {
    pub tag: FourCc,
    /// The quest id, which is also what `.qdd` and `RGSMissionTemplate` use.
    pub quest: &'a [u8],
    pub code: FourCc,
    pub pattern: FourCc,
    /// `words[1]` is the map number, world: 0 on
    /// every shipped quest, so the map takes the clock.
    pub words: [u32; 2],
    /// The map's size. 128, 256 or 512 on everything shipped.
    pub size: (u32, u32),
    pub spawners: Vec<(u32, Spawner<'a>)>,
    pub slots: Vec<Slot<'a>>,
    /// Three words before the regions; the last is a quest-type code.
    pub trailer: [u32; 3],
    pub regions: Vec<Region<'a>>,
    pub zones: Vec<Zone<'a>>,
    /// Two sets, kept apart as in the editor.
    pub units: [Vec<UnitPattern<'a>>; 2],
}

/// A set 1 record's `words[0]` on an Enemy Forces entry.
///
/// **Binary** — freestyle dialog refresh buckets
/// the dialog's force lists on it; see [`UnitPattern::words`].
pub const ENEMY_FORCES: u32 = 9;

/// A set 1 record's `words[0]` on a Wandering Monsters entry; see
/// [`ENEMY_FORCES`].
pub const WANDERING_MONSTERS: u32 = 10;

/// The [`UnitPattern::trailer`] flag a single-player dialog offers an
/// Enemy Forces or Wandering Monsters entry on. **Game**: the sized Enemy
/// Forces entries it is set on are `local/game-text/enemy_forces.txt`, the dialog's list
/// written down from the game.
pub const ONE_PLAYER_FLAG: usize = 3;

impl UnitPattern<'_> {
    /// The rating a Random pick tests, trailer word `+0x7c`;
    /// `None` below force version 6.
    #[must_use]
    pub fn rating(&self) -> Option<u32> {
        self.trailer.map(|t| t[2])
    }

    /// Whether a Random pick keeps it out: `RGC8`'s flag,.
    #[must_use]
    pub fn kept_out(&self) -> bool {
        self.flag == Some(FLAG_SET)
    }

    /// Whether it is for `players` players: four trailer flags, one a
    /// count.
    #[must_use]
    pub fn for_players(&self, players: u32) -> bool {
        let flag = ONE_PLAYER_FLAG + players.saturating_sub(1) as usize;

        self.trailer
            .and_then(|t| t.get(flag).copied())
            .is_some_and(|f| f == FLAG_SET)
    }
}

impl<'a> Map<'a> {
    /// Read a quest map body — everything after the 16-byte framing.
    ///
    /// # Errors
    ///
    /// If the version is unknown, a field runs past the end, or the file is not
    /// consumed exactly.
    pub fn parse(tag: FourCc, body: &'a [u8]) -> Result<Self> {
        let v = Versions::of(tag.as_bytes()[3]).ok_or(Error::BadValue {
            what: "unknown quest map version",
            value: u64::from(tag.as_bytes()[3]),
        })?;

        let mut c = Cursor::new(body);

        let quest = c.cstr("quest string")?;
        let code = c.fourcc("quest tag")?;
        let pattern = c.fourcc("quest tag")?;
        let words = [c.u32("quest word")?, c.u32("quest word")?];
        let size = (c.u32("quest word")?, c.u32("quest word")?);

        let spawners = keyed_spawners(&mut c, v)?;

        let mut slots = Vec::new();

        for _ in 0..c.count("slot count")? {
            slots.push(slot(&mut c)?);
        }

        let trailer = [
            c.u32("quest word")?,
            c.u32("quest word")?,
            c.u32("quest word")?,
        ];

        let mut regions = Vec::new();

        for _ in 0..c.count("region count")? {
            regions.push(region(&mut c, v)?);
        }

        let mut zones = Vec::new();

        for _ in 0..c.count("zone count")? {
            zones.push(zone(&mut c)?);
        }

        let first = unit_patterns(&mut c, v)?;

        // The second set is absent in the oldest files, which just end.
        let second = if c.left() >= 4 {
            unit_patterns(&mut c, v)?
        } else {
            Vec::new()
        };

        // `Rel@`, present only in the later versions.
        if c.left() >= 4 {
            c.fourcc("quest tag")?;
        }

        if c.done() {
            Ok(Map {
                tag,
                quest,
                code,
                pattern,
                words,
                size,
                spawners,
                slots,
                trailer,
                regions,
                zones,
                units: [first, second],
            })
        } else {
            Err(Error::BrokenInvariant {
                what: "quest map read to its end",
                expected: body.len() as u64,
                found: c.at() as u64,
            })
        }
    }

    /// Every region that names a pattern rather than sitting empty.
    pub fn used_regions(&self) -> impl Iterator<Item = &Region<'a>> + '_ {
        self.regions.iter().filter(|r| r.team.is_some())
    }
}

fn keyed_spawners<'a>(c: &mut Cursor<'a>, v: Versions) -> Result<Vec<(u32, Spawner<'a>)>> {
    let mut out = Vec::new();

    for _ in 0..c.count("spawner count")? {
        let key = c.u32("quest word")?;

        out.push((key, spawner(c, v)?));
    }

    Ok(out)
}

fn spawner<'a>(c: &mut Cursor<'a>, v: Versions) -> Result<Spawner<'a>> {
    let words = [
        c.u32("quest word")?,
        c.u32("quest word")?,
        c.u32("quest word")?,
        c.u32("quest word")?,
    ];
    let extra = (v.spawner >= 2).then(|| c.u32("quest word")).transpose()?;
    let team = team(c, v)?;

    let mut names = Vec::new();

    if v.spawner >= 3 {
        for _ in 0..c.count("spawner name count")? {
            names.push(c.fourcc("quest tag")?);
        }
    }

    Ok(Spawner {
        words,
        extra,
        team,
        names,
    })
}

fn team<'a>(c: &mut Cursor<'a>, v: Versions) -> Result<Team<'a>> {
    let code = c.fourcc("quest tag")?;
    let name = c.cstr("quest string")?;
    let active = c.u32("quest word")?;
    let expanding = c.u32("quest word")?;

    let mut items = Vec::new();

    for _ in 0..c.count("spawn item count")? {
        items.push(SpawnItem {
            id: c.fourcc("quest tag")?,
            level: c.u32("quest word")?,
            words: [c.u32("quest word")?, c.u32("quest word")?],
            active: (v.item >= 2).then(|| c.u32("quest word")).transpose()?,
            resource: (v.item >= 3).then(|| c.u32("quest word")).transpose()?,
        });
    }

    Ok(Team {
        code,
        name,
        active,
        expanding,
        items,
    })
}

fn slot<'a>(c: &mut Cursor<'a>) -> Result<Slot<'a>> {
    let key = c.u32("quest word")?;
    let name = c.cstr("quest string")?;
    let active = c.u32("quest word")?;
    let gold = c.u32("quest word")?;
    let word = c.u32("quest word")?;

    let mut items = Vec::new();

    for _ in 0..c.count("slot item count")? {
        items.push(c.u32("quest word")?);
    }

    Ok(Slot {
        key,
        name,
        active,
        gold,
        word,
        items,
    })
}

fn region<'a>(c: &mut Cursor<'a>, v: Versions) -> Result<Region<'a>> {
    let code = c.fourcc("quest tag")?;

    // `NONE` is an unused slot and carries nothing after it.
    let team = if code == b"NONE" {
        None
    } else {
        Some(team(c, v)?)
    };

    Ok(Region { code, team })
}

fn zone<'a>(c: &mut Cursor<'a>) -> Result<Zone<'a>> {
    let code = c.fourcc("quest tag")?;

    // The code repeated, as in every RGS record.
    c.fourcc("quest tag")?;

    let zone = Zone {
        code,
        name: c.cstr("quest string")?,
        landscape: c.fourcc("quest tag")?,
        surface: c.fourcc("quest tag")?,
        height: c.fourcc("quest tag")?,
    };

    // Two words, unread.
    c.u32("quest word")?;
    c.u32("quest word")?;

    Ok(zone)
}

fn unit_patterns<'a>(c: &mut Cursor<'a>, v: Versions) -> Result<Vec<UnitPattern<'a>>> {
    let mut out = Vec::new();

    for _ in 0..c.count("unit pattern count")? {
        let faction = c.fourcc("quest tag")?;
        let code = c.fourcc("quest tag")?;
        let name = c.cstr("quest string")?;
        let alternate = c.u32("quest word")?;
        let word = c.u32("quest word")?;
        let ground = c.fourcc("quest tag")?;
        let word2 = c.u32("quest word")?;

        let mut placements = Vec::new();

        for _ in 0..c.count("placement count")? {
            let id = c.fourcc("quest tag")?;
            let word = c.u32("quest word")?;
            let description = c.cstr("quest string")?;
            let n = c.count("placement cell count")?;

            placements.push(Placement {
                id,
                word,
                description,
                cells: c.bytes("quest bytes", n)?,
            });
        }

        let spawners = keyed_spawners(c, v)?;
        let sizes = (v.force >= FORCE_SIZES)
            .then(|| c.u32("quest word"))
            .transpose()?;

        let trailer = if v.force >= FORCE_TRAILER {
            let mut tail = [0u32; 7];

            for word in &mut tail {
                *word = c.u32("quest word")?;
            }

            Some(tail)
        } else {
            None
        };

        let flag = (v.force >= FORCE_FLAG)
            .then(|| c.u32("quest word"))
            .transpose()?;

        out.push(UnitPattern {
            faction,
            code,
            name,
            alternate,
            words: [word, word2],
            ground,
            placements,
            spawners,
            sizes,
            trailer,
            flag,
        });
    }

    Ok(out)
}
