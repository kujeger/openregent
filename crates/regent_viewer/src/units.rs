//! The unit browser's lists over the corpus, and what it is looking at.
//!
//! The lists resolve every `DUNT` and every interface `IMAG` up front, so
//! they are the viewer's alone: openregent opens the same corpus and never
//! pays for them.

use bevy::prelude::*;
use regent_core::content::imagery;
use regent_core::content::imagery::{Animation, FrameSource, Pose};
use regent_core::content::unit::Unit;
use regent_core::data::catalogue::{Catalogue, Located};
use regent_core::data::desc::UnitKind;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::{INTERFACE_ICON, INTERFACE_PANEL, INTERFACE_PORTRAIT, SetKind};
use regent_core::sim::movement::action::frame_ms;
use regent_core::view::map::ring::{self, Look};
use regent_core::view::panel::portrait::{BAR, Bar, Crown};

use regent_draw::data::Corpus;

/// Which list a row is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitList {
    /// A `DUNT` record and the `IMAG` it names.
    Unit,
    /// A bare `IMAG` record: interface art, and the selection rings.
    Picture,
}

/// One row of the list.
///
/// Flattened up front: the list is drawn every frame.
pub struct Entry {
    /// The `DUNT` record for a unit, the `IMAG` record for a picture.
    pub at: Located,
    pub source: UnitList,
    pub id: FourCc,
    pub name: String,
    pub kind: Option<UnitKind>,
    /// The archive holding the art.
    pub art: Option<String>,
    pub sets: usize,
    pub frames: usize,
    /// What the game draws a picture for, where read: "crown, nothing
    /// selected". Searched too.
    pub uses: Vec<&'static str>,
}

impl Entry {
    /// Whether this row survives the current filters.
    pub fn matches(&self, filter: Filter, search: &str) -> bool {
        let wanted = match filter {
            Filter::All => true,
            Filter::Kind(k) => self.source == UnitList::Unit && self.kind == Some(k),
            Filter::Interface => self.source == UnitList::Picture,
        };

        let search = search.to_lowercase();
        let named = search.is_empty()
            || self.name.to_lowercase().contains(&search)
            || self.id.to_string().to_lowercase().contains(&search)
            || self.uses.iter().any(|u| u.contains(&search));

        wanted && named
    }
}

/// The list's category row: everything, one unit subtype, or interface
/// pictures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Kind(UnitKind),
    Interface,
}

/// A row: which list, and where in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pick {
    pub source: UnitList,
    pub index: usize,
}

/// Pictures whose use the library has read off the binary: record, set id,
/// use. From [`regent_core::view::panel::portrait`] and [`regent_core::view::map::ring`].
fn known() -> [(FourCc, u32, &'static str); 8] {
    let crown = |c: Crown| (c.art().0, u32::from(c.art().1));
    let (gold, gold_set) = crown(Crown::Gold);
    let (expansion, expansion_set) = crown(Crown::Expansion);

    [
        (gold, gold_set, "crown, nothing selected"),
        (
            expansion,
            expansion_set,
            "crown, nothing selected, expansion",
        ),
        (BAR, u32::from(Bar::Plain.set()), "health bar"),
        (BAR, u32::from(Bar::Poisoned.set()), "health bar, poisoned"),
        (Look::Selected.image(), ring::SET, "selection ring"),
        (Look::Flag.image(), ring::SET, "ring, reward flag brackets"),
        (
            Look::Fits.image(),
            ring::SET,
            "ring, fits or under the cursor",
        ),
        (Look::Blocked.image(), ring::SET, "ring, blocked"),
    ]
}

/// What one set of a row is drawn for, where read.
///
/// A unit's `Interface` sets by id (`docs/formats/art.md`); a picture's from
/// [`known`]. Ids 1000 to 1002 mean nothing on a bare picture: `INDs` has a
/// set 1000 that is no portrait.
pub fn purpose(source: UnitList, id: FourCc, set: u32) -> Option<&'static str> {
    let set = u32::from(regent_core::data::imag::set_number(set));

    match source {
        UnitList::Unit if set == u32::from(INTERFACE_PORTRAIT) => Some("portrait"),
        UnitList::Unit if set == u32::from(INTERFACE_PANEL) => Some("hero panel backdrop"),
        UnitList::Unit if set == u32::from(INTERFACE_ICON) => Some("list icon"),
        UnitList::Unit => None,
        UnitList::Picture => known()
            .into_iter()
            .find(|(record, at, _)| *record == id && *at == set)
            .map(|(_, _, said)| said),
    }
}

/// Every use [`known`] gives a picture.
fn uses(id: FourCc) -> Vec<&'static str> {
    known()
        .into_iter()
        .filter(|(record, _, _)| *record == id)
        .map(|(_, _, said)| said)
        .collect()
}

/// Whether an `IMAG` goes in the interface list: it sits in an interface
/// archive (`interfacedata.cam`, `mx_interfacedata.cam`,
/// `addinterface.cam`), or its use is known (the rings, in `maindata.cam`).
fn interface(archive: &str, id: FourCc) -> bool {
    archive.to_lowercase().contains("interface") || !uses(id).is_empty()
}

/// The browser's two lists over the corpus.
#[derive(Resource)]
pub struct Rows {
    pub units: Vec<Entry>,
    /// Interface `IMAG` records, by [`interface`].
    pub pictures: Vec<Entry>,
    /// Units whose descriptor would not read. Empty on every shipped corpus.
    pub unreadable: Vec<String>,
}

impl Rows {
    /// Every unit and interface picture of `corpus`, resolved.
    pub fn open(corpus: &Corpus) -> Self {
        let catalogue = &corpus.catalogue;
        let (mut units, mut unreadable) = (Vec::new(), Vec::new());

        for (at, record) in catalogue.tagged(b"DUNT") {
            let Ok(unit) = Unit::at(catalogue, at) else {
                unreadable.push(record.id.to_string());
                continue;
            };

            units.push(Entry {
                at,
                id: unit.id,
                name: String::from_utf8_lossy(unit.desc.head.name).into_owned(),
                kind: unit.kind,
                art: unit
                    .art
                    .and_then(|l| l.at)
                    .map(|found| catalogue.archives[found.archive].0.to_owned()),
                sets: unit.animations.len(),
                frames: unit.frames().count(),
                uses: Vec::new(),
                source: UnitList::Unit,
            });
        }

        let pictures = pictures(catalogue, &mut unreadable);

        Rows {
            units,
            pictures,
            unreadable,
        }
    }

    /// A row by pick.
    pub fn entry(&self, pick: Pick) -> Option<&Entry> {
        match pick.source {
            UnitList::Unit => self.units.get(pick.index),
            UnitList::Picture => self.pictures.get(pick.index),
        }
    }

    /// One line describing what was loaded. Ambiguous ids matter:
    /// `Catalogue::find` takes the last loaded, in argument order.
    pub fn summary(&self, corpus: &Corpus) -> String {
        // Text tables: without `gpltext.cam`, names silently fall back to
        // numbers.
        format!(
            "{} archives, {} empty, {} ambiguous ids, {} records, {} units, \
             {} interface pictures, {} text tables",
            corpus.catalogue.archives.len(),
            corpus.empty.len(),
            corpus.catalogue.ambiguous().count(),
            corpus.catalogue.records(),
            self.units.len(),
            self.pictures.len(),
            corpus.text.len(),
        )
    }
}

/// The interface list: each `IMAG` [`interface`] keeps, per archive.
///
/// Per location, not per id: one id can sit in two archives. Empty records
/// are skipped, as `regent_cli imag` does; one that will not parse goes to
/// `unreadable`.
fn pictures(catalogue: &Catalogue<'static>, unreadable: &mut Vec<String>) -> Vec<Entry> {
    catalogue
        .tagged(b"IMAG")
        .filter(|(at, record)| {
            !record.data.is_empty() && interface(catalogue.archives[at.archive].0, record.id)
        })
        .filter_map(|(at, record)| {
            let sets = imagery::sets(catalogue, at)
                .inspect_err(|_| unreadable.push(format!("IMAG {}", record.id)))
                .ok()?;

            Some(Entry {
                at,
                source: UnitList::Picture,
                id: record.id,
                name: record.name().into_owned(),
                kind: None,
                art: Some(catalogue.archives[at.archive].0.to_owned()),
                sets: sets.len(),
                frames: sets
                    .iter()
                    .flat_map(|a| &a.poses)
                    .map(|p| p.frames.len())
                    .sum(),
                uses: uses(record.id),
            })
        })
        .collect()
}

/// What the panels are looking at.
#[allow(clippy::struct_excessive_bools, reason = "independent draw toggles")]
#[derive(Resource)]
pub struct Selection {
    /// The row shown.
    pub picked: Option<Pick>,
    /// The resolved unit, cached across frames. `None` for a picture.
    pub resolved: Option<Unit<'static>>,
    /// A picture's sets; empty for a unit, whose sets are its own.
    pub image: Vec<Animation>,
    pub filter: Filter,
    pub search: String,

    /// Which animation set, pose and frame of [`Selection::resolved`].
    ///
    /// Indices; [`Selection::frame`] clamps them.
    pub set: usize,
    pub pose: usize,
    pub stream: u16,
    pub frame: usize,

    /// Whether to draw at the pose's own offset.
    ///
    /// The offset places a stacked layer on its base; off for layer 0.
    pub offset: bool,

    /// Show every pose on a circle, to see facings. See `crate::sprites::ring`.
    pub ring: bool,
    /// The ring's radius, in multiples of the widest sprite in it.
    pub spread: f32,

    /// Mark each pose's attachment points on the picture.
    pub points: bool,
    /// Mark the tile's own anchor.
    ///
    /// The ground point, not a drawing offset: 55 of the base dataset's 59
    /// standing characters have their lowest pixel at or just below it; the
    /// other four fly.
    pub anchor: bool,
}

impl Default for Selection {
    fn default() -> Self {
        Selection {
            picked: None,
            resolved: None,
            image: Vec::new(),
            filter: Filter::All,
            search: String::new(),
            set: 0,
            pose: 0,
            stream: 0,
            frame: 0,
            offset: true,
            ring: false,
            spread: 1.4,
            points: false,
            anchor: false,
        }
    }
}

impl Selection {
    /// Point at a row, resolving it if that is a change.
    pub fn select(&mut self, (corpus, rows): (&Corpus, &Rows), pick: Pick) {
        if self.picked == Some(pick) {
            return;
        }

        self.picked = Some(pick);

        let entry = rows.entry(pick);
        let at = |source| entry.filter(|e| e.source == source).map(|e| e.at);

        self.resolved = at(UnitList::Unit).and_then(|at| Unit::at(&corpus.catalogue, at).ok());
        self.image = at(UnitList::Picture)
            .and_then(|at| imagery::sets(&corpus.catalogue, at).ok())
            .unwrap_or_default();

        // New row: reset to its opening set.
        let (set, pose) = match (&self.resolved, entry) {
            (Some(unit), _) => opening(unit),
            (None, Some(e)) => (known_set(e.id, &self.image), 0),
            (None, None) => (0, 0),
        };

        self.set = set;
        self.pose = pose;
        self.stream = 0;
        self.frame = 0;
    }

    /// The shown row's sets: the unit's, or the picture's.
    pub fn sets(&self) -> &[Animation] {
        self.resolved
            .as_ref()
            .map_or(&self.image, |u| &u.animations)
    }

    /// The animation set currently pointed at.
    pub fn animation(&self) -> Option<&Animation> {
        self.sets().get(self.set)
    }

    /// Show set `set` from its first stream and frame, on its own.
    pub const fn show(&mut self, set: usize) {
        self.set = set;
        self.pose = 0;
        self.stream = 0;
        self.frame = 0;
        self.ring = false;
    }

    /// The pose currently pointed at.
    pub fn pose(&self) -> Option<&Pose> {
        self.animation()?.poses.get(self.pose)
    }

    /// Where the current frame's pixels are.
    pub fn frame(&self) -> Option<FrameSource> {
        self.frame_of(self.pose)
    }

    /// The same, for any pose of the current set (for the ring).
    ///
    /// Clamps: counts differ per pose (Ranger `Die`: 15 frames at one facing,
    /// 16 at the other seven).
    pub fn frame_of(&self, pose: usize) -> Option<FrameSource> {
        let pose = self.animation()?.poses.get(pose)?;
        let stream = pose.stream(self.stream);
        let at = self.frame.min(stream.len().saturating_sub(1));

        stream.get(at).copied()
    }

    /// How many frames the current stream holds.
    pub fn frames(&self) -> usize {
        self.pose().map_or(0, |p| p.stream(self.stream).len())
    }

    /// Game ms each frame of the current set shows, as an order plays it
    /// ([`frame_ms`]); a real ms each at the engine's default pace.
    pub fn frame_ms(&self) -> u32 {
        frame_ms(self.animation().map_or(0, |a| a.playback))
    }

    /// How many poses the current set holds.
    pub fn poses(&self) -> usize {
        self.animation().map_or(0, |a| a.poses.len())
    }

    /// Every pose that the stage should draw, in slot order.
    ///
    /// One entry, or all in ring mode. Called every frame.
    pub fn placement(&self) -> Vec<(usize, Option<FrameSource>)> {
        if self.ring {
            (0..self.poses()).map(|p| (p, self.frame_of(p))).collect()
        } else {
            vec![(self.pose, self.frame())]
        }
    }
}

/// Which slot of a compass rose faces south-west.
///
/// **Inferred.** Only meaningful where poses are facings (a rock's 10 are
/// variants). See [`Selection::ring`].
const SOUTH_WEST: usize = 5;

/// Which set and pose a newly selected unit opens on.
///
/// Characters open on `Walk` facing south-west (`Stand` is often one frame).
/// Others open on their first set.
fn opening(unit: &Unit<'_>) -> (usize, usize) {
    let walk = unit
        .animations
        .iter()
        .position(|set| set.kind == SetKind::Walk)
        .filter(|_| unit.kind == Some(UnitKind::Character));

    match walk {
        // Too few slots for a compass rose.
        Some(set) if unit.animations[set].poses.len() > SOUTH_WEST => (set, SOUTH_WEST),
        Some(set) => (set, 0),
        None => (0, 0),
    }
}

/// The first set of a picture whose use is known, else 0.
fn known_set(id: FourCc, sets: &[Animation]) -> usize {
    sets.iter()
        .position(|a| purpose(UnitList::Picture, id, a.id).is_some())
        .unwrap_or(0)
}

/// Animation playback.
///
/// A frame shows its set's time ([`Selection::frame_ms`]); `IMAG` repeats
/// a reference to hold longer (`repeated_references_are_holds_and_survive`).
#[derive(Resource)]
pub struct Playback {
    pub playing: bool,
    pub looping: bool,
    /// Seconds accumulated towards the next frame.
    pub since: f32,
}

impl Default for Playback {
    fn default() -> Self {
        Playback {
            playing: true,
            looping: true,
            since: 0.0,
        }
    }
}

/// How far apart the ring may spread its poses, in sprite widths.
///
/// Below 1 they overlap, for comparing neighbouring facings.
pub const SPREAD_MIN: f32 = 0.6;
pub const SPREAD_MAX: f32 = 4.0;

/// The six subtypes, for the filter row.
pub const KINDS: [(UnitKind, &str); 6] = [
    (UnitKind::Character, "Character"),
    (UnitKind::Building, "Building"),
    (UnitKind::General, "General"),
    (UnitKind::Projectile, "Projectile"),
    (UnitKind::Overlay, "Overlay"),
    (UnitKind::ParticleSystem, "Particle"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A list row, as [`Rows::open`] makes it.
    fn entry(source: UnitList, id: [u8; 4], name: &str, kind: Option<UnitKind>) -> Entry {
        Entry {
            at: Located {
                archive: 0,
                record: 0,
            },
            source,
            id: FourCc(id),
            name: name.to_owned(),
            kind,
            art: None,
            sets: 1,
            frames: 1,
            uses: match source {
                UnitList::Unit => Vec::new(),
                UnitList::Picture => uses(FourCc(id)),
            },
        }
    }

    fn set(id: u32) -> Animation {
        Animation {
            id,
            kind: SetKind::Interface,
            variant: 0,
            poses: Vec::new(),
            playback: 0,
            nudge: 0,
            sort: regent_core::content::imagery::Sort::WithBase,
            passes: vec![regent_core::data::imag::Pass::First],
        }
    }

    #[test]
    fn interface_archives_and_the_rings_make_the_interface_list() {
        assert!(interface("interfacedata.cam", FourCc(*b"INDe")));
        assert!(interface("mx_interfacedata.cam", FourCc(*b"IX72")));
        assert!(interface("addinterface.cam", FourCc(*b"ZZZZ")));

        // The rings sit in `maindata.cam`.
        for look in [Look::Selected, Look::Flag, Look::Fits, Look::Blocked] {
            assert!(interface("maindata.cam", look.image()), "{look:?}");
        }

        assert!(!interface("maindata.cam", FourCc(*b"AVL1")));
    }

    #[test]
    fn a_units_interface_sets_are_named_by_id() {
        let warrior = FourCc(*b"AVL1");

        assert_eq!(purpose(UnitList::Unit, warrior, 1000), Some("portrait"));
        assert_eq!(
            purpose(UnitList::Unit, warrior, 1001),
            Some("hero panel backdrop")
        );
        assert_eq!(purpose(UnitList::Unit, warrior, 1002), Some("list icon"));
        assert_eq!(purpose(UnitList::Unit, warrior, 0x08), None);
    }

    #[test]
    fn a_bare_pictures_set_1000_is_no_portrait() {
        assert_eq!(purpose(UnitList::Picture, FourCc(*b"INDs"), 1000), None);
        assert_eq!(
            purpose(UnitList::Picture, FourCc(*b"INDe"), 1000),
            Some("crown, nothing selected")
        );
        assert_eq!(
            purpose(UnitList::Picture, FourCc(*b"INBc"), 1010),
            Some("health bar, poisoned")
        );
    }

    #[test]
    fn a_sets_layer_byte_does_not_hide_its_use() {
        assert_eq!(
            purpose(UnitList::Picture, FourCc(*b"AVn8"), 0x0100_0008),
            Some("ring, fits or under the cursor")
        );
    }

    #[test]
    fn the_interface_filter_shows_pictures_and_no_units() {
        let crown = entry(UnitList::Picture, *b"INDe", "hero", None);
        let warrior = entry(
            UnitList::Unit,
            *b"AVL1",
            "Warrior",
            Some(UnitKind::Character),
        );

        assert!(crown.matches(Filter::Interface, ""));
        assert!(!warrior.matches(Filter::Interface, ""));
        assert!(!crown.matches(Filter::Kind(UnitKind::Character), ""));
        assert!(warrior.matches(Filter::Kind(UnitKind::Character), ""));
        assert!(crown.matches(Filter::All, "") && warrior.matches(Filter::All, ""));
    }

    #[test]
    fn search_finds_a_picture_by_id_name_or_use() {
        let crown = entry(UnitList::Picture, *b"INDe", "hero", None);

        for search in ["INDe", "inde", "hero", "crown", "Crown", "nothing selected"] {
            assert!(crown.matches(Filter::All, search), "{search}");
        }

        assert!(!crown.matches(Filter::All, "ring"));
    }

    #[test]
    fn a_picture_opens_on_its_known_set() {
        // A known set need not be first.
        let sets = [set(1003), set(1000), set(1001)];

        assert_eq!(known_set(FourCc(*b"INDe"), &sets), 1);
        assert_eq!(known_set(FourCc(*b"IX72"), &sets), 2);
        assert_eq!(known_set(FourCc(*b"INDs"), &sets), 0);
    }

    /// Another set opens on its first stream: the one picked before may
    /// not be in it, and its slider shows only with two or more.
    #[test]
    fn another_set_opens_on_its_first_stream() {
        let mut selection = Selection {
            stream: 2,
            ..Selection::default()
        };

        selection.show(1);
        assert_eq!(selection.stream, 0);
    }
}
