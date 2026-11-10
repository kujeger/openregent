//! What a sound cue plays, and how loud and where: sound records joined to
//! their waves, and the engine's mixing law. **Binary** (`MajestyHD.exe`)
//! unless marked.
//!
//! ```text
//!  Cue (world) ──> Sounds::play ──> Play: wave, looped, voice, distance, group
//!  view centre, radius ──> mix ──> volume 0..10,000, pan ±667 ──> gains
//! ```
//!
//! The listener is the view's centre in cells, set as the view moves.
//! A sound is heard within [`radius`] of it: `SoundViewRadius` view widths.
//! Its volume falls linearly, 33 dB at the edge; its pan comes from the
//! angle off the listener's facing, which nothing sets from its default
//! `+x` (listener new; both method calls pass none), so a sound north or
//! south of the centre pans, one east or west does not. openregent faces
//! [`Facing::SOUTH`] instead, so a sound pans by its side of the screen
//! (`docs/openregent/enhancements.md`, no switch).

use std::collections::{HashMap, HashSet};

use crate::data::attrib;
use crate::data::desc::{self, Phase as PhaseRecord, PhaseFlag, SoundGroup, UnitKind};
use crate::data::error::{Error, Result};
use crate::data::fourcc::FourCc;
use crate::sim::world::sound::{self, Voice};

use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;

/// `DistanceModifier` 10,000: full volume, centred, wherever it is
/// (sound mix zeroes the distance).
pub const GLOBAL: f32 = 10_000.0;

/// 10,001: the owner's own units' at full volume, heard by no one else
/// differently.
pub const OWNER_GLOBAL: f32 = 10_001.0;

/// 10,002: only the owner hears it, at full volume.
pub const OWNER_ONLY: f32 = 10_002.0;

/// The top of the engine's volume scale; `DirectSound` gets `v - 10,000`
/// hundredths of a dB.
pub const FULL: i32 = 10_000;

/// Volume lost at the edge of hearing, hundredths of a dB (3300.0).
pub const FALLOFF: f32 = 3300.0;

/// Pan at the edge, 90° off the facing, hundredths of a dB.
pub const PAN_EDGE: f32 = 667.0;

/// `SoundViewRadius`, view widths (sound view radius; `GOPT` agrees).
pub const SOUND_VIEW_RADIUS: f32 = 1.5;

/// Added to [`SOUND_VIEW_RADIUS`] zoomed out, where the view's scale is
/// 0.5.
pub const ZOOMED_OUT_EXTRA: f32 = 1.0;

/// A view pixel to cells: the 32 of `fixed_div(width << 16, 0x200000)`.
pub const CELL_PX: f32 = 32.0;

/// Below this a distance or angle is zero (degrees to radians, 1e-6).
const EPSILON: f32 = 1e-6;

/// The view's zoom, as the radius reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zoom {
    Regular,
    Out,
}

/// How far from the view's centre a sound is heard, cells, for a view
/// `width` pixels wide.
///
/// ```
/// use regent_core::view::sound::{Zoom, radius};
///
/// assert_eq!(radius(640.0, Zoom::Regular), 30.0);
/// assert_eq!(radius(640.0, Zoom::Out), 50.0);
/// ```
#[must_use]
pub fn radius(width: f32, zoom: Zoom) -> f32 {
    let views = match zoom {
        Zoom::Regular => SOUND_VIEW_RADIUS,
        Zoom::Out => SOUND_VIEW_RADIUS + ZOOMED_OUT_EXTRA,
    };

    width / CELL_PX * views
}

/// Whose unit a sound comes from, against the local player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Local,
    Other,
}

/// Whether the local player sees a sound's unit (method of the local
/// player, `GenUnit`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seen {
    Yes,
    No,
}

impl Seen {
    /// By the unit's sight word for the local player
    /// (`World::sight_word`): seen above 0 (`jnle` in `GenUnit`). The
    /// game's sight, not the drawn fog: lifting the fog to look changes
    /// nothing heard.
    ///
    /// ```
    /// use regent_core::view::sound::Seen;
    ///
    /// assert_eq!(Seen::of(25), Seen::Yes);
    /// assert_eq!(Seen::of(0), Seen::No);
    /// ```
    #[must_use]
    pub const fn of(word: i32) -> Seen {
        if word > 0 { Seen::Yes } else { Seen::No }
    }
}

/// The distance a sound is mixed at, or `None` where it is not played:
/// an unseen unit's only at [`GLOBAL`]; [`OWNER_GLOBAL`]
/// and [`OWNER_ONLY`] become [`GLOBAL`] for the owner, the latter silent
/// for anyone else.
///
/// ```
/// use regent_core::view::sound::{GLOBAL, OWNER_ONLY, Owner, Seen, heard};
///
/// assert_eq!(heard(OWNER_ONLY, Owner::Local, Seen::Yes), Some(GLOBAL));
/// assert_eq!(heard(OWNER_ONLY, Owner::Other, Seen::Yes), None);
/// assert_eq!(heard(0.0, Owner::Local, Seen::No), None);
/// assert_eq!(heard(GLOBAL, Owner::Other, Seen::No), Some(GLOBAL));
/// ```
#[must_use]
#[allow(clippy::float_cmp, reason = "the engine compares exactly")]
pub fn heard(distance: f32, owner: Owner, seen: Seen) -> Option<f32> {
    let unseen_ok = seen == Seen::Yes || distance == GLOBAL;
    let owners = distance == OWNER_GLOBAL || distance == OWNER_ONLY;

    match (unseen_ok, owners, owner) {
        (false, ..) => None,
        (true, true, Owner::Local) => Some(GLOBAL),
        (true, true, Owner::Other) if distance == OWNER_ONLY => None,
        _ => Some(distance),
    }
}

/// Whether a unit's sound, played, makes the unit the one Backspace
/// centres on (map keys case 8): the local player's,
/// at `distance` [`GLOBAL`] or [`OWNER_GLOBAL`] (`GenUnit`,
/// gpl play sound). **Inferred**: the
/// sound manager's they test is the played phase's own distance.
///
/// ```
/// use regent_core::view::sound::{GLOBAL, OWNER_ONLY, Owner, speaks};
///
/// assert!(speaks(GLOBAL, Owner::Local));
/// assert!(!speaks(GLOBAL, Owner::Other));
/// assert!(!speaks(OWNER_ONLY, Owner::Local));
/// ```
#[must_use]
#[allow(clippy::float_cmp, reason = "the engine compares exactly")]
pub fn speaks(distance: f32, owner: Owner) -> bool {
    owner == Owner::Local && (distance == GLOBAL || distance == OWNER_GLOBAL)
}

/// A positioned sound's level: volume 0 to [`FULL`], pan in hundredths of
/// a dB, negative quieting the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mix {
    pub volume: i32,
    pub pan: i32,
}

impl Mix {
    /// Unplaced: full volume, centred.
    pub const CENTRED: Mix = Mix {
        volume: FULL,
        pan: 0,
    };

    /// Out of hearing.
    pub const SILENT: Mix = Mix { volume: 0, pan: 0 };

    /// Left and right amplitudes, 0 to 1: volume `v` is `v - 10,000`
    /// hundredths of a dB on both, pan quiets one side.
    #[must_use]
    pub fn gains(self) -> (f32, f32) {
        let level = |mb: i32| {
            #[allow(clippy::cast_precision_loss, reason = "within ±20,000")]
            let db = mb as f32 / 2000.0;

            10f32.powf(db)
        };
        let base = if self.volume <= 0 {
            0.0
        } else {
            level(self.volume - FULL)
        };

        (
            base * level(-self.pan.max(0)),
            base * level(self.pan.min(0)),
        )
    }
}

/// Sound mix: a sound at `at` heard from `listener`, cells, within
/// `radius`. The squared distance is divided by `distance`, the
/// `DistanceModifier`, unless 0 or 1; [`GLOBAL`] zeroes it. Past the
/// radius, silent. Else, at `r` = distance over radius, the volume loses
/// `3300 r`, truncated, and the pan is `±667 r` times the angle off `+x`
/// over 90°, folded past 90°: negative when the sound is south (`+y`).
/// The angle is off `facing`, [`Facing::EAST`] in the engine.
///
/// ```
/// use regent_core::view::sound::{Facing, Mix, mix};
///
/// let here = [10.0, 10.0, 0.0];
/// let east = Facing::EAST;
///
/// assert_eq!(mix(here, here, 30.0, 0.0, east), Mix { volume: 10_000, pan: 0 });
/// assert_eq!(mix(here, [25.0, 10.0, 0.0], 30.0, 0.0, east).volume, 8350);
/// assert_eq!(mix(here, [10.0, 25.0, 0.0], 30.0, 0.0, east).pan, -333);
/// assert_eq!(mix(here, [10.0, 41.0, 0.0], 30.0, 0.0, east), Mix::SILENT);
/// assert_eq!(mix(here, [10.0, 41.0, 0.0], 30.0, 10_000.0, east), Mix::CENTRED);
/// ```
#[must_use]
#[allow(clippy::float_cmp, reason = "the engine compares exactly")]
pub fn mix(listener: [f32; 3], at: [f32; 3], radius: f32, distance: f32, facing: Facing) -> Mix {
    let d = [
        at[0] - listener[0],
        at[1] - listener[1],
        at[2] - listener[2],
    ];
    let squared = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let squared = if distance == GLOBAL {
        0.0
    } else if distance != 0.0 && distance != 1.0 {
        squared / distance
    } else {
        squared
    };

    if radius * radius < squared {
        Mix::SILENT
    } else {
        let far = if squared > EPSILON {
            squared.sqrt()
        } else {
            0.0
        };
        let r = far / radius;
        let [x, y] = facing.turned(d[0], d[1]);
        let flat = x.hypot(y);
        let pan = if flat > 0.0 {
            pan_of(angle(x / flat, y / flat)) * r
        } else {
            0.0
        };
        let lost = r.clamp(0.0, 1.0) * FALLOFF;

        #[allow(clippy::cast_possible_truncation, reason = "the engine's ftol")]
        Mix {
            volume: FULL - lost as i32,
            pan: pan as i32,
        }
    }
}

/// Which way the listener faces, a unit vector in cells (`+y` south). A
/// sound 90° clockwise of it on the map is heard on the left: facing
/// `+x`, a sound south.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Facing([f32; 2]);

impl Facing {
    /// The engine's, `+x`: sounds pan north–south.
    pub const EAST: Facing = Facing([1.0, 0.0]);

    /// `+y`: a sound west of the centre, on the screen's left, is heard on
    /// the left. Cell `x` runs right on screen, `y` down.
    pub const SOUTH: Facing = Facing([0.0, 1.0]);

    /// An offset in cells as seen facing this way: along it, and 90°
    /// clockwise of it. [`Facing::EAST`] leaves it as it is.
    fn turned(self, x: f32, y: f32) -> [f32; 2] {
        let [fx, fy] = self.0;

        [x * fx + y * fy, y * fx - x * fy]
    }
}

/// Angle signed against the facing `+x`: degrees, negative where `y`
/// is not south.
fn angle(x: f32, y: f32) -> f32 {
    let deg = x.clamp(-1.0, 1.0).acos().to_degrees();

    if y <= 0.0 { -deg } else { deg }
}

/// The pan at the edge for an angle, sound mix–sound mix.
fn pan_of(deg: f32) -> f32 {
    let quarter = 90.0;
    let half = 180.0;

    if deg.abs() <= EPSILON {
        0.0
    } else if deg < 0.0 {
        let off = if deg >= -quarter { -deg } else { deg + half };

        off / quarter * PAN_EDGE
    } else {
        let off = if deg <= quarter { deg } else { half - deg };

        off / quarter * -PAN_EDGE
    }
}

/// The `DSDP` id of the phase a unit loops while it shows `set`
/// (`GenUnit`, run as any set starts and as its
/// sight changes): `Special-n` `Ambient_Idle`, `Build-n` `Ambient_Build`,
/// `Die-n` `Ambient_Die`, `Active-n` `Ambient_Active`, `Inactive-n`
/// `Ambient_Inactive`, each `AM` and a letter and the stage from 0; any
/// other set stops it. Only while the local player sees the unit
/// ([`Seen`], `GenUnit`).
///
/// ```
/// use regent_core::data::fourcc::FourCc;
/// use regent_core::view::sound::ambient;
///
/// assert_eq!(ambient(0x61), Some(FourCc(*b"AMD1"))); // Die-2
/// assert_eq!(ambient(0xc0), Some(FourCc(*b"AMA0"))); // Active
/// assert_eq!(ambient(0x08), None); // Stand
/// ```
#[must_use]
pub fn ambient(set: u32) -> Option<FourCc> {
    let stage = |base: u32| set.checked_sub(base).filter(|n| *n < AMBIENT_STAGES);
    let id = |letter: u8, n: u32| {
        let digit = b'0' + u8::try_from(n).unwrap_or(0);

        FourCc([b'A', b'M', letter, digit])
    };

    AMBIENT
        .iter()
        .find_map(|(base, letter)| stage(*base).map(|n| id(*letter, n)))
}

/// Stages per ambient set run: ten (a compare with 9).
const AMBIENT_STAGES: u32 = 10;

/// Set runs and their ambient letters, in `GenUnit`'s order.
const AMBIENT: [(u32, u8); 5] = [
    (0x50, b'B'),
    (0xc0, b'A'),
    (0xd0, b'N'),
    (0x40, b'I'),
    (0x60, b'D'),
];

/// A voice phase: its id opens `GV` (`VFX_*`, `Taunt`). A unit says one at
/// a time, on `DirectSound`'s voice
/// category, 6, not 2.
#[must_use]
pub fn is_voice(phase: FourCc) -> bool {
    phase.0.starts_with(b"GV")
}

/// Phase `ESL0`, `Select`.
pub const SELECT: FourCc = FourCc(*b"ESL0");

/// Phase `EG01`: what the `"` key plays,
/// the selection's own, at it, as its owner's, no attribute asked.
pub const VOICE: FourCc = FourCc(*b"EG01");

/// What the `"` key plays for the selection ([`VOICE`]).
#[must_use]
pub const fn voice() -> (Voice, sound::Phase) {
    (Voice::Own, sound::Phase::Id(VOICE))
}

/// `BP43 Construction`: a building site's selection sound.
pub const CONSTRUCTION: FourCc = FourCc(*b"BP43");

/// What a unit's class says about its selection sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selected {
    /// A building not yet built (`APB:1 FirstStageBuilt` 0).
    Site { magical: bool },
    /// Anything else: a built building, a hero, a monster.
    Unit { inhibited: bool },
}

impl Selected {
    /// A unit of `kind` whose attributes read `attribute`, 0 where unset:
    /// a building whose first stage is not built is a site. **Inferred**:
    /// Unit class of 3 is the descriptor's kind.
    pub fn of(kind: Option<UnitKind>, attribute: impl Fn(attrib::Key) -> i32) -> Selected {
        if kind == Some(UnitKind::Building) && attribute(attrib::FIRST_STAGE_BUILT) == 0 {
            Selected::Site {
                magical: attribute(attrib::MAGICAL_BIRTH) != 0,
            }
        } else {
            Selected::Unit {
                inhibited: attribute(attrib::INHIBIT_SELECTION_SOUND) != 0,
            }
        }
    }
}

/// The sound a unit makes as the local player selects it, if it was not
/// selected: a site `Construction`'s `Begin` unless born
/// by magic (`APB:54`); else its own `Select` unless `APU:14
/// InhibitSelectionSound`. Placed at the unit, as its owner's.
#[must_use]
pub const fn selection(selected: Selected) -> Option<(Voice, sound::Phase)> {
    match selected {
        Selected::Site { magical: false } => Some((
            Voice::Sound(CONSTRUCTION),
            sound::Phase::Id(crate::sim::world::sound::BEGIN),
        )),
        Selected::Unit { inhibited: false } => Some((Voice::Own, sound::Phase::Id(SELECT))),
        _ => None,
    }
}

/// `ER01 Error_Alert`: a placing click the preview refuses.
pub const ERROR_ALERT: FourCc = FourCc(*b"ER01");

/// The sound of a placing click the preview refuses, a building's or a
/// camp move's (build menu place;
/// camp move place): `Error_Alert`'s `Begin`,
/// unplaced, the local player's. The verdict's
/// chat line goes with it. A building that fits sounds at its decode
/// ([`crate::sim::world::sound::PLACE_BUILDING`]). Of the order panels only
/// the rogues' guild control 8009 plays one more, `EN01 Extortion`:
/// no other order panel refers to the
/// sound manager (the message scroll's CLOSE does,
/// [`crate::view::panel::message::closed`]), so picking a building, recruiting, researching, casting
/// and upgrading sound nothing at the click (**Inferred**: nor through a
/// unit's sound).
#[must_use]
pub const fn refused() -> (Voice, sound::Phase) {
    (
        Voice::Sound(ERROR_ALERT),
        sound::Phase::Id(crate::sim::world::sound::BEGIN),
    )
}

/// The rogues' guild's extortion press, before the amount is asked:
/// `EN01 Extortion`'s `Begin`, unplaced, the local player's.
#[must_use]
pub const fn extorted() -> (Voice, sound::Phase) {
    (
        Voice::Sound(crate::sim::world::guild::extortion::EXTORTION_SOUND),
        sound::Phase::Id(crate::sim::world::sound::BEGIN),
    )
}

/// One phase, ready to play.
#[derive(Clone, Debug, PartialEq)]
pub struct Play {
    /// The `DSND` record.
    pub sound: FourCc,
    pub phase: FourCc,
    /// A `WAVE` id.
    pub wave: FourCc,
    pub looped: bool,
    /// Held back by its group, it waits rather than drops: `CanQueue`, or
    /// its group's flag bit 0.
    pub queues: bool,
    /// `DistanceModifier`: the cue's, else the phase's; 0 none.
    pub distance: f32,
    pub group: Option<Group>,
    /// Random spreads, up and down: volume, hundredths of a dB; rate, Hz.
    pub volume_spread: (u32, u32),
    pub rate_spread: (u32, u32),
    /// Set values (`desc::Phase::set_volume` and kin).
    pub volume: Option<u32>,
    pub rate: Option<u32>,
    pub pan: Option<i32>,
}

/// Whether a sound has a place: a unit's, or none (`CYSndBuffer`,
/// a null position clearing flag 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placing {
    Placed,
    Unplaced,
}

impl Play {
    /// The volume shift a play keeps, hundredths of a dB, `spread` its
    /// drawn volume spread. `GE_Sound` sets the set volume, then adds
    /// the spread; placing the buffer remixes it (`CYSndBuffer` calls
    /// `CYDSSndBuffer`) and so does playing it, each writing
    /// [`mix`]'s volume over both. So only an unplaced sound keeps them.
    ///
    /// ```
    /// use regent_core::data::fourcc::FourCc;
    /// use regent_core::view::sound::{Placing, Play};
    ///
    /// let play = Play {
    ///     sound: FourCc(*b"PB01"),
    ///     phase: FourCc(*b"EBE0"),
    ///     wave: FourCc(*b"PB01"),
    ///     looped: false,
    ///     queues: false,
    ///     distance: 10_002.0,
    ///     group: None,
    ///     volume_spread: (5000, 0),
    ///     rate_spread: (0, 0),
    ///     volume: Some(5000),
    ///     rate: None,
    ///     pan: None,
    /// };
    ///
    /// assert_eq!(play.kept(Placing::Placed, 2500), 0);
    /// assert_eq!(play.kept(Placing::Unplaced, 2500), -2500);
    /// ```
    #[must_use]
    pub fn kept(&self, placing: Placing, spread: i32) -> i32 {
        match placing {
            Placing::Placed => 0,
            Placing::Unplaced => spread.wrapping_add(
                self.volume
                    .map_or(0, |v| v.cast_signed().wrapping_sub(FULL)),
            ),
        }
    }
}

/// A phase's `DSDG` group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: FourCc,
    pub limit: u32,
    /// Real ms.
    pub interval: u32,
    /// `SoundGroup::flags` bit 0.
    pub queues: bool,
}

/// The sound records of a catalogue, by folded name; each lookup takes the
/// catalogue it was made from.
#[derive(Clone, Debug, Default)]
pub struct Sounds {
    by_name: HashMap<String, FourCc>,
    phases: HashMap<String, FourCc>,
    groups: HashMap<FourCc, Group>,
    units: HashMap<String, FourCc>,
}

impl Sounds {
    /// Indexes `catalogue`'s `DSND`, `DSDP`, `DSDG` and `DUNT` names.
    ///
    /// # Errors
    ///
    /// If a record will not parse or lacks a `HEAD`.
    pub fn of(catalogue: &Catalogue<'_>) -> Result<Sounds> {
        let names = |tag: &'static [u8; 4]| -> Result<HashMap<String, FourCc>> {
            catalogue
                .tagged(tag)
                .map(|(_, r)| {
                    let part = desc::Part::parse(r.data)?;
                    let head = part.find(b"HEAD").ok_or(Error::Missing {
                        tag: FourCc::new(b"HEAD"),
                    })?;
                    let head = desc::Head::parse(head.body)?;

                    Ok((fold(head.name), head.id))
                })
                .collect()
        };
        let groups = catalogue
            .tagged(b"DSDG")
            .map(|(_, r)| {
                let g = SoundGroup::parse(&desc::Part::parse(r.data)?)?;
                let id = g.head.id;

                Ok((
                    id,
                    Group {
                        id,
                        limit: g.limit,
                        interval: g.interval,
                        queues: g.flags & 1 != 0,
                    },
                ))
            })
            .collect::<Result<_>>()?;

        Ok(Sounds {
            by_name: names(b"DSND")?,
            phases: names(b"DSDP")?,
            groups,
            units: names(b"DUNT")?,
        })
    }

    /// A `DUNT` record's `DefaultSound`.
    #[must_use]
    pub fn default_sound(catalogue: &Catalogue<'_>, unit: FourCc) -> Option<FourCc> {
        let unit = Unit::find(catalogue, unit)?.ok()?;

        unit.desc.tail().default_sound
    }

    /// The `DSND` id a cue's voice names; `own` is the source unit's
    /// descriptor.
    #[must_use]
    pub fn sound_of(
        &self,
        catalogue: &Catalogue<'_>,
        voice: &Voice,
        own: Option<FourCc>,
    ) -> Option<FourCc> {
        let default = |u| Sounds::default_sound(catalogue, u);

        match voice {
            Voice::Own => own.and_then(default),
            Voice::Unit(name) => self
                .units
                .get(&name.to_ascii_lowercase())
                .and_then(|u| default(*u)),
            Voice::Sound(id) => Some(*id),
            Voice::Named(name) => self.by_name.get(&name.to_ascii_lowercase()).copied(),
        }
    }

    /// The `DSDP` id a cue's phase names; [`sound::Phase::First`] names
    /// none.
    #[must_use]
    pub fn phase_of(&self, phase: &sound::Phase) -> Option<FourCc> {
        match phase {
            sound::Phase::Id(id) => Some(*id),
            sound::Phase::Named(name) => self.phases.get(&name.to_ascii_lowercase()).copied(),
            sound::Phase::First => None,
        }
    }

    /// What `voice` in `phase` plays, `distance` the cue's; `None` where a
    /// record or the phase is missing, which plays nothing
    /// (`GE_SoundManager`: no buffer).
    #[must_use]
    pub fn play(
        &self,
        catalogue: &Catalogue<'_>,
        voice: &Voice,
        phase: &sound::Phase,
        own: Option<FourCc>,
        distance: f32,
    ) -> Option<Play> {
        let sound = self.sound_of(catalogue, voice, own)?;
        let at = catalogue.find(b"DSND", sound)?;
        let record = catalogue.record(b"DSND", at)?;
        let part = desc::Part::parse(record.data).ok()?;
        let parsed = desc::Sound::parse(&part).ok()?;
        let found: PhaseRecord<'_> = match phase {
            sound::Phase::First => parsed.phases().next(),
            named => {
                let id = self.phase_of(named)?;

                parsed.phases().find(|p| p.id() == Some(id))
            }
        }?;
        let phase = found.id()?;
        let distance = if distance == 0.0 {
            found.distance().unwrap_or(0.0)
        } else {
            distance
        };

        let group = found.group().and_then(|g| self.groups.get(&g).copied());

        Some(Play {
            sound,
            phase,
            wave: found.wave()?,
            looped: found.has(PhaseFlag::Looped),
            queues: found.has(PhaseFlag::CanQueue) || group.is_some_and(|g| g.queues),
            distance,
            group,
            volume_spread: found.volume().unwrap_or((0, 0)),
            rate_spread: found.frequency().unwrap_or((0, 0)),
            volume: found.set_volume(),
            rate: found.set_frequency(),
            pan: found.set_pan(),
        })
    }
}

fn fold(name: &[u8]) -> String {
    String::from_utf8_lossy(name).to_ascii_lowercase()
}

/// Effects playing at once at most. **Inferred**: the sound manager's
/// constructor gives each category 16, a use
/// not read.
pub const MOST_EFFECTS: usize = 16;

/// Whether a sound is a unit's ambient loop or a cue: only cues count
/// against [`MOST_EFFECTS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Ambient,
    Cue,
}

/// What [`Gate::admit`] does with a sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admit {
    Start,
    /// Its group holds it back and it queues: ask again next frame.
    Wait,
    Drop,
}

/// What plays, as the start of a sound weighs it:
/// each group's count, the speakers, the
/// cues.
#[derive(Clone, Debug, Default)]
pub struct Gate {
    counts: HashMap<FourCc, u32>,
    speaking: HashSet<u32>,
    effects: usize,
}

impl Gate {
    /// A sound already playing: its group, a voice's speaker.
    pub fn playing(&mut self, group: Option<FourCc>, speaker: Option<u32>, source: Origin) {
        if let Some(g) = group {
            *self.counts.entry(g).or_default() += 1;
        }

        self.speaking.extend(speaker);
        self.effects += usize::from(source == Origin::Cue);
    }

    /// Whether `play` starts, its group last started `since` real ms ago
    /// (`None` never): its group must have waited its interval and be
    /// under its limit, else it waits if it queues or drops; one voice a
    /// speaker; at most [`MOST_EFFECTS`] cues. A start counts as playing.
    pub fn admit(
        &mut self,
        play: &Play,
        since: Option<u64>,
        speaker: Option<u32>,
        source: Origin,
    ) -> Admit {
        let group_free = play.group.is_none_or(|g| {
            let waited = since.is_none_or(|ms| ms >= u64::from(g.interval));

            waited && self.counts.get(&g.id).copied().unwrap_or(0) < g.limit
        });
        let silent = speaker.is_some_and(|s| self.speaking.contains(&s));
        let full = source == Origin::Cue && self.effects >= MOST_EFFECTS;

        match (group_free, play.queues) {
            (false, true) => Admit::Wait,
            (false, false) => Admit::Drop,
            (true, _) if silent || full => Admit::Drop,
            (true, _) => {
                self.playing(play.group.map(|g| g.id), speaker, source);
                Admit::Start
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Facing south, a sound left of the view's centre (west) is heard on
    /// the left, one right of it (east) on the right; one above (north) or
    /// below (south) stays centred.
    #[test]
    fn facing_south_pans_by_screen_side() {
        let here = [10.0, 10.0, 0.0];
        let pan = |at: [f32; 3]| mix(here, at, 30.0, 0.0, Facing::SOUTH).pan;
        let louder = |at: [f32; 3]| {
            let (l, r) = mix(here, at, 30.0, 0.0, Facing::SOUTH).gains();

            l - r
        };

        assert_eq!(pan([0.0, 10.0, 0.0]), -222);
        assert_eq!(pan([20.0, 10.0, 0.0]), 222);
        assert_eq!(pan([10.0, 0.0, 0.0]), 0);
        assert_eq!(pan([10.0, 20.0, 0.0]), 0);
        assert!(louder([0.0, 10.0, 0.0]) > 0.0);
        assert!(louder([20.0, 10.0, 0.0]) < 0.0);
    }

    /// Facing east, the engine's: north–south pans, east–west does not.
    #[test]
    fn facing_east_pans_north_south() {
        let here = [10.0, 10.0, 0.0];
        let pan = |at: [f32; 3]| mix(here, at, 30.0, 0.0, Facing::EAST).pan;

        assert_eq!(pan([0.0, 10.0, 0.0]), 0);
        assert_eq!(pan([20.0, 10.0, 0.0]), 0);
        assert_eq!(pan([10.0, 0.0, 0.0]), 222);
        assert_eq!(pan([10.0, 20.0, 0.0]), -222);
    }

    fn play(group: Option<Group>, queues: bool) -> Play {
        Play {
            sound: FourCc(*b"SND1"),
            phase: FourCc(*b"PHS1"),
            wave: FourCc(*b"WAV1"),
            looped: false,
            queues,
            distance: 0.0,
            group,
            volume_spread: (0, 0),
            rate_spread: (0, 0),
            volume: None,
            rate: None,
            pan: None,
        }
    }

    const GROUP: Group = Group {
        id: FourCc(*b"GRP1"),
        limit: 1,
        interval: 100,
        queues: false,
    };

    /// A full group holds a sound back: it waits if it queues, else
    /// drops; past the interval and under the limit it starts.
    #[test]
    fn a_group_holds_back_by_its_limit_and_interval() {
        let (grouped, queued) = (play(Some(GROUP), false), play(Some(GROUP), true));
        let mut gate = Gate::default();

        assert_eq!(gate.admit(&grouped, None, None, Origin::Cue), Admit::Start);
        assert_eq!(gate.admit(&queued, None, None, Origin::Cue), Admit::Wait);
        assert_eq!(gate.admit(&grouped, None, None, Origin::Cue), Admit::Drop);

        let mut fresh = Gate::default();

        assert_eq!(
            fresh.admit(&queued, Some(50), None, Origin::Cue),
            Admit::Wait
        );
        assert_eq!(
            fresh.admit(&queued, Some(100), None, Origin::Cue),
            Admit::Start
        );
    }

    /// One voice a speaker; sixteen cues, ambient loops besides.
    #[test]
    fn a_speaker_says_one_line_and_cues_stop_at_sixteen() {
        let free = play(None, false);
        let mut gate = Gate::default();

        assert_eq!(gate.admit(&free, None, Some(7), Origin::Cue), Admit::Start);
        assert_eq!(gate.admit(&free, None, Some(7), Origin::Cue), Admit::Drop);

        for _ in 1..MOST_EFFECTS {
            assert_eq!(gate.admit(&free, None, None, Origin::Cue), Admit::Start);
        }

        assert_eq!(gate.admit(&free, None, None, Origin::Cue), Admit::Drop);
        assert_eq!(gate.admit(&free, None, None, Origin::Ambient), Admit::Start);
    }
}
