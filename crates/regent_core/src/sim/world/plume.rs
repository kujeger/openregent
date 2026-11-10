//! Particle systems: units the engine makes as a unit's art plays a set
//! naming one. No script makes them, yet each takes a handle. **Binary**
//! unless marked.
//!
//! ```text
//!  a unit's set changes   UnitIMG (a set play, a
//!    │                    building's damage set, a unit's
//!    │                    first set)
//!    ├─ UnitIMG           each old part's set naming a system: every
//!    │                    system of that code on the unit stopped
//!    │                    (GenUnit -> effector end)
//!    └─ UnitIMG           each new part's: GenUnit (code, -1, 0x10)
//!                         at the part's offset, a unit of kind 1, the
//!                         next handle, no Wait, no ORDR, no world draw
//!  effector end           stopped: marked as such, emitting no
//!                         more (CY3D_ParticleSystem), off its
//!                         holder where it stands
//!  CY3D_ParticleSystem    its window: from its start and end ms; endless while
//!                         emitting forever, the emitter's lifespan plus
//!                         the particles' otherwise; a stop restarts it as
//!                         the particles' lifespan alone
//!  its holder deleted     GenUnit's relations: deleted with it
//! ```
//!
//! Smoke: a damaged building's `Die` stages name `Smoke_Large` (`XLSL`) or
//! `Smoke_Large_Heavy` (`XLSH`) on each fire layer, so every stage change
//! stops the old smokes and makes new ones; a ratapult ball's flight
//! names `Ratasmoke` (`XL23`), its impact (set `0x60`) stops it.
//!
//! **Corpus**: of 73 stopped systems in the saves, each was last updated
//! at most its window after its stop, four exactly at it; one past it is
//! queued for deletion ([`DELETING`]). Particles draw on their own
//! clock-seeded RNG, never the world's or GPL's, and only
//! for what is drawn: a front end steps them ([`crate::view::map::particles`])
//! from the pass each frame records ([`World::particle_sweep`]).

use crate::sim::world::piece_state;
use std::collections::BTreeMap;

use super::World;
use super::effectors::{INTRO_SET, NEARER};
use super::particles::{Emitting, Past, Spec, Sprite, StandingSystem, Sweep, Window};
use crate::base::fixed::{Fixed, div, from_whole};
use crate::data::fourcc::FourCc;
use crate::data::imag;
use crate::gpl::runtime;
use crate::sim::units::CAMERA_SIN;

/// Camera, `tile_pixels_fixed`: 32.0, art pixels across a cell.
pub const CAMERA_TILE: Fixed = from_whole(32);

/// `UnitIMG`'s first set: `Special`, else `Stand`,
/// else the art's first.
pub const FIRST_SETS: [u32; 2] = [
    crate::data::imag::SetKind::Special.id(),
    crate::data::imag::SetKind::Stand.id(),
];

/// A missile's impact set, `Die`: `0x60` in missile impact default.
pub const IMPACT_SET: u32 = crate::data::imag::SetKind::Die.id();

/// The layer flags giving a layer its parts, one each
/// (`UnitIMG`: bits 8 and 9).
pub(super) const PART_BITS: [u32; 2] = [
    imag::Pass::First.part_flag(),
    imag::Pass::Second.part_flag(),
];

/// What one part playing a set carries: a particle system's `DUNT` id,
/// and where it hangs about the unit, 16.16 cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vent {
    pub code: FourCc,
    pub offset: [Fixed; 2],
}

/// Each set's vents by whole id (its layer 0's), part by part.
pub type Vents = BTreeMap<u32, Vec<Vent>>;

/// A unit's vents from its art: each layer of each set naming a system
/// ([`imag::Image::particle_system`]), a vent per part, at the layer's
/// slot 0 offset; layer 0's parts hang at the unit itself, their
/// offset's base being none. **Inferred**:
/// slot 0, where a layer's slot follows its base's; every layer naming
/// a system in HD has one slot.
#[must_use]
pub fn vents(image: &imag::Image<'_>) -> Vents {
    let mut out = Vents::new();

    for set in &image.sets {
        let Some(code) = image.particle_system(set) else {
            continue;
        };
        let base = set.id & crate::data::imag::SET_BASE;
        let parts = PART_BITS
            .iter()
            .filter(|b| set.layer_flags() & **b != 0)
            .count();
        let offset = match (set.variant(), set.slots.first()) {
            (0, _) | (_, None) => [Fixed::ZERO; 2],
            (_, Some(s)) => [
                div(from_whole(i32::from(s.x)), CAMERA_TILE),
                div(from_whole(i32::from(s.y)), CAMERA_SIN),
            ],
        };

        out.entry(base)
            .or_default()
            .extend(std::iter::repeat_n(Vent { code, offset }, parts));
    }

    out
}

/// A particle system's lifespans from its descriptor, ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lifespan {
    /// The emitter's; `None` for `0xffffffff`, forever.
    pub emitter: Option<u32>,
    /// A particle's.
    pub particle: u32,
}

impl Lifespan {
    /// The window a new system's `CY3D_ParticleSystem` method `(emitter)`
    /// sets: `None` endless.
    #[must_use]
    pub fn window(self) -> Option<u32> {
        self.emitter.map(|e| e.wrapping_add(self.particle))
    }
}

/// Unit state bit 2: queued for deletion, drained
/// in the next frame (`GE_Level`, `GS_Level`).
pub const DELETING: u8 = 0x04;

/// What the world reads of a particle system's descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Particles {
    pub lifespan: Lifespan,
    /// Its unit's state bits ([`Plume::state`]): `DontBlock`.
    pub state: u8,
    /// `AttachmentPointID`, `PRIM` `+0x14`: where an effector of it
    /// hangs (`GenUnit`, descriptor).
    pub attach: u32,
    /// How its particles are born and move ([`crate::view::map::particles`]).
    pub spec: Spec,
    /// What they draw: frames of its art's first set.
    pub sprite: Sprite,
}

/// A particle-system unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plume {
    pub handle: u32,
    pub code: FourCc,
    /// Its holder's unit handle, 0 once stopped.
    pub holder: u32,
    pub owner: u32,
    /// When it was made, game ms: its system's first clock.
    pub made: u32,
    /// About its holder while held, else where it stands: 16.16 cells,
    /// x, y and height.
    pub at: [Fixed; 3],
    /// Unit state's low byte: [`crate::sim::world::piece_state::DONT_BLOCK`] from its
    /// descriptor, [`crate::sim::world::piece_state::RUINED`] once stopped, [`DELETING`] once its
    /// system is gone and it is stopped.
    pub state: u8,
    /// Its window's start, game ms.
    pub since: u32,
    /// Its window, ms; `None` endless.
    pub window: Option<u32>,
    /// Whether its system is gone, its window past: the unit draws
    /// nothing, and a save writes it no system (tail flag bit 0 clear).
    pub spent: bool,
    /// An effector's `Wait`: pushed at, and for ms; its end stops it.
    wait: Option<(u32, u32)>,
    lifespan: Lifespan,
    /// Its emitter's lifespan from [`Plume::since`], ms:
    /// `None` forever, 0 once a stop restarts its window.
    emits: Option<u32>,
}

impl Plume {
    /// Whether its window is past at game time `now`: the first update
    /// past it kills its particles.
    #[must_use]
    pub fn past(&self, now: u32) -> bool {
        self.window
            .is_some_and(|w| now.wrapping_sub(self.since) > w)
    }

    /// Whether it is stopped.
    #[must_use]
    pub const fn stopped(&self) -> bool {
        self.state & piece_state::RUINED != 0
    }

    /// Its emitter at game time `now`, and its window.
    fn window(&self, now: u32) -> Window {
        let emitting = self.emits.is_none_or(|e| now.wrapping_sub(self.since) < e);

        Window {
            emitting: if emitting {
                Emitting::Yes
            } else {
                Emitting::No
            },
            past: if self.past(now) {
                Past::Beyond
            } else {
                Past::Within
            },
        }
    }
}

/// A missile playing its impact set, `0x60`: its unit stands there until
/// the set ends, callback `0x1005` deleting it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spent {
    pub handle: u32,
    pub code: FourCc,
    /// Where it struck, 16.16 cells.
    pub at: [Fixed; 3],
    /// Its direction as it flew, kept: a save's record writes it.
    pub facing: i32,
    /// When it struck, game ms: its set plays from frame 0.
    pub since: u32,
    /// When its set ends, game ms.
    pub end: u32,
}

/// Whether building damage set plays its pick over a running
/// `InitAction`: `GSUnit`'s call forces it, `GSUnit`'s does
/// not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Force {
    Unforced,
    Forced,
}

/// Particle systems standing, missiles playing their impact set, and the
/// last frame's particle pass.
#[derive(Clone, Debug, Default)]
pub(super) struct Plumes {
    /// In the order made.
    list: Vec<Plume>,
    /// In impact order ([`Spent`]).
    spent: Vec<Spent>,
    /// The last frame's pass, for a front end; never saved.
    sweep: Option<Sweep>,
}

impl World {
    /// building damage set on building `id`:
    /// unforced, nothing while an `InitAction` holds track 2
    /// (`GenUnit` method `(2, 11)`), so its end picks again;
    /// else its set, played, so its systems change ([`World::vent`]) and
    /// its `Animate` with it ([`World::play_loop`]).
    pub(super) fn restyle(&mut self, id: u32, force: Force) {
        let held = self.falling(id).is_some() || self.track2(id);
        let set = self.building_set(id).map(u32::from);

        match (self.venting(id), set) {
            (Some(_), _) if held && force == Force::Unforced => {
                self.units.entry(id).skipped = true;
            }
            // building set play: nothing while an `Animate` shows it.
            (Some((handle, owner, vents)), Some(set)) if !self.shows(id, set) => {
                let old = self.units.entry(id).played.replace(set);

                self.vent(handle, owner, &vents, old, set);
                self.play_loop(id, set);
            }
            _ => {}
        }
    }

    /// After the frame's orders: each building whose write found track 2
    /// held, picked again once it is free (**Inferred**: `GSUnit`
    /// runs inside the orders, so a unit made after it there takes the
    /// handle before its systems).
    pub(super) fn restyle_skipped(&mut self) {
        let free: Vec<u32> = self
            .units
            .iter()
            .filter(|(_, u)| u.skipped)
            .map(|(id, _)| id)
            .filter(|id| self.falling(*id).is_none() && !self.track2(*id))
            .collect();

        for id in free {
            self.units.entry(id).skipped = false;
            self.restyle(id, Force::Forced);
        }
    }

    /// Particle systems standing, in the order they were made.
    #[must_use]
    pub fn plumes(&self) -> &[Plume] {
        &self.effects.plumes.list
    }

    /// Missiles playing their impact set, in impact order.
    #[must_use]
    pub fn spent(&self) -> &[Spent] {
        &self.effects.plumes.spent
    }

    /// The unit of `holder`'s set changed from `old` to `new`, its art
    /// `vents`: `UnitIMG` stops every system of an old part's code
    /// on it, then method makes one per new part, each the next handle.
    pub(super) fn vent(
        &mut self,
        holder: u32,
        owner: u32,
        vents: &Vents,
        old: Option<u32>,
        new: u32,
    ) {
        if old != Some(new) {
            self.replay_vents(holder, owner, vents, old, new);
        }
    }

    /// [`World::vent`] whether or not the set changed.
    fn replay_vents(&mut self, holder: u32, owner: u32, vents: &Vents, old: Option<u32>, new: u32) {
        let codes: Vec<FourCc> = old
            .and_then(|o| vents.get(&o))
            .map(|v| v.iter().map(|v| v.code).collect())
            .unwrap_or_default();

        for i in 0..self.effects.plumes.list.len() {
            if self.effects.plumes.list[i].holder == holder
                && codes.contains(&self.effects.plumes.list[i].code)
            {
                self.stop_plume(i);
            }
        }

        for v in vents.get(&new).into_iter().flatten() {
            self.emit(holder, owner, *v);
        }
    }

    /// `GenUnit` method `(code, -1, 0x10)`: a system on `holder`.
    fn emit(&mut self, holder: u32, owner: u32, vent: Vent) {
        let particles = self
            .descriptors
            .body(vent.code)
            .and_then(|b| b.particles.as_ref());

        if let Some(Particles {
            lifespan, state, ..
        }) = particles
        {
            let (lifespan, state) = (*lifespan, *state);

            self.handle += 1;
            self.effects.plumes.list.push(Plume {
                handle: self.handle,
                code: vent.code,
                holder,
                owner,
                made: self.now,
                at: [vent.offset[0], vent.offset[1], Fixed::ZERO],
                state,
                since: self.now,
                window: lifespan.window(),
                spent: false,
                wait: None,
                lifespan,
                emits: lifespan.emitter,
            });
        }
    }

    /// A particle system made as a unit of its own (`CreateSpellUnit`):
    /// the next handle, standing at `at`, held by nothing, no `Wait`.
    pub(super) fn stand_plume(&mut self, code: FourCc, owner: u32, at: [Fixed; 3]) {
        let before = self.handle;

        self.emit(
            0,
            owner,
            Vent {
                code,
                offset: [at[0], at[1]],
            },
        );

        if let Some(p) = self
            .effects
            .plumes
            .list
            .last_mut()
            .filter(|p| p.handle > before)
        {
            p.at[2] = at[2];
        }
    }

    /// `SetSpellUnitTimeout` on agent `id`'s particle system: its `Wait`
    /// of `ms`, whose end (spell unit timeout done, kind 5) stops
    /// it, unit state bit `0x10` and particle stop. Whether it is one.
    pub(super) fn time_plume(&mut self, id: u32, ms: u32) -> bool {
        let now = self.now;
        let handle = self.pieces.handle(id);
        let plume = self
            .effects
            .plumes
            .list
            .iter_mut()
            .find(|p| Some(p.handle) == handle && !p.stopped());

        plume.map(|p| p.wait = Some((now, ms))).is_some()
    }

    /// Effector end on plume `i`: bit `0x10`; particle stop
    /// restarts its window as its particles' lifespan unless its emitter
    /// is finite and still emitting; off its holder, where it stands.
    fn stop_plume(&mut self, i: usize) {
        let now = self.now;
        let holder = self.effects.plumes.list[i].holder;
        let base = self.holder_pos(holder);
        let p = &mut self.effects.plumes.list[i];
        let emitting = p
            .lifespan
            .emitter
            .is_some_and(|e| now.wrapping_sub(p.since) < e);

        if !emitting {
            p.since = now;
            p.window = Some(p.lifespan.particle);
            p.emits = Some(0);
        }

        p.state |= piece_state::RUINED;
        p.holder = 0;

        if let Some(b) = base {
            p.at = [b[0] + p.at[0], b[1] + p.at[1], b[2] + p.at[2]];
        }
    }

    /// Where the unit of handle `handle` stands in 3D: a piece's
    /// unit pos, a missile's flight point.
    fn holder_pos(&self, handle: u32) -> Option<[Fixed; 3]> {
        let piece = self
            .pieces
            .iter()
            .find(|(_, p)| p.handle == handle)
            .map(|(id, _)| *id);

        match piece {
            Some(id) => self.unit_pos3(id),
            None => self.effects.flights.at(handle).or_else(|| {
                self.effects
                    .plumes
                    .spent
                    .iter()
                    .find(|s| s.handle == handle)
                    .map(|s| s.at)
            }),
        }
    }

    /// A new unit of handle `handle` and descriptor `code`, made by
    /// piece `maker`'s owner: `UnitIMG` plays its first set.
    pub(super) fn first_vents(&mut self, handle: u32, maker: u32, code: FourCc) {
        let owner = self.piece_owner(maker);
        let body = self.descriptors.body(code);
        let first = body.and_then(|b| b.first);
        let vents = body.map(|b| b.vents.clone()).unwrap_or_default();

        if let Some(set) = first {
            self.vent(handle, owner, &vents, None, set);
        }
    }

    /// Effector `handle` named `name`, made on agent `holder`:
    /// its art's systems as any new unit's,
    /// then those of effector animate's set
    /// [`INTRO_SET`] where the art has it. Its end at the intro's last
    /// frame (callback `0x100c`, effector intro done: set `0x40`) is not
    /// followed: no effector's `0x40` names a system unless its first set
    /// is that. A `Wait` of 0 ms skips effector animate.
    pub(super) fn effector_vents(
        &mut self,
        handle: u32,
        holder: u32,
        name: &str,
        wait: Option<u32>,
    ) {
        let code = self.effector_code(name);

        if let Some(code) = code {
            self.first_vents(handle, holder, code);

            let (vents, first) = self.vents_of(code);

            if intro_plays(wait) && vents.contains_key(&INTRO_SET) {
                let owner = self.piece_owner(holder);

                self.vent(handle, owner, &vents, first, INTRO_SET);
            }
        }
    }

    /// Effector `handle` named `name`, on agent `holder`, as its `Wait`
    /// ends: set `0x60` from frame 0 where its art
    /// has it, a set change making that set's systems; else no set
    /// change. **Game**: play5's 0 ms `energy_blast_effector` at 277,800
    /// ms makes `XL08`, the handle agent 56 (285,500 ms) is short of
    /// without it.
    pub(super) fn effector_end_vents(
        &mut self,
        handle: u32,
        holder: u32,
        name: &str,
        wait: Option<u32>,
    ) {
        let code = self.effector_code(name);

        if let Some(code) = code {
            let (vents, first) = self.vents_of(code);
            let shown = if intro_plays(wait) && vents.contains_key(&INTRO_SET) {
                Some(INTRO_SET)
            } else {
                first
            };
            let owner = self.piece_owner(holder);

            if vents.contains_key(&IMPACT_SET) {
                self.vent(handle, owner, &vents, shown, IMPACT_SET);
            }
        }
    }

    /// The `DUNT` id of the effector named `name`: no particle system.
    fn effector_code(&self, name: &str) -> Option<FourCc> {
        self.descriptors
            .body_named(name)
            .filter(|(_, b)| b.particles.is_none())
            .map(|(c, _)| c)
    }

    /// Descriptor `code`'s vents and first set.
    fn vents_of(&self, code: FourCc) -> (Vents, Option<u32>) {
        let body = self.descriptors.body(code);

        (
            body.map(|b| b.vents.clone()).unwrap_or_default(),
            body.and_then(|b| b.first),
        )
    }

    /// A missile's impact:
    /// it plays set `0x60`, callback `0x1005` deleting it at the set's
    /// end; without the set, deleted at once, and its systems with it.
    pub(super) fn impact_vents(&mut self, f: &super::missile::Flight, at: [Fixed; 3]) {
        let (handle, code) = (f.handle, f.code());
        let body = self.descriptors.body(code);
        let vents = body.map(|b| b.vents.clone()).unwrap_or_default();
        let first = body.and_then(|b| b.first);
        let end = body
            .and_then(|b| b.poses.as_ref()?.get(&IMPACT_SET).map(|p| p.init_ms(0)))
            .map(|ms| self.now.wrapping_add(ms));

        if let Some(end) = end {
            let owner = self
                .effects
                .plumes
                .list
                .iter()
                .find(|p| p.holder == handle)
                .map_or(0, |p| p.owner);

            self.effects.plumes.spent.push(Spent {
                handle,
                code,
                at,
                facing: f.facing(),
                since: self.now,
                end,
            });
            self.vent(handle, owner, &vents, first, IMPACT_SET);
        }
    }

    /// Building `id`'s systems made again over its set: a damaged
    /// building's Action, refused a set, ends with its
    /// set played anew. **Game**: the dwarves game's damaged ballista
    /// tower and settlement, 164 shots, take 315 handles more than their
    /// missiles, each shot its stage's vents. **Inferred**: the stale
    /// local `GE_OrderProcessor` reads then is not followed; the old
    /// systems stop as at any set change.
    pub(super) fn revent(&mut self, id: u32) {
        let played = self.units.get(id).and_then(|u| u.played);

        if let (Some((handle, owner, vents)), Some(set)) = (self.venting(id), played) {
            self.replay_vents(handle, owner, &vents, Some(set), set);
        }
    }

    /// Building `id`'s `Become_Rubble` plays `Crumble` on track 2: its
    /// systems change as any set change's.
    pub(super) fn crumble_vents(&mut self, id: u32) {
        if let Some((handle, owner, vents)) = self.venting(id) {
            let set = u32::from(crate::sim::damage::CRUMBLE);
            let old = self.units.entry(id).played.replace(set);

            self.vent(handle, owner, &vents, old, set);
        }
    }

    /// Agent `id`'s piece as a holder of systems: its handle, owner and
    /// art's vents.
    fn venting(&self, id: u32) -> Option<(u32, u32, Vents)> {
        let piece = self.pieces.get(id)?;
        let vents = self.descriptors.body(piece.code)?.vents.clone();

        Some((piece.handle, piece.owner, vents))
    }

    /// The owner of agent `id`'s piece, 0 for none.
    fn piece_owner(&self, id: u32) -> u32 {
        self.pieces.owner(id).unwrap_or(0)
    }

    /// `CreateEffector` of a particle system on agent `id`:
    /// never re-armed; its unit hangs at the holder's
    /// `AttachmentPointID` (`GenUnit`: the point less the holder's
    /// place, y plus 1, `GenUnit`), with its `Wait` of `wait` ms, whose
    /// end stops it. **Game**: the united game's ice
    /// dragons' breaths hang `Icebreath_hit` on the palace for 0 ms, so
    /// each stands stopped at the breath's hit point.
    pub(super) fn hang_plume(&mut self, id: u32, name: &str, wait: Option<u32>) {
        let code = self
            .descriptors
            .body_named(name)
            .filter(|(_, b)| b.particles.is_some())
            .map(|(c, _)| c);
        let piece = self.pieces.get(id).map(|p| (p.handle, p.owner, p.code));

        if let (Some(code), Some((holder, owner, kind))) = (code, piece) {
            let attach = self
                .descriptors
                .body(code)
                .and_then(|b| b.particles.as_ref())
                .map_or(0, |p| p.attach);
            let facing = self.facing_word(id);
            let offset = self
                .descriptors
                .body(kind)
                .map_or([Fixed::ZERO; 3], |b| b.hit.offset3_at(attach, facing));

            self.emit(
                holder,
                owner,
                Vent {
                    code,
                    offset: [offset[0], offset[1] + NEARER],
                },
            );

            if let Some(p) = self.effects.plumes.list.last_mut() {
                p.at[2] = offset[2];
                p.wait = wait.map(|ms| (self.now, ms));
            }

            if wait.is_some() {
                self.dispatches.join(super::Listed::Effector(self.handle));
            }
        }
    }

    /// Each frame, after the orders and their deletes (`GE_Level`
    /// calls the particle manager's update, particle manager update, after method):
    /// a system past its window is freed; a stopped one's
    /// unit is then queued for deletion (particle system free, where
    /// `GenUnit` sees unit state bit `0x10`), gone the next
    /// frame, while a held one stands empty with its holder. A held one whose holder is
    /// gone goes with it. **Corpus**: no stopped
    /// system in the saves is older than its window but one, so queued:
    /// united's `XL31` #1037, unit state bit 2, no system.
    pub(super) fn plume_step(&mut self) {
        let now = self.now;
        // The handles that can hold a system, gathered only while one is
        // held: most frames none is.
        let holding = self.effects.plumes.list.iter().any(|p| p.holder != 0);
        let held: std::collections::BTreeSet<u32> = if holding {
            self.pieces
                .values()
                .map(|p| p.handle)
                .chain(self.effects.flights.handles())
                .chain(self.effects.plumes.spent.iter().map(|s| s.handle))
                .chain(
                    self.effectors
                        .iter()
                        .filter(|e| e.hangs(now))
                        .map(|e| e.handle),
                )
                .collect()
        } else {
            std::collections::BTreeSet::new()
        };

        let ended: Vec<usize> = (0..self.effects.plumes.list.len())
            .filter(|i| {
                self.effects.plumes.list[*i]
                    .wait
                    .is_some_and(|(when, ms)| now != when && runtime::elapsed(now, when) >= ms)
            })
            .collect();

        for i in ended {
            self.effects.plumes.list[i].wait = None;

            if !self.effects.plumes.list[i].stopped() {
                self.stop_plume(i);
            }
        }

        self.effects.plumes.spent.retain(|s| now < s.end);
        self.effects
            .plumes
            .list
            .retain(|p| p.state & DELETING == 0 && (p.holder == 0 || held.contains(&p.holder)));
        self.record_sweep();

        let mut gone = Vec::new();

        for p in self
            .effects
            .plumes
            .list
            .iter_mut()
            .filter(|p| !p.spent && p.past(now))
        {
            p.spent = true;

            if p.stopped() {
                p.state |= DELETING;
                gone.push(p.handle);
            }
        }

        // A spell unit's system: its piece goes too, `UnitDelete` with it.
        let pieces: Vec<u32> = self
            .pieces
            .iter()
            .filter(|(_, p)| gone.contains(&p.handle))
            .map(|(id, _)| *id)
            .collect();

        for id in pieces {
            self.dispatches.delete(id);
        }
    }
}

impl World {
    /// The particle manager's pass:
    /// each system not yet freed, its unit's place and its window, for a
    /// front end to step its particles by ([`crate::view::map::particles`]).
    fn record_sweep(&mut self) {
        let sources = self
            .effects
            .plumes
            .list
            .iter()
            .filter(|p| !p.spent)
            .map(|p| StandingSystem {
                handle: p.handle,
                code: p.code,
                made: p.made,
                unit: match p.holder {
                    0 => Some(p.at),
                    h => self
                        .holder_pos(h)
                        .map(|b| [b[0] + p.at[0], b[1] + p.at[1], b[2] + p.at[2]]),
                },
                window: p.window(self.now),
            })
            .collect();

        self.effects.plumes.sweep = Some(Sweep {
            at: self.now,
            sources,
        });
    }

    /// The last frame's particle pass; `None` before the first frame
    /// stepped here or since a load.
    #[must_use]
    pub const fn particle_sweep(&self) -> Option<&Sweep> {
        self.effects.plumes.sweep.as_ref()
    }
}

/// Whether `GenUnit` runs effector animate for an effector
/// whose `Wait` is `wait` ms (`None`: none pushed): all but 0 ms.
fn intro_plays(wait: Option<u32>) -> bool {
    wait != Some(0)
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Lifespan {
        type Model = m::Lifespan;

        fn save(&self) -> m::Lifespan {
            m::Lifespan {
                emitter: self.emitter,
                particle: self.particle,
            }
        }

        fn load(model: m::Lifespan) -> Result<Lifespan, Refused> {
            Ok(Lifespan {
                emitter: model.emitter,
                particle: model.particle,
            })
        }
    }

    impl Saved for Plume {
        type Model = m::Plume;

        fn save(&self) -> m::Plume {
            let Plume {
                handle,
                code,
                holder,
                owner,
                made,
                at,
                state,
                since,
                window,
                spent,
                wait,
                lifespan,
                emits,
            } = self;

            m::Plume {
                handle: *handle,
                code: code.save(),
                holder: *holder,
                owner: *owner,
                made: *made,
                at: at.save(),
                state: *state,
                since: *since,
                window: *window,
                spent: *spent,
                wait: *wait,
                lifespan: lifespan.save(),
                emits: *emits,
            }
        }

        fn load(model: m::Plume) -> Result<Plume, Refused> {
            Ok(Plume {
                handle: model.handle,
                code: FourCc::load(model.code)?,
                holder: model.holder,
                owner: model.owner,
                made: model.made,
                at: <[Fixed; 3]>::load(model.at)?,
                state: model.state,
                since: model.since,
                window: model.window,
                spent: model.spent,
                wait: model.wait,
                lifespan: Lifespan::load(model.lifespan)?,
                emits: model.emits,
            })
        }
    }

    impl Saved for Spent {
        type Model = m::Spent;

        fn save(&self) -> m::Spent {
            let Spent {
                handle,
                code,
                at,
                facing,
                since,
                end,
            } = *self;

            m::Spent {
                handle,
                code: code.save(),
                at: at.save(),
                facing,
                since,
                end,
            }
        }

        fn load(model: m::Spent) -> Result<Spent, Refused> {
            Ok(Spent {
                handle: model.handle,
                code: FourCc::load(model.code)?,
                at: <[Fixed; 3]>::load(model.at)?,
                facing: model.facing,
                since: model.since,
                end: model.end,
            })
        }
    }

    impl Saved for Plumes {
        type Model = m::Plumes;

        fn save(&self) -> m::Plumes {
            m::Plumes {
                list: self.list.save(),
                spent: self.spent.save(),
            }
        }

        fn load(model: m::Plumes) -> Result<Plumes, Refused> {
            Ok(Plumes {
                list: Vec::load(model.list)?,
                spent: Vec::load(model.spent)?,
                sweep: None,
            })
        }
    }
}
