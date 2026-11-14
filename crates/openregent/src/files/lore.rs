//! Profile lore from shipped unit descriptors: what the Book of Lore
//! (`ui::lore`) lists, and what a profile's marks reveal.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use regent_core::content::dataset::{self, Bytecode, Datasets};
use regent_core::content::quest;
use regent_core::data::desc::{self, CharacterFlag, UnitKind, User};
use regent_core::data::fourcc::FourCc;
use regent_core::data::{cam, payload, smnu};
use regent_core::view::menu::quest_map::Progress;

use super::profile::LoreMark;

/// Kills needed for full Lore details. Ownership bypasses this rule.
const DETAILS_AFTER: u32 = 5;
const BOSS_DETAILS_AFTER: u32 = 1;
pub(crate) const SPAWNS_AFTER: u32 = 10;

pub(crate) const fn spawns_revealed(mark: LoreMark) -> bool {
    mark.owned || mark.kills >= SPAWNS_AFTER
}

/// Entries a quest places, and the quest whose victory reveals all of each.
/// Keys from `quest::offered`; a mod's copy of a quest has its own key, so
/// teaches nothing. Kills count as for any entry: a once-placed lair takes
/// replays to reach them.
const QUEST_TAUGHT: [(FourCc, FourCc); 19] = [
    (FourCc(*b"ABJ4"), FourCc(*b"XQ07")), // Dark Palace, The Siege
    (FourCc(*b"CB01"), FourCc(*b"XQD1")), // Shrine of Darkness, Balance of Twilight
    (FourCc(*b"BBi2"), FourCc(*b"QU17")), // Slave Cross, Free the Slaves
    (FourCc(*b"BB01"), FourCc(*b"MD01")), // Altar to Krolm, Wrath of Krolm
    (FourCc(*b"BBO1"), FourCc(*b"XQ04")), // Ancient Barrow, Legendary Heroes
    (FourCc(*b"BBQ1"), FourCc(*b"XQ03")), // Fortress of Ixmil
    (FourCc(*b"BBT1"), FourCc(*b"XQ08")), // Spire Of Death
    (FourCc(*b"BBa1"), FourCc(*b"QU18")), // Brashnard's Sphere
    (FourCc(*b"BBi1"), FourCc(*b"QU17")), // Slave Pits, Free the Slaves
    (FourCc(*b"BBp1"), FourCc(*b"QU04")), // Crown Site
    (FourCc(*b"BBf1"), FourCc(*b"QU03")), // Hidden Chalice Site
    (FourCc(*b"BBg1"), FourCc(*b"QU16")), // Hidden Ring Site
    (FourCc(*b"BBl1"), FourCc(*b"QU15")), // Tower Prison
    (FourCc(*b"BBd1"), FourCc(*b"QU10")), // Elven Hideout
    (FourCc(*b"BBe1"), FourCc(*b"QU08")), // Magic Sword Site
    (FourCc(*b"BBm1"), FourCc(*b"QU05")), // Witch King Tower
    (FourCc(*b"BBh1"), FourCc(*b"QU14")), // Liche Queen Lair
    (FourCc(*b"BBP1"), FourCc(*b"XQ02")), // Archaic Tomb, Darkness Falls
    (FourCc(*b"BBc1"), FourCc(*b"QU09")), // Dragon King Tomb, spawned by Tomb_victory
];

/// The line under a locked section.
pub(crate) fn unlock_text(code: FourCc, kills: u32, needed: u32) -> String {
    let also = if QUEST_TAUGHT.iter().any(|&(entry, _)| entry == code) {
        ", or win its quest"
    } else {
        ""
    };

    format!("Unlock: {kills}/{needed} kills{also}")
}

/// Whether the profile won the quest that reveals `code`.
pub(crate) fn taught(code: FourCc, progress: &Progress) -> bool {
    QUEST_TAUGHT
        .iter()
        .any(|&(entry, quest)| entry == code && progress.won(quest))
}

/// `mark` with what a won quest teaches: all of `code`, as owning it does.
pub(crate) fn known(code: FourCc, mark: LoreMark, progress: &Progress) -> LoreMark {
    let taught = taught(code, progress);

    LoreMark {
        seen: mark.seen || taught,
        owned: mark.owned || taught,
        ..mark
    }
}

/// Named quest and event bosses.
const BOSS_MONSTERS: [FourCc; 11] = [
    FourCc(*b"BV01"), // Avatar of Krolm
    FourCc(*b"BVc1"), // Url Shekk
    FourCc(*b"BVd1"), // Dirgo
    FourCc(*b"BVe1"), // Rrongol
    FourCc(*b"BVg1"), // Vendral
    FourCc(*b"BVj1"), // Liche Queen
    FourCc(*b"BVk1"), // Witch King
    FourCc(*b"BVl1"), // Abomination
    FourCc(*b"BVt1"), // Rhoden
    FourCc(*b"BVv1"), // Stones
    FourCc(*b"BVw1"), // Styx
];

pub(crate) fn details_needed(code: FourCc) -> u32 {
    if BOSS_MONSTERS.contains(&code) {
        BOSS_DETAILS_AFTER
    } else {
        DETAILS_AFTER
    }
}
const MONSTER_BASIC: [&str; 4] = ["Health", "Magic resistance", "Dodge", "Parry"];
const WRATH_VARIANTS: [FourCc; 4] = [
    FourCc(*b"BV02"),
    FourCc(*b"BV03"),
    FourCc(*b"BV04"),
    FourCc(*b"BV05"),
];

#[derive(Clone, Copy)]
enum BirthChange {
    Double,
    Add(i64),
    Set(i64),
}

/// WrathOfKrolm.gpl birth changes; BV02 uses its minimum hero level of 1.
/// The `STRT` record holding the help texts.
const HELP_TEXTS: FourCc = FourCc(*b"HPTX");

/// `BV03 Krolm_Eagle`.
const KROLM_EAGLE: FourCc = FourCc(*b"BV03");

const WRATH_BIRTH: &[(FourCc, &str, BirthChange)] = &[
    (FourCc(*b"BV02"), "Attack", BirthChange::Add(1)),
    (FourCc(*b"BV02"), "Strength", BirthChange::Add(1)),
    (FourCc(*b"BV02"), "Magic resistance", BirthChange::Add(1)),
    (FourCc(*b"BV02"), "Dodge", BirthChange::Add(1)),
    (FourCc(*b"BV02"), "Parry", BirthChange::Add(1)),
    (FourCc(*b"BV03"), "Health", BirthChange::Double),
    (FourCc(*b"BV03"), "Attack", BirthChange::Add(30)),
    (FourCc(*b"BV04"), "Health", BirthChange::Double),
    (FourCc(*b"BV04"), "Strength", BirthChange::Double),
    (FourCc(*b"BV04"), "Attack", BirthChange::Add(20)),
    (FourCc(*b"BV04"), "Magic resistance", BirthChange::Set(65)),
    (FourCc(*b"BV05"), "Health", BirthChange::Double),
    (FourCc(*b"BV05"), "Dodge", BirthChange::Add(30)),
    (FourCc(*b"BV05"), "Parry", BirthChange::Add(30)),
    (FourCc(*b"BV05"), "Attack", BirthChange::Add(30)),
];

/// Keep the shipped name; label its quest-only lookalike.
pub(crate) fn subtitle(code: FourCc) -> Option<&'static str> {
    WRATH_VARIANTS.contains(&code).then_some("Wrath of Krolm")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Category {
    #[default]
    Monsters,
    Lairs,
    Heroes,
    Henchmen,
    Buildings,
}

impl Category {
    pub(crate) const ALL: [Category; 5] = [
        Category::Monsters,
        Category::Lairs,
        Category::Heroes,
        Category::Henchmen,
        Category::Buildings,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Category::Monsters => "Monsters",
            Category::Lairs => "Lairs",
            Category::Heroes => "Heroes",
            Category::Henchmen => "Henchmen",
            Category::Buildings => "Buildings",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub code: FourCc,
    pub kind: UnitKind,
    pub category: Category,
    pub title: String,
    pub help: Option<String>,
    pub stats: Vec<(&'static str, i64)>,
    pub links: Vec<(&'static str, FourCc)>,
}

impl Entry {
    pub(crate) fn basic(&self, label: &str) -> bool {
        if self.category == Category::Monsters {
            MONSTER_BASIC.contains(&label)
        } else {
            label == "Health"
        }
    }

    pub(crate) fn stat_label<'a>(&self, label: &'a str) -> &'a str {
        if self.category == Category::Monsters {
            match label {
                "Magic resistance" => "Resistance",
                "Parry" => "Defense",
                _ => label,
            }
        } else {
            label
        }
    }

    /// A stat's value as shown: a damage roll as "1–N".
    pub(crate) fn stat_text(&self, label: &str, value: i64) -> String {
        if self.kind == UnitKind::Building && label == DAMAGE {
            format!("1–{value}")
        } else {
            value.to_string()
        }
    }
}

#[derive(Resource, Default)]
pub struct Catalogue(pub BTreeMap<FourCc, Entry>, pub Vec<PathBuf>, Spawned);

/// Spawning buildings' monsters, read on first use: each set of programs
/// is loaded for it, which a start should not wait for.
#[derive(Default)]
pub struct Spawned {
    /// Each distinct set of programs a game loads.
    sets: Vec<Programs>,
    /// Descriptors by folded HEAD name.
    units: BTreeMap<String, (FourCc, UnitKind)>,
    found: std::sync::OnceLock<BTreeMap<FourCc, Vec<FourCc>>>,
}

impl Spawned {
    const fn new(sets: Vec<Programs>, units: BTreeMap<String, (FourCc, UnitKind)>) -> Spawned {
        Spawned {
            sets,
            units,
            found: std::sync::OnceLock::new(),
        }
    }
}

/// A descriptor's HEAD name as the scripts' names are compared: folded.
fn folded(name: &[u8]) -> String {
    String::from_utf8_lossy(name).to_ascii_lowercase()
}

impl Catalogue {
    /// The monsters `code`'s spawn script can spawn ([`spawns`]): a
    /// lair's regular spawns. Not a falling lair's, nor a quest event's.
    /// None where the programs will not read: logged, once.
    pub(crate) fn spawns(&self, code: FourCc) -> &[FourCc] {
        let found = self.2.found.get_or_init(|| {
            spawns(&self.2.sets, &self.2.units).unwrap_or_else(|e| {
                bevy::log::warn!("lore: lair spawns not read: {e}");
                BTreeMap::new()
            })
        });

        found.get(&code).map_or(&[], Vec::as_slice)
    }

    /// Read each shipped archive once, without installed mods.
    pub fn read(hd: &Path) -> Result<Self, String> {
        Self::gathered(hd).map(|(catalogue, _)| catalogue)
    }

    /// [`Catalogue::read`], and what its buildings' attacks were read from.
    fn gathered(hd: &Path) -> Result<(Self, Armoury), String> {
        let shipped = shipped(hd)?;
        let (mut paths, mut programs) = (shipped.archives, shipped.programs);
        let mut used = BTreeSet::new();
        let (mut entries, mut units) = (BTreeMap::new(), BTreeMap::new());
        let mut images = BTreeSet::new();
        let mut help = BTreeMap::new();
        let mut help_panel = None;
        let mut help_strings = None;
        let mut armoury = Armoury::default();

        paths.retain(|path| used.insert(path.clone()));

        for path in &paths {
            // Its videos and sounds left on disk: none is read here.
            let image = cam::Image::read(path).map_err(|e| e.to_string())?;
            if image.is_empty() {
                continue;
            }

            let archive = image
                .archive()
                .map_err(|e| format!("{}: {e}", path.display()))?;

            for (chunk, record) in archive.records() {
                if chunk.tag == b"DUNT" {
                    let part = desc::Part::parse(record.data)
                        .map_err(|e| format!("{} {}: {e}", path.display(), record.id))?;
                    let unit = desc::Unit::parse(&part)
                        .map_err(|e| format!("{} {}: {e}", path.display(), record.id))?;

                    armoury.scripts.insert(
                        record.id,
                        String::from_utf8_lossy(unit.tail().script).into_owned(),
                    );
                    units.insert(folded(unit.head.name), (record.id, unit.kind()));

                    if let Some(entry) = entry(record.id, &unit) {
                        entries.insert(
                            record.id,
                            (
                                entry,
                                unit.character()
                                    .and_then(|c| c.help_id())
                                    .or_else(|| unit.building().and_then(|b| b.help_id())),
                                unit.image_id(),
                                String::from_utf8_lossy(unit.head.name).into_owned(),
                            ),
                        );
                    } else {
                        entries.remove(&record.id);
                    }
                } else if chunk.tag == b"DACT" {
                    armoury
                        .add_action(record.data)
                        .map_err(|e| format!("{} {}: {e}", path.display(), record.id))?;
                } else if chunk.tag == b"IMAG" {
                    images.insert(record.id);
                } else if chunk.tag == b"STRT" && record.id == HELP_TEXTS {
                    let table = payload::StringTable::parse(record.data)
                        .map_err(|e| format!("{} HPTX: {e}", path.display()))?;

                    for key in table.keys() {
                        if let Some(text) = table.get_by_key(key) {
                            help.insert(key, text.into_owned());
                        }
                    }
                } else if record.id == regent_core::view::panel::portrait::HELP {
                    if chunk.tag == b"SMNU" {
                        let panel = smnu::Panel::parse(record.data)
                            .map_err(|e| format!("{} AP95: {e}", path.display()))?;
                        help_panel = panel.control(5000).and_then(|control| control.label());
                    } else if chunk.tag == b"STRT" {
                        let table = payload::StringTable::parse(record.data)
                            .map_err(|e| format!("{} AP95: {e}", path.display()))?;
                        help_strings = Some(
                            (0..table.len())
                                .map(|index| table.get(index).map(std::borrow::Cow::into_owned))
                                .collect::<Vec<_>>(),
                        );
                    }
                }
            }
        }

        let default_help = help_panel.and_then(|index| help_strings.as_ref()?.get(index)?.clone());

        programs.retain(|path| used.insert(path.clone()));
        for path in &programs {
            armoury.read(path)?;
        }

        // Only entries with shipped art can show a portrait.
        let catalogue = Catalogue(
            entries
                .into_iter()
                .filter_map(|(code, (mut entry, topic, image, name))| {
                    image.filter(|id| images.contains(id))?;
                    entry.help = topic
                        .and_then(|id| help.get(&id).cloned())
                        .or_else(|| default_help.clone());

                    // A building's attack is in its `.dat` instance, not its descriptor.
                    if entry.kind == UnitKind::Building {
                        let attack = armoury.tower(&name, &entry.stats);
                        entry.stats.extend(attack);
                    }

                    armoury.names.insert(code, name);
                    Some((code, entry))
                })
                .collect(),
            paths,
            Spawned::new(shipped.sets, units),
        );

        Ok((catalogue, armoury))
    }
}

/// The `.bcd` programs one game loads, in load order.
type Programs = Vec<PathBuf>;

/// The shipped archives and `.bcd` programs: the expansion's, then each
/// quest's. And each distinct set of programs a game loads, the
/// expansion's first.
struct Shipped {
    archives: Vec<PathBuf>,
    programs: Vec<PathBuf>,
    sets: Vec<Programs>,
}

fn shipped(hd: &Path) -> Result<Shipped, String> {
    let install = dataset::Install::new(hd, Bytecode::Install);
    let files = install
        .files(Datasets::Expansion)
        .map_err(|e| e.to_string())?;
    let mut paths = files.archives;
    let mut programs = files.programs.clone();
    let mut sets = vec![files.programs];

    for quest in quest::offered(hd, &[]) {
        let files = install.quest(&quest).map_err(|e| e.to_string())?;
        paths.extend(files.archives);
        programs.extend(files.programs.iter().cloned());

        if !sets.contains(&files.programs) {
            sets.push(files.programs);
        }
    }

    Ok(Shipped {
        archives: paths,
        programs,
        sets,
    })
}

/// Each spawning building's monsters, by code: what its spawn script
/// can spawn (`regent_core::gpl::spawns`), read from each of `sets`'
/// programs; the first set that holds a building says. `units`:
/// descriptors by folded HEAD name.
///
/// A script names a unit by its descriptor's HEAD name; a name that is
/// no character's is left out (an overlay a rat lair spawns). The spawn
/// function's monsters are in code order, a later stage's after them in
/// the script's order (the Spire of Death's).
fn spawns(
    sets: &[Programs],
    units: &BTreeMap<String, (FourCc, UnitKind)>,
) -> Result<BTreeMap<FourCc, Vec<FourCc>>, String> {
    let of_kind = |name: &str, kind: UnitKind| {
        units
            .get(&name.to_ascii_lowercase())
            .filter(|(_, k)| *k == kind)
            .map(|(code, _)| *code)
    };
    let characters = |names: &[String]| -> Vec<FourCc> {
        names
            .iter()
            .filter_map(|name| of_kind(name, UnitKind::Character))
            .collect()
    };
    let mut found = BTreeMap::new();

    for set in sets {
        let images = set
            .iter()
            .map(|path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display())))
            .collect::<Result<Vec<_>, String>>()?;
        let program = regent_core::gpl::load::Program::load(&images).map_err(|e| e.to_string())?;

        for spawner in regent_core::gpl::spawns::spawners(&program) {
            if let Some(lair) = of_kind(&spawner.instance, UnitKind::Building) {
                let mut first = characters(&spawner.names);

                first.sort_unstable();
                first.extend(characters(&spawner.staged));
                found.entry(lair).or_insert(first);
            }
        }
    }

    Ok(found)
}

/// `.dat` instance members, keys folded: text, as the compiler stored them.
type Members = BTreeMap<String, String>;

/// The callback that rolls a building's `.dat` attack (SDK
/// `mx_make_attack.gpl`): `damage` takes `RandomNumber(Strength) + 1` from a
/// building or lair, `gettohit` takes `Ranged` under [`TOWER_ATTACK`].
const TOWER_HIT: &str = "make_attack";
/// The `AttackType` whose to-hit is the attacker's own `Ranged` member.
const TOWER_ATTACK: i64 = 5;

/// Spell blasts' damage, a literal in the script: each callback's
/// `$spell_attack(thisagent, target, 20)` (SDK `GPLMx/TaskModules/Subtasks/mx_Spells.gpl`,
/// `Ixmil_Blast_Hit`, `Spire_Blast_Hit`), which `spelldamage` rolls as
/// `RandomNumber(20) + 1`, at least 1.
const BLAST_DAMAGE: [(&str, i64); 2] = [("Ixmil_Blast_Hit", 20), ("Spire_Blast_Hit", 20)];

/// A damage roll's label, shown as "1–N" ([`Entry::stat_text`]).
const DAMAGE: &str = "Damage";

/// A building's attack action: its projectile's callback.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Shot {
    callback: String,
}

/// What buildings' attacks are read from. `Tower_Attack`, `Spire_Attack`
/// and `Ixmil_Attack` fire the agent's `Attack_Action` member
/// (`$PerformAction`); the descriptor holds no attack.
#[derive(Default)]
struct Armoury {
    /// `.dat` instances by folded name; a later program wins.
    instances: BTreeMap<String, Members>,
    /// Actions' projectiles, by folded action name.
    actions: BTreeMap<String, Option<FourCc>>,
    /// Each unit's callback, by code.
    scripts: BTreeMap<FourCc, String>,
    /// Each entry's descriptor name, which names its `.dat` instance.
    names: BTreeMap<FourCc, String>,
}

impl Armoury {
    /// Add a `DACT` record, by its folded name.
    fn add_action(&mut self, data: &[u8]) -> regent_core::data::error::Result<()> {
        let part = desc::Part::parse(data)?;
        let action = desc::Action::parse(&part)?;

        self.actions.insert(
            String::from_utf8_lossy(action.head.name).to_ascii_lowercase(),
            action.projectile(),
        );
        Ok(())
    }

    /// The attack stats of the building descriptor `name`, its `Sight`
    /// among `stats`; none when it fires no `Attack_Action`.
    fn tower(&self, name: &str, stats: &[(&str, i64)]) -> Vec<(&'static str, i64)> {
        let sight = stats
            .iter()
            .find(|(label, _)| *label == "Sight")
            .map_or(0, |(_, value)| *value);

        self.attack(name)
            .map(|(members, shot)| tower_stats(members, &shot, sight))
            .unwrap_or_default()
    }

    /// Add a `.bcd`'s section 3 instances.
    fn read(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let fail = |e: regent_core::data::error::Error| format!("{}: {e}", path.display());
        let program = regent_core::data::bcd::Container::parse(&bytes).map_err(fail)?;
        let sections = program.sections().map_err(fail)?;
        let data = regent_core::gpl::bcd::DataSection::decode(sections.data.body).map_err(fail)?;

        for proto in data.prototypes {
            let members = proto
                .values
                .iter()
                .map(|(key, value)| (key.to_ascii_lowercase(), value.to_string()))
                .collect();
            self.instances
                .insert(proto.name.to_ascii_lowercase(), members);
        }

        Ok(())
    }

    /// The `.dat` members and attack action of descriptor `name`, when its
    /// instance names an `Attack_Action` the archives hold.
    fn attack(&self, name: &str) -> Option<(&Members, Shot)> {
        let members = self.instances.get(&name.to_ascii_lowercase())?;
        let projectile = self
            .actions
            .get(&members.get("attack_action")?.to_ascii_lowercase())?;
        let callback = projectile
            .and_then(|code| self.scripts.get(&code))
            .cloned()
            .unwrap_or_default();

        Some((members, Shot { callback }))
    }
}

/// A building's attack stats, as the scripts use them. Damage: the `.dat`
/// `Strength` where [`TOWER_HIT`] rolls it, else a [`BLAST_DAMAGE`]. Reach:
/// its own `sight`, the range its scan lists targets in. **Inferred**: the
/// action's `Distance` (1000 for `ballista_bolt`) narrows nothing;
/// openregent's sim reads no `Distance`, the engine's use is unknown.
fn tower_stats(members: &Members, shot: &Shot, sight: i64) -> Vec<(&'static str, i64)> {
    let number = |key: &str| members.get(key).and_then(|text| text.parse::<i64>().ok());
    let rolled =
        shot.callback.eq_ignore_ascii_case(TOWER_HIT) && number("attacktype") == Some(TOWER_ATTACK);
    let damage = if rolled {
        number("strength")
    } else {
        BLAST_DAMAGE
            .iter()
            .find(|(callback, _)| shot.callback.eq_ignore_ascii_case(callback))
            .map(|(_, damage)| *damage)
    };
    let to_hit = number("ranged").filter(|_| rolled);

    [
        damage.map(|value| (DAMAGE, value)),
        to_hit.map(|value| ("Ranged attack", value)),
        Some(("Attack range maximum", sight)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Eligible actors and structures, with fixed quest birth changes.
fn entry(code: FourCc, unit: &desc::Unit<'_>) -> Option<Entry> {
    let (kind, category, mut stats, links) = match unit.kind() {
        UnitKind::Character => {
            let character = unit.character()?;
            (character.max_hp().is_some_and(|hp| hp > 0) && character.has(CharacterFlag::HasHpBar))
                .then(|| {
                    (
                        UnitKind::Character,
                        match character.dialog_id() {
                            Some(regent_core::view::panel::portrait::details::MONSTER_PANEL) => {
                                Category::Monsters
                            }
                            Some(regent_core::view::panel::portrait::details::HERO_PANEL) => {
                                Category::Heroes
                            }
                            _ => Category::Henchmen,
                        },
                        character_stats(code, character),
                        Vec::new(),
                    )
                })?
        }
        UnitKind::Building => {
            let building = unit.building()?;
            let dialog = building.dialog_id();
            let post = [
                regent_core::view::panel::portrait::details::SIGN_PANEL,
                regent_core::view::panel::portrait::details::CHEST_PANEL,
            ];

            (building.max_hp().is_some_and(|hp| hp > 0)
                && !dialog.is_some_and(|id| post.contains(&id)))
            .then(|| {
                let mut links = Vec::new();

                links.extend(building.upgrade_to().map(|code| ("Upgrades to", code)));
                links.extend(building.upgrade_from().map(|code| ("Upgrades from", code)));
                links.extend(
                    building
                        .produces()
                        .into_iter()
                        .map(|code| ("Produces", code)),
                );

                (
                    UnitKind::Building,
                    if unit.can_use(User::Monster) {
                        Category::Lairs
                    } else {
                        Category::Buildings
                    },
                    building_stats(building),
                    links,
                )
            })?
        }
        _ => return None,
    };

    // Hide hero attributes and recruitment fields on monsters and henchmen.
    if matches!(category, Category::Monsters | Category::Henchmen) {
        stats.retain(|(label, _)| {
            !matches!(
                *label,
                "Willpower"
                    | "Vitality"
                    | "Charisma"
                    | "Artifice"
                    | "Recruit cost"
                    | "Recruit delay"
            )
        });
    } else if category == Category::Lairs {
        // Lairs have no player economy.
        stats.retain(|(label, _)| {
            !matches!(*label, "Cost" | "Guild members" | "Income type" | "Income")
        });
    }

    Some(Entry {
        code,
        kind,
        category,
        title: unit.head.title().into_owned(),
        help: None,
        stats,
        links,
    })
}

fn character_stats(code: FourCc, c: desc::Character<'_>) -> Vec<(&'static str, i64)> {
    let fields = [
        ("Health", c.max_hp()),
        ("Sight", c.sight_range()),
        ("Speed", c.speed()),
        ("Attack", c.attack()),
        ("Ranged attack", c.ranged_attack()),
        ("Attack range minimum", c.attack_range_min()),
        ("Attack range maximum", c.attack_range_max()),
        ("Strength", c.strength()),
        ("Intelligence", c.intelligence()),
        ("Willpower", c.will_power()),
        ("Vitality", c.vitality()),
        ("Charisma", c.charisma()),
        ("Artifice", c.artifice()),
        ("Magic resistance", c.magic_resistance()),
        ("Parry", c.parry()),
        ("Dodge", c.dodge()),
        ("Weapon damage", c.weapon_damage()),
        ("Armor damage", c.armor_damage()),
        ("Experience", c.experience()),
        ("Gold", c.gold()),
        ("Recruit cost", c.cost()),
        ("Recruit delay", c.recruit_delay()),
    ];

    let mut stats: Vec<_> = fields
        .into_iter()
        .filter_map(|(label, value)| value.map(|value| (label, i64::from(value))))
        .collect();

    // Quest copies gain these stats at birth; all other monsters keep descriptor values.
    for (label, value) in &mut stats {
        if let Some((_, _, change)) = WRATH_BIRTH
            .iter()
            .find(|(unit, stat, _)| *unit == code && *stat == *label)
        {
            *value = match change {
                BirthChange::Double => *value * 2,
                BirthChange::Add(amount) => *value + amount,
                BirthChange::Set(amount) => *amount,
            };
        }
    }

    if code == KROLM_EAGLE {
        stats.extend([
            ("Movement rate modifier", -300),
            ("Action rate modifier", -250),
        ]);
    }

    stats
}

fn building_stats(b: desc::Building<'_>) -> Vec<(&'static str, i64)> {
    let fields = [
        ("Health", b.max_hp()),
        ("Cost", b.cost()),
        ("Sight", b.sight_range()),
        ("Guild members", b.max_guild_members()),
        ("Gold", b.gold()),
        ("Income type", b.income_type()),
    ];
    let mut stats: Vec<_> = fields
        .into_iter()
        .filter_map(|(label, value)| value.map(|value| (label, i64::from(value))))
        .collect();

    if let Some(income) = b.income_amount() {
        stats.push(("Income", i64::from(income)));
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::data::rgs;

    #[test]
    fn lair_spawns_unlock_after_ten_kills_or_ownership() {
        let marked = |kills, owned| LoreMark {
            seen: true,
            kills,
            owned,
        };

        assert!(!spawns_revealed(marked(9, false)));
        assert!(spawns_revealed(marked(10, false)));
        assert!(spawns_revealed(marked(0, true)));
    }

    #[test]
    fn winning_a_lairs_quest_reveals_all_of_it_with_no_kills() {
        let lairs = [
            (*b"BB01", *b"MD01"), // Altar to Krolm, Wrath of Krolm
            (*b"BBO1", *b"XQ04"), // Ancient Barrow, Legendary Heroes
            (*b"BBQ1", *b"XQ03"), // Fortress of Ixmil
            (*b"BBT1", *b"XQ08"), // Spire Of Death
            (*b"BBa1", *b"QU18"), // Brashnard's Sphere
            (*b"BBi1", *b"QU17"), // Slave Pits, Free the Slaves
            (*b"BBp1", *b"QU04"), // Crown Site
            (*b"BBf1", *b"QU03"), // Hidden Chalice Site
            (*b"BBg1", *b"QU16"), // Hidden Ring Site
            (*b"BBl1", *b"QU15"), // Tower Prison
            (*b"BBd1", *b"QU10"), // Elven Hideout
            (*b"BBe1", *b"QU08"), // Magic Sword Site
            (*b"BBm1", *b"QU05"), // Witch King Tower
            (*b"BBh1", *b"QU14"), // Liche Queen Lair
            (*b"BBP1", *b"XQ02"), // Archaic Tomb, Darkness Falls
            (*b"BBc1", *b"QU09"), // Dragon King Tomb
        ];

        for (lair, quest) in lairs.map(|(lair, quest)| (FourCc(lair), FourCc(quest))) {
            let mut progress = Progress::default();
            assert!(!known(lair, LoreMark::default(), &progress).seen, "{lair}");

            progress.win(quest, 1);
            let mark = known(lair, LoreMark::default(), &progress);
            assert!(mark.seen && mark.owned, "{lair}");
            assert_eq!(mark.kills, 0, "{lair}");
            assert!(spawns_revealed(mark), "{lair}");
            assert!(mark.owned || mark.kills >= details_needed(lair), "{lair}");
        }
    }

    /// Also an old profile's: its `lore.toml` entry keeps the kill.
    #[test]
    fn one_kill_alone_reveals_no_quest_taught_lair() {
        let kept: LoreMark = toml::from_str("seen = true\nkills = 1\n").expect("a lore entry");

        for (lair, _) in QUEST_TAUGHT {
            let mark = known(lair, kept, &Progress::default());
            assert!(mark.seen && !mark.owned, "{lair}");
            assert!(mark.kills < details_needed(lair), "{lair}");
            assert!(!spawns_revealed(mark), "{lair}");
        }
    }

    #[test]
    fn a_quest_taught_entry_says_so_where_it_is_locked() {
        assert_eq!(
            unlock_text(FourCc(*b"BBe1"), 1, DETAILS_AFTER),
            "Unlock: 1/5 kills, or win its quest"
        );
        assert_eq!(
            unlock_text(FourCc(*b"BBH1"), 1, DETAILS_AFTER),
            "Unlock: 1/5 kills"
        );
    }

    /// Each quest that teaches an entry has one definition and places that
    /// entry, bar the tomb its script spawns; a taught lair stands in no
    /// other offered quest.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn quest_taught_entries_stand_in_their_quests() {
        const SCRIPTED: FourCc = FourCc(*b"BBc1");

        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let places = |quest: &quest::Quest, code: FourCc| {
            let path = quest.template_path().expect("a quest's map");
            let bytes = std::fs::read(path).expect("a quest's map");
            let map = rgs::File::parse(&bytes)
                .and_then(|file| file.map())
                .expect("a quest map");

            map.units
                .iter()
                .flatten()
                .flat_map(|pattern| &pattern.placements)
                .any(|place| place.id == code)
        };
        let offered = quest::offered(&hd, &[]);

        for (entry, key) in QUEST_TAUGHT {
            let (own, others): (Vec<_>, Vec<_>) = offered.iter().partition(|q| q.key == key);
            assert_eq!(own.len(), 1, "{key}");
            assert_eq!(places(own[0], entry), entry != SCRIPTED, "{key}: {entry}");

            if entry.0.starts_with(b"BB") {
                let also: Vec<_> = others.iter().filter(|q| places(q, entry)).collect();
                assert!(also.is_empty(), "{entry} also in {:?}", also[0].key);
            }
        }
    }

    /// `TOMB_DRAGON`, the id in QU09's map, starts `Tomb_victory`, which
    /// spawns the Dragon King Tomb.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_tomb_quests_script_spawns_its_tomb() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let path = hd.join("SDK/OriginalQuests/GPLMx/Rules/mx_Epic_Quest_Scripts.gpl");
        let source = std::fs::read_to_string(path).expect("quest scripts");
        let body = |name: &str| {
            let start = source.find(&format!("function {name}(")).expect(name);
            let rest = &source[start + 1..];
            rest.split("\nfunction ").next().expect("a body").to_owned()
        };

        assert!(body("TOMB_DRAGON").contains("$Tomb_victory"));
        assert!(body("Tomb_victory").contains("\"dragon_tomb\""));

        let quests = quest::offered(&hd, &[]);
        let tomb = quests
            .iter()
            .find(|q| q.key == FourCc(*b"QU09"))
            .expect("QU09");
        assert_eq!(tomb.name, "TOMB_DRAGON");
    }

    #[test]
    fn quest_victory_teaches_its_entries() {
        let (shrine, cross) = (FourCc(*b"CB01"), FourCc(*b"BBi2"));
        let mut progress = Progress::default();
        assert!(!taught(shrine, &progress));

        progress.win(FourCc(*b"XQD1"), 1);
        assert!(taught(shrine, &progress));
        assert!(!taught(cross, &progress));

        progress.win(FourCc(*b"QU17"), 1);
        assert!(taught(cross, &progress));
        assert!(!taught(FourCc(*b"BBH1"), &progress));
    }

    #[test]
    fn winning_the_siege_reveals_all_of_the_dark_palace() {
        let (palace, siege) = (FourCc(*b"ABJ4"), FourCc(*b"XQ07"));
        let mut progress = Progress::default();
        let before = known(palace, LoreMark::default(), &progress);
        assert!(!before.seen && !before.owned);

        progress.win(siege, 1);
        let after = known(palace, LoreMark::default(), &progress);
        assert!(after.seen && after.owned);
        assert!(!known(FourCc(*b"ABJ1"), LoreMark::default(), &progress).seen);
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn quest_taught_entries_are_in_the_book() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let catalogue = Catalogue::read(&hd).expect("shipped data");

        for (entry, _) in QUEST_TAUGHT {
            assert!(catalogue.0.contains_key(&entry), "{entry}");
        }
        assert_eq!(catalogue.0[&FourCc(*b"ABJ4")].title, "Dark Palace");
    }

    /// The Spire of Death's spawn function is set by its quest's script
    /// as its spires fall: every stage's monsters are listed, the first
    /// stage's in code order, the later ones' in the script's.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn spire_lore_includes_every_scripted_spawn() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let catalogue = Catalogue::read(&hd).expect("shipped data");

        assert_eq!(
            catalogue.spawns(FourCc(*b"BBT1")),
            [
                FourCc(*b"BVL1"),
                FourCc(*b"BVL2"),
                FourCc(*b"BVL3"),
                FourCc(*b"BVL4"),
                FourCc(*b"BVo1"),
                FourCc(*b"BVJ1"),
                FourCc(*b"BVT1"),
                FourCc(*b"BVY1"),
                FourCc(*b"BVX1"),
                FourCc(*b"BVx1"),
                FourCc(*b"BVm1"),
            ]
        );
    }

    /// A lair's spawns are read from its spawn script: monsters all, an
    /// overlay a rat lair spawns left out; a lair with its own spawn
    /// function lists each unit that names; a tower lair spawns none.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn lair_spawns_are_read_from_the_spawn_scripts() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let catalogue = Catalogue::read(&hd).expect("shipped data");
        let spawns = |code: &[u8; 4]| catalogue.spawns(FourCc(*code));
        let lairs = catalogue
            .0
            .values()
            .filter(|entry| entry.category == Category::Lairs);

        for lair in lairs {
            for spawn in catalogue.spawns(lair.code) {
                assert_eq!(
                    catalogue.0.get(spawn).map(|entry| entry.category),
                    Some(Category::Monsters),
                    "{} -> {spawn}",
                    lair.code
                );
            }
        }

        // Graveyard_Spawn, Snake_Pit_Spawn, Rats_Nest; Krolm's altar is its quest's.
        assert_eq!(spawns(b"BBJ1"), [FourCc(*b"BVT1"), FourCc(*b"BVZ1")]);
        assert_eq!(spawns(b"BBS1"), [FourCc(*b"BVO1"), FourCc(*b"BVp1")]);
        assert_eq!(
            spawns(b"BBx1"),
            [
                FourCc(*b"BVH1"),
                FourCc(*b"BVQ1"),
                FourCc(*b"BVq1"),
                FourCc(*b"BVr1"),
                FourCc(*b"BVs1"),
            ]
        );
        assert_eq!(spawns(b"BB01"), [FourCc(*b"BV02")]);
        assert!(spawns(b"BBQ1").is_empty() && spawns(b"BBy1").is_empty());
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn ordinary_lair_spawns_match_sdk() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let catalogue = Catalogue::read(&hd).expect("shipped data");
        let sdk = hd.join("SDK");
        let mut names = BTreeMap::new();
        for file in [
            "OriginalQuests/Data/M_Buildings.xml",
            "OriginalQuests/Data/M_Characters.xml",
            "OriginalQuests/DataMX/MX_Buildings.xml",
            "OriginalQuests/DataMX/MX_Characters.xml",
            "Example/Data/WrathOfKrolm_Buildings.xml",
            "Example/Data/WrathOfKrolm_Characters.xml",
        ] {
            let source = std::fs::read_to_string(sdk.join(file)).expect(file);
            let xml = regent_core::data::xml::parse(&source).expect(file);
            for unit in xml.named("Description") {
                if let (Some(name), Some(id)) = (unit.attribute("Name"), unit.attribute("ID")) {
                    let code = FourCc(*<&[u8; 4]>::try_from(id.as_bytes()).expect("fourcc"));
                    names.insert(name.to_ascii_lowercase(), code);
                }
            }
        }

        let mut checked = BTreeSet::new();
        for file in [
            "OriginalQuests/GPL/Building_Data.dat",
            "OriginalQuests/GPLMx/mx_Building_Data.dat",
            "Example/GPL/WrathOfKrolm_Data.dat",
        ] {
            let source = std::fs::read_to_string(sdk.join(file)).expect(file);
            let parsed = regent_core::gpl::dat::parse(&source);
            assert!(parsed.is_ok(), "{file}: {:?}", parsed.errors);

            for prototype in parsed.prototypes {
                let Some(&lair) = names.get(&prototype.name.to_ascii_lowercase()) else {
                    continue;
                };
                let listed = catalogue.0.get(&lair).map(|entry| entry.category);

                if listed != Some(Category::Lairs) {
                    continue;
                }

                let field = |key: &str| {
                    prototype
                        .values
                        .iter()
                        .find_map(|(name, value)| name.eq_ignore_ascii_case(key).then_some(value))
                };
                if matches!(
                    field("Spawn_Function"),
                    Some(regent_core::gpl::dat::Value::Word("Spawn"))
                ) {
                    let expected = match field("Spawn_Type") {
                        Some(regent_core::gpl::dat::Value::Word(name))
                            if !name.eq_ignore_ascii_case("xx") =>
                        {
                            vec![*names.get(&name.to_ascii_lowercase()).expect("spawn name")]
                        }
                        _ => Vec::new(),
                    };
                    assert_eq!(
                        catalogue.spawns(lair),
                        expected,
                        "{file}: {}",
                        prototype.name
                    );
                    checked.insert(lair);
                }
            }
        }

        assert!(checked.len() >= 25, "checked only {} lairs", checked.len());
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn monster_lore_omits_recruitment() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let catalogue = Catalogue::read(&hd).expect("shipped data");
        let has = |entry: &Entry, label| entry.stats.iter().any(|(name, _)| *name == label);

        assert!(
            catalogue
                .0
                .values()
                .filter(|entry| entry.category == Category::Monsters)
                .all(|entry| !has(entry, "Recruit cost") && !has(entry, "Recruit delay"))
        );

        let adept = &catalogue.0[&FourCc(*b"AVA1")];
        assert!(has(adept, "Recruit cost") && has(adept, "Recruit delay"));
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn monster_lore_omits_hero_stats() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let catalogue = Catalogue::read(&hd).expect("shipped data");
        let labels = ["Willpower", "Vitality", "Charisma", "Artifice"];
        let has = |entry: &Entry, label| entry.stats.iter().any(|(name, _)| *name == label);

        for entry in catalogue
            .0
            .values()
            .filter(|entry| entry.category == Category::Monsters)
        {
            assert!(
                labels.into_iter().all(|label| !has(entry, label)),
                "{}",
                entry.code
            );
        }

        let adept = &catalogue.0[&FourCc(*b"AVA1")];
        assert!(
            ["Willpower", "Vitality", "Artifice"]
                .into_iter()
                .all(|label| has(adept, label))
        );
        assert!(
            !has(&catalogue.0[&FourCc(*b"AVh1")], "Charisma"),
            "the Hooligan"
        );
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn henchman_lore_omits_hero_and_recruitment_stats() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let catalogue = Catalogue::read(&hd).expect("shipped data");
        let hidden = [
            "Willpower",
            "Vitality",
            "Charisma",
            "Artifice",
            "Recruit cost",
            "Recruit delay",
        ];

        for entry in catalogue
            .0
            .values()
            .filter(|entry| entry.category == Category::Henchmen)
        {
            assert!(
                entry.stats.iter().all(|(label, _)| !hidden.contains(label)),
                "{}",
                entry.code
            );
        }
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn lair_lore_omits_economy() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let catalogue = Catalogue::read(&hd).expect("shipped data");
        let labels = ["Cost", "Guild members", "Income type", "Income"];
        let has = |entry: &Entry, label| entry.stats.iter().any(|(name, _)| *name == label);

        for entry in catalogue
            .0
            .values()
            .filter(|entry| entry.category == Category::Lairs)
        {
            assert!(
                labels.into_iter().all(|label| !has(entry, label)),
                "{}",
                entry.code
            );
        }

        let market = &catalogue.0[&FourCc(*b"ABH1")];
        assert!(
            ["Cost", "Income type", "Income"]
                .into_iter()
                .all(|label| has(market, label))
        );
        assert!(has(&catalogue.0[&FourCc(*b"ABV1")], "Guild members"));
    }

    #[test]
    fn boss_details_need_one_kill() {
        for code in [
            *b"BV01", *b"BVd1", *b"BVe1", *b"BVg1", *b"BVj1", *b"BVk1", *b"BVc1", *b"BVt1",
            *b"BVl1", *b"BVv1", *b"BVw1",
        ] {
            assert_eq!(details_needed(FourCc(code)), 1, "{code:?}");
        }
        assert_eq!(details_needed(FourCc(*b"BVa1")), DETAILS_AFTER);
        assert_eq!(details_needed(FourCc(*b"BVF1")), DETAILS_AFTER);
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn unique_subtype_is_not_a_boss_rule() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let path = hd.join("SDK/OriginalQuests/GPLMx/mx_Monster_Data.dat");
        let source = std::fs::read_to_string(path).expect("monster prototypes");
        let parsed = regent_core::gpl::dat::parse(&source);
        assert!(parsed.is_ok(), "{:?}", parsed.errors);

        let unique: BTreeSet<_> = parsed
            .prototypes
            .iter()
            .filter(|p| {
                p.values.iter().any(|(key, value)| {
                    key.eq_ignore_ascii_case("subtype")
                        && matches!(value, regent_core::gpl::dat::Value::Word(word) if word.eq_ignore_ascii_case("unique"))
                })
            })
            .map(|p| p.name)
            .collect();
        assert_eq!(
            unique,
            BTreeSet::from([
                "Black_Phantom",
                "Dirgo",
                "Liche_Queen",
                "Rrongol",
                "Url_Shekk",
                "Vendral",
                "Witch_King",
            ])
        );
        assert!(!BOSS_MONSTERS.contains(&FourCc(*b"BVa1")));

        let catalogue = Catalogue::read(&hd).expect("shipped data");
        assert!(BOSS_MONSTERS.iter().all(|code| {
            catalogue
                .0
                .get(code)
                .is_some_and(|entry| entry.category == Category::Monsters)
        }));
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn wrath_copies_use_birth_stats() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let catalogue = Catalogue::read(&hd).expect("shipped data");

        let cases = [
            (
                *b"BV02",
                &[
                    ("Health", 20),
                    ("Attack", 86),
                    ("Strength", 26),
                    ("Magic resistance", 21),
                    ("Dodge", 56),
                    ("Parry", 46),
                ][..],
            ),
            (
                *b"BV03",
                &[
                    ("Health", 50),
                    ("Attack", 55),
                    ("Movement rate modifier", -300),
                    ("Action rate modifier", -250),
                ][..],
            ),
            (
                *b"BV04",
                &[
                    ("Health", 90),
                    ("Attack", 70),
                    ("Strength", 32),
                    ("Magic resistance", 65),
                ][..],
            ),
            (
                *b"BV05",
                &[("Health", 90), ("Attack", 80), ("Dodge", 65), ("Parry", 70)][..],
            ),
        ];

        for (code, expected) in cases {
            let entry = &catalogue.0[&FourCc(code)];
            for &(label, value) in expected {
                assert_eq!(
                    entry
                        .stats
                        .iter()
                        .find(|(name, _)| *name == label)
                        .map(|(_, n)| *n),
                    Some(value),
                    "{code:?} {label}"
                );
            }
        }

        for (code, health) in [
            (*b"BVG1", 25),
            (*b"BVF1", 45),
            (*b"BVK1", 45),
            (*b"BVg1", 500),
        ] {
            let entry = &catalogue.0[&FourCc(code)];
            assert_eq!(
                entry
                    .stats
                    .iter()
                    .find(|(name, _)| *name == "Health")
                    .map(|(_, n)| *n),
                Some(health),
                "{code:?}"
            );
        }
    }

    /// The Goblin Watchtower's `.dat` attack (`mx_Building_Data.dat`), its
    /// `ballista_bolt` projectile's `make_attack`, and its sight 255.
    #[test]
    fn a_tower_shows_its_dat_attack() {
        let members: Members = [
            ("attack_action", "Ballista_Bolt"),
            ("strength", "15"),
            ("attacktype", "5"),
            ("ranged", "85"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
        let shot = Shot {
            callback: "make_attack".to_owned(),
        };

        assert_eq!(
            tower_stats(&members, &shot, 255),
            [
                ("Damage", 15),
                ("Ranged attack", 85),
                ("Attack range maximum", 255),
            ]
        );
    }

    /// The Spire of Death's `Strength` 0 and `Ranged` 255 roll nothing:
    /// `Spire_Blast_Hit` calls `spell_attack` with a fixed 20.
    #[test]
    fn a_spell_blast_shows_its_script_damage() {
        let members: Members = [
            ("attack_action", "Spire_Blast_One"),
            ("strength", "0"),
            ("attacktype", "5"),
            ("ranged", "255"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
        let shot = Shot {
            callback: "Spire_Blast_Hit".to_owned(),
        };

        assert_eq!(
            tower_stats(&members, &shot, 300),
            [("Damage", 20), ("Attack range maximum", 300)]
        );
    }

    /// A damage roll reads as its range: `RandomNumber(n) + 1` is 1 to n.
    #[test]
    fn damage_shows_as_its_roll() {
        let entry = Entry {
            code: FourCc(*b"BBy1"),
            kind: UnitKind::Building,
            category: Category::Lairs,
            title: String::new(),
            help: None,
            stats: Vec::new(),
            links: Vec::new(),
        };

        assert_eq!(entry.stat_text("Damage", 15), "1–15");
        assert_eq!(entry.stat_text("Health", 150), "150");
    }

    /// Every lore building that fires an `Attack_Action` shows its damage
    /// and its reach, its own sight.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn every_dat_attack_shows_in_the_lore() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let (catalogue, armoury) = Catalogue::gathered(&hd).expect("shipped data");
        let attackers: Vec<_> = catalogue
            .0
            .values()
            .filter(|entry| entry.kind == UnitKind::Building)
            .filter_map(|entry| {
                let (_, shot) = armoury.attack(armoury.names.get(&entry.code)?)?;
                Some((entry, shot))
            })
            .collect();
        let stat = |entry: &Entry, label: &str| {
            entry
                .stats
                .iter()
                .find(|(found, _)| *found == label)
                .map(|(_, value)| *value)
        };
        let silent: Vec<_> = attackers
            .iter()
            .filter(|(entry, _)| {
                stat(entry, "Damage").is_none()
                    || stat(entry, "Attack range maximum") != stat(entry, "Sight")
            })
            .map(|(entry, _)| (entry.code, entry.title.as_str()))
            .collect();
        let callbacks: Vec<_> = attackers
            .iter()
            .map(|(entry, shot)| (entry.code, shot.callback.as_str()))
            .collect();

        assert!(silent.is_empty(), "attacks the lore omits: {silent:?}");
        assert_eq!(
            callbacks,
            [
                (FourCc(*b"ABB1"), "make_attack"),
                (FourCc(*b"ABb1"), "make_attack"),
                (FourCc(*b"BBQ1"), "Ixmil_Blast_Hit"),
                (FourCc(*b"BBT1"), "Spire_Blast_Hit"),
                (FourCc(*b"BBy1"), "make_attack"),
            ]
        );
    }

    /// The SDK's `function` named `name` in `file`, parsed.
    fn sdk_function(file: &str, name: &str, check: impl FnOnce(&regent_core::gpl::ast::Item<'_>)) {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let sdk = hd.join(crate::files::env::SDK_IN_HD);
        let bytes = std::fs::read(sdk.join(file)).expect("SDK source");
        let source = String::from_utf8_lossy(&bytes);
        let parsed = regent_core::gpl::parse(&source);
        let item = parsed
            .script
            .items
            .iter()
            .find(|item| item.name().eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("{name} in {file}"));

        check(item);
    }

    /// The calls to `$callee` in `item`: their arguments.
    fn calls<'a>(item: &'a regent_core::gpl::ast::Item<'a>, callee: &str) -> Vec<Vec<String>> {
        use regent_core::gpl::ast::ExprKind;

        let mut found = Vec::new();
        item.visit_exprs(&mut |expr| {
            if let ExprKind::Call {
                callee: called,
                args,
            } = &expr.kind
                && matches!(called.kind, ExprKind::Func(name) if name.eq_ignore_ascii_case(callee))
            {
                found.push(args.iter().map(|arg| format!("{:?}", arg.kind)).collect());
            }
        });
        found
    }

    /// [`BLAST_DAMAGE`]: each blast's `spell_attack` passes its damage as a
    /// literal, and `spelldamage` rolls `RandomNumber(damage) + 1`.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn blast_damage_is_the_sdks() {
        const SPELLS: &str = "OriginalQuests/GPLMx/TaskModules/Subtasks/mx_Spells.gpl";

        for (callback, damage) in BLAST_DAMAGE {
            sdk_function(SPELLS, callback, |item| {
                let attacks = calls(item, "spell_attack");

                assert_eq!(attacks.len(), 1, "{callback}");
                assert_eq!(attacks[0][2], format!("Int({damage})"), "{callback}");
            });
        }
    }

    /// A tower's reach is its own sight: `Tower_Attack`, `Spire_Attack` and
    /// `Ixmil_Attack` list targets with `list_enemies_seen(thisagent,
    /// SightRange)`, whose `ListObjects` measures from the tower.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn towers_aim_within_their_own_sight() {
        const TOWER: &str = "OriginalQuests/GPLMx/TaskModules/Buildings/mx_Tower.gpl";
        const QUESTS: &str = "OriginalQuests/GPLMx/Rules/Quests_3.gpl";

        for (file, name) in [
            (TOWER, "Tower_Attack"),
            (QUESTS, "Spire_Attack"),
            (QUESTS, "Ixmil_Attack"),
        ] {
            sdk_function(file, name, |item| {
                let lists = calls(item, "list_enemies_seen");

                assert_eq!(lists.len(), 1, "{name}");
                assert!(lists[0][1].contains("ATTRIB_SightRange"), "{name}");
            });
        }

        sdk_function(TOWER, "list_enemies_seen", |item| {
            let ranges: Vec<_> = calls(item, "listobjects")
                .into_iter()
                .map(|args| args[2].clone())
                .collect();

            assert_eq!(ranges, [r#"Ident("range")"#, r#"Ident("range")"#]);
        });
    }

    /// "Damage 1–N" is the scripts' roll: `damage` takes a building's
    /// `RandomNumber(Attacker's "Strength") + 1`, `spelldamage`
    /// `RandomNumber(damage) + 1`.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn tower_damage_is_the_sdks_roll() {
        const MAKE_ATTACK: &str = "OriginalQuests/GPLMx/TaskModules/Subtasks/mx_make_attack.gpl";

        sdk_function(MAKE_ATTACK, "damage", |item| {
            assert!(
                calls(item, "randomnumber")
                    .iter()
                    .any(|args| args[0].contains(r#"Str("Strength")"#))
            );
        });
        sdk_function(MAKE_ATTACK, "spelldamage", |item| {
            assert_eq!(calls(item, "randomnumber"), [[r#"Ident("damage")"#]]);
        });
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn shipped_lore_uses_descriptors() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let catalogue = Catalogue::read(&hd).expect("shipped data");
        let units = catalogue
            .0
            .values()
            .filter(|entry| entry.kind == UnitKind::Character)
            .count();
        let buildings = catalogue.0.len() - units;

        assert_eq!((units, buildings), (76, 102));
        let minotaur = catalogue
            .0
            .values()
            .find(|entry| entry.title.contains("Minotaur"))
            .expect("Minotaur");
        assert_eq!(minotaur.category, Category::Monsters);
        for (code, category) in [
            (FourCc(*b"BBH1"), Category::Lairs),
            (FourCc(*b"AVA1"), Category::Heroes),
            (FourCc(*b"AVg1"), Category::Henchmen),
            (FourCc(*b"ABJ1"), Category::Buildings),
        ] {
            assert_eq!(
                catalogue.0.get(&code).map(|entry| entry.category),
                Some(category)
            );
        }
        assert!(
            minotaur
                .help
                .as_ref()
                .is_some_and(|text| !text.trim().is_empty())
        );
        assert!(
            catalogue
                .0
                .values()
                .filter(|entry| entry.category == Category::Monsters)
                .all(|entry| {
                    MONSTER_BASIC
                        .iter()
                        .all(|label| entry.stats.iter().any(|(found, _)| found == label))
                })
        );
        assert!(catalogue.0.values().all(|entry| !entry.stats.is_empty()));
        assert!(catalogue.0.values().any(|entry| entry.help.is_some()));
        assert!(catalogue.0.values().any(|entry| !entry.links.is_empty()));

        let art = regent_draw::data::catalogue(&catalogue.1).expect("portrait data");
        let missing: Vec<_> = catalogue
            .0
            .keys()
            .filter(|code| {
                regent_draw::art::read::sprite(&art, regent_draw::art::Pic::Portrait(**code))
                    .is_none()
            })
            .collect();
        assert!(missing.is_empty(), "missing portraits: {missing:?}");
    }
}
