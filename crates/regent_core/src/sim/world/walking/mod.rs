//! Walking in the world: each unit's order tracks, run every frame before
//! the scheduler, and the builtins that give and read them. **Binary**
//! unless marked; the walk itself is `sim::movement`.
//!
//! ```text
//!  World::step ── ORDR, in insertion order, per unit (GenUnit):
//!     track 0  the top order: MovePath, Turn, Move       movement::Walker
//!     track 1  every order:   FollowUnit, FollowPath, Action
//!  then a unit with no order leaves ORDR                  GE_Level
//!
//!  Move(unit)   StopMoving, FollowUnit run at once: not adjacent ─> MovePath
//!               beside it; re-checked every 500 ms; adjacent ─> path off, end
//!  Move(coord)  StopMoving, FollowPath run at once
//!  Hide         StopMoving, FollowPath to the building's exit point
//!  FollowPath   every frame from its push, track 0 empty: MovePath to the
//!               next waypoint; past the last it ends ─> callback 0x1003,
//!               the unit enters
//! ```

mod collide;
mod orders;
mod persist;
mod step;

use std::cell::OnceCell;

pub use collide::CHECK_GRID;

use super::loss_log::Why;
use super::{
    Answer, Found, Listed, World, agent_ref, int_ref, placing::Blockers, put_bool, string_ref,
};
use crate::base::fixed::Fixed;
use crate::data::attrib::Key;
use crate::data::fourcc::FourCc;
use crate::gpl::ops::{self, Value};
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::clearing;
use crate::sim::movement::action::{Peak, Pose, Sets, Settle};
use crate::sim::movement::plan::{
    Avoid, Blocking, Cell, Grid, LAYERS, NOWHERE, Occupant, Rank, Visit, aim, flood, plan,
    rect_cells,
};
use crate::sim::movement::{
    Around, ENTER_GAP2, FOLLOW_MS, MODES, PATH_HORIZON, TRAMPLES, Track1, Waiter, Walker,
    direction, wait_for,
};
use crate::sim::placement::{Edge, Heights, Plateau};
use crate::sim::space::{self, HalfRect};

/// A new unit's facing: north. The facing is its animation parts' slot,
/// `UnitIMGPartEntry`; the part constructor and
/// reset zero it, a set change keeps it,
/// and no step of `SpawnUnit`'s writes it: `GSUnit`
/// applies its drawn slot only to a `Directionless` descriptor.
/// **Game**: every spawned unit never walked is saved facing 0.
pub const FIRST_FACING: i32 = 0;

/// `ActionRateModifier`, `APV:50`: added to an action's rate.
const ACTION_RATE: Key = Key::new(*b"APV", 50);

/// `MovementRateModifier`, `APV:34`: added to the class's move rate.
/// **Game**: plague's +100 on its target.
const MOVE_RATE: Key = Key::new(*b"APV", 34);

/// `Walk`, the set a frameless class's path plays.
const WALK: u32 = crate::data::imag::SetKind::Walk.id();

/// The set a spent `MovePath` plays where the art has it, else `Stand`.
const PATH_END_SET: u32 = crate::data::imag::SetKind::WalkToStand.id();

/// `IsFrozen`, `APV:63`: a frozen unit is never nudged.
const FROZEN: Key = Key::new(*b"APV", 63);

/// The one descriptor no unit is hostile to, nor it to any.
const NEVER_HOSTILE: [u8; 4] = *b"AVh1";

/// What an Action order reads of its `DACT` record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Act {
    /// `Rate` max, ms: `ActionDesc_Std_Engine`.
    pub rate: u32,
    /// `ImageSet`,: the set its `InitAction` plays
    /// (`desc::Action::image_set`); 0 plays none.
    pub set: u32,
    /// `Projectile`,: action face then aims at a target
    /// unit's method, and the last frame launches it (`super::missile`).
    pub projectile: Option<super::missile::Missile>,
    /// The set played as its `InitAction` ends,
    /// (`desc::Action::after_set`): 0 none, -1 the action list's default.
    pub after: i32,
    /// `Overlay`,: hung on the actor for 0 ms at its peak
    /// (`desc::Action::overlay`).
    pub overlay: Option<FourCc>,
    /// `cProc`,: the callback its peak runs
    /// (init action peak, `desc::Script::c_proc`); 0 none.
    pub proc: u32,
    /// `Sound`, and `SoundPhase` begin and peak,
    /// (`desc::Action::sound_phases`): action sound's.
    pub sound: Option<FourCc>,
    pub begin: Option<FourCc>,
    pub peak: Option<FourCc>,
}

/// `Basic_death`'s and `Become_Rubble`'s `cProc`: `GS_OrderProcessor`'s
/// callback `0x2000`, which runs
/// unit lie down (unit lie down, callback `0x1004`).
pub const DYING_DONE: u32 = 0x2000;

/// The `Stand` set's id.
const STAND: u32 = crate::data::imag::SetKind::Stand.id();

/// The set unit lie down puts on a dying unit's parts where its art
/// has one: `Dead` (`0xe0`, unit lie down), a hero's gravestone.
pub const LAID_SET: u32 = crate::sim::damage::DEAD as u32;

/// The set `GSUnit` grants a damaged building: `Crumble`
/// (a compare with `0xf0`).
const CRUMBLE_SET: u32 = crate::sim::damage::CRUMBLE as u32;

/// An Action order's length, ms: `Rate` max plus `ActionRateModifier`,
/// rounded to whole frames from a frame up (`GSUnit`;
/// the rounding [`crate::sim::movement::rate`]'s).
///
/// ```
/// use regent_core::sim::world::walking::action_ms;
///
/// assert_eq!(action_ms(300, 0), 300); // Basic_build: 6 frames
/// assert_eq!(action_ms(1100, 30), 1150);
/// ```
#[must_use]
pub const fn action_ms(rate_max: u32, modifier: i32) -> u32 {
    crate::sim::movement::rate(rate_max, modifier)
}

/// `Move`'s info bit that keeps `FollowUnit` running when close
/// and loops `FollowPath`.
const KEEP: u32 = 0x2;

/// A `BBt` chest, which `GSUnit` method `(1)` does not call a building (the
/// code's first three bytes in `GSUnit`).
pub(super) fn is_chest(code: [u8; 4]) -> bool {
    code.starts_with(&clearing::CHEST)
}

/// `Move`'s info bit that appends a waypoint to a running `FollowPath`.
const ADD_WAYPOINT: u32 = 0x1;

/// The ground walkers stand on: the map's vertex heights, 16.16.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeightMap {
    across: u32,
    heights: Vec<Fixed>,
}

impl HeightMap {
    /// A save's ground, its `f32` heights exact 16.16 values; or a
    /// generated one.
    #[must_use]
    pub fn of(terrain: &dyn Heights) -> HeightMap {
        let across = terrain.across();
        let heights = (0..across)
            .flat_map(|y| (0..across).map(move |x| (x, y)))
            .map(|(x, y)| terrain.height(x, y).unwrap_or(Fixed::ZERO))
            .collect();

        HeightMap { across, heights }
    }

    /// Write a flatten's plateau.
    pub fn level(&mut self, plateau: &Plateau) {
        for (&(x, y), &h) in plateau {
            if let Some(v) = self.heights.get_mut((y * self.across + x) as usize) {
                *v = h;
            }
        }
    }
}

impl Heights for HeightMap {
    fn across(&self) -> u32 {
        self.across
    }

    fn height(&self, x: u32, y: u32) -> Option<Fixed> {
        (x < self.across && y < self.across)
            .then(|| self.heights.get((y * self.across + x) as usize).copied())
            .flatten()
    }
}

/// What a `MovePath` walks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Goal {
    /// The nearest free half cell beside this agent's unit;
    /// with the unit's place when the `MovePath`
    /// was pushed, unit pos32, 32nds of a cell: `MovePath`,
    /// the goal a nudged unit is sent to.
    Beside(u32, Option<(i32, i32)>),
    /// The nearest free half cell to a point, 32nds of a cell.
    Point((i32, i32)),
}

/// What an order does when it ends: its callback by id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Callback {
    None,
    /// `0x1003`: enter this agent's building.
    Enter(u32),
}

/// A track 1 order.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Intent {
    /// `OrderFollowUnit`: to beside `target`, or within `max` half cells.
    FollowUnit { target: u32, max: i32, info: u32 },
    /// `OrderFollowPath`: waypoints in 32nds, the next to walk to.
    /// `range` is a field, the `Move`'s distance over 16: each
    /// `MovePath`'s range.
    FollowPath {
        waypoints: Vec<(i32, i32)>,
        next: usize,
        info: u32,
        range: i32,
        callback: Callback,
    },
    /// `OrderAction`, `PerformAction`'s: waits for tracks 0 and 2, then
    /// turns to `aim` and runs `ms` more ([`World::act`]); track 2 plays
    /// `pose`, then settles the facing. `ms` is `None` where the action's
    /// record is not given: it never ends. `pose` is `None` where the
    /// actor's art is not given.
    Action {
        aim: Aim,
        ms: Option<u32>,
        pose: Option<(Pose, Settle, Sets)>,
        armed: bool,
        missile: Option<super::missile::Missile>,
    },
    /// `OrderFade`, `FadeIn`'s and `FadeOut`'s ([`World::fade_run`]):
    /// the draw effects' translucency a step toward `target` each `rate`
    /// ms.
    Fade { target: u8, rate: u32 },
}

/// What an action turns to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Aim {
    /// A unit, by agent; the order ends when it is gone.
    Unit(u32),
    /// A point, 32nds of a cell.
    Point((i32, i32)),
    /// The actor itself: action turn turns nothing.
    None,
}

/// A timed track 1 order: runs when `now - when >= delta`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TrackOrder {
    intent: Intent,
    when: u32,
    delta: u32,
}

/// `CombatSignal`'s GS order `0x4001` on track 1 (combat flash,
/// `GS_OrderProcessor`, run by `GSOrderCombatSignal`): its count
/// from 0 toward [`Flash::RUNS`], one a run, a run every [`Flash::MS`], run on
/// push; the run finding the count there ends it. A hit while it lasts sets
/// the count back to 0. Only `CombatSignal` makes or ends it: no stop takes
/// it off, so a unit hit stays in ORDR some 4 s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flash {
    count: u32,
    when: u32,
}

impl Flash {
    /// The count's end: `0x14` in combat flash, the order's.
    pub const RUNS: u32 = 20;
    /// Game ms between runs: `0xc8` in combat flash, the order's rate.
    pub const MS: u32 = 200;

    /// Pushed at `now`, run on push (header bit 2).
    #[must_use]
    pub const fn pushed(now: u32) -> Flash {
        Flash {
            count: 1,
            when: now,
        }
    }

    /// A hit while it lasts: the count to 0, its time kept.
    pub const fn hit(&mut self) {
        self.count = 0;
    }

    /// Run in the unit's turn at `now`, if due: whether it lasts.
    pub const fn run(&mut self, now: u32) -> bool {
        if now.wrapping_sub(self.when) < Self::MS {
            true
        } else if self.count == Self::RUNS {
            false
        } else {
            self.count += 1;
            self.when = now;
            true
        }
    }
}

/// Whether an order runs on after a run.
enum Run {
    On,
    End(Callback),
}

/// A unit's order tracks, as far as the world runs them.
#[derive(Clone, Debug)]
pub struct Tracks {
    walker: Walker,
    goal: Goal,
    /// The `MovePath`'s avoid modes and radius, set with its goal.
    avoid: Avoid,
    /// Track 1, newest first (`OrderStack` pushes at the front).
    intents: Vec<TrackOrder>,
    /// What track 2's `InitAction` launches at its last frame.
    shot: Option<super::missile::Shot>,
    /// The callback track 2's `InitAction` runs at its peak: [`Act::proc`].
    proc: u32,
    /// The function its peak dispatches, and what the action aims at.
    script: Option<(super::missile::Script, Aim)>,
    /// Track 1's Action's `cProc` and function, which track 2 takes when
    /// it arms: a later `PerformAction` leaves a playing `InitAction`'s own.
    pending: Option<(u32, Option<(super::missile::Script, Aim)>)>,
    /// The overlay track 2's `InitAction` hangs on the unit at its peak
    /// ([`Act::overlay`]), and track 1's Action's, taken as it arms.
    overlay: Option<FourCc>,
    pending_overlay: Option<FourCc>,
    /// The action whose sounds the `InitAction` plays as it is pushed and
    /// at its peak ([`Act::begin`], [`Act::peak`]), taken as it arms.
    sounding: Option<Act>,
    pending_sounding: Option<Act>,
    /// Whether an `InitAction` holding the GPL agent ended since the last
    /// `PerformAction`: `OrderEntry` clears the agent's byte `0x40`,
    /// though an Action pushed meanwhile took the agent too.
    released: bool,
    /// A looping `Animate` on track 1 ([`Track1::Plays`]): an order, so
    /// the unit stays in ORDR until a stop or an `InitAction`'s end takes
    /// it off.
    animate: bool,
    /// `CombatSignal`'s order on track 1 ([`Flash`]).
    flash: Option<Flash>,
}

impl Tracks {
    /// The walker's facing.
    pub(super) const fn facing(&self) -> i32 {
        self.walker.facing
    }

    /// The walker's own 3D point.
    pub(super) const fn point(&self) -> [Fixed; 3] {
        self.walker.at
    }

    /// A flier's own height ([`Walker::aloft`]).
    pub(super) const fn aloft(&self) -> Option<Fixed> {
        self.walker.aloft()
    }

    /// Game ms a `Walk` frame shows: [`Walker::stride_ms`].
    pub(super) const fn stride(&self) -> Option<u32> {
        self.walker.stride_ms()
    }

    /// The `Walk` frame at game time `now`: [`Walker::walk_frame`].
    pub(super) const fn walk_frame(&self, now: u32) -> Option<u32> {
        self.walker.walk_frame(now)
    }

    /// What track 2 shows: [`Walker::look`].
    pub(super) fn look(&self) -> Option<crate::sim::movement::action::Look> {
        self.walker.look()
    }

    /// Whether an order holds the unit: [`World::holds`]'s test.
    pub(super) fn held(&self) -> bool {
        (self.holding() && !self.released) || self.walker.holds()
    }

    /// Whether track 0 holds an order: [`Walker::moving`].
    pub(super) const fn moving(&self) -> bool {
        self.walker.moving()
    }

    /// Whether its walk's gait is `Stand`: waiting on a walker in its way.
    pub(super) fn waits(&self) -> bool {
        self.walker.gait() == crate::sim::movement::Gait::Stand
    }

    /// Whether any track holds an order: `GenUnit`.
    const fn busy(&self) -> bool {
        self.walker.moving()
            || self.walker.acting()
            || !self.intents.is_empty()
            || self.animate
            || self.flash.is_some()
    }

    fn acting(&self) -> bool {
        self.intents
            .iter()
            .any(|o| matches!(o.intent, Intent::Action { .. }))
    }

    /// Whether its Action holds the GPL agent for a known time: one with
    /// no record never ends here, which says nothing of the engine's.
    fn holding(&self) -> bool {
        self.intents.iter().any(|o| {
            matches!(
                o.intent,
                Intent::Action {
                    ms: Some(_),
                    pose: Some(_),
                    ..
                }
            )
        })
    }

    fn following(&self) -> bool {
        self.intents.iter().any(|o| {
            matches!(
                o.intent,
                Intent::FollowUnit { .. } | Intent::FollowPath { .. }
            )
        })
    }

    /// `GenUnit`: Animate and Action off track 1, then `FollowPath`
    /// and `FollowUnit`; taking either off takes the `MovePath` off track 0
    /// (their method in mode 1, path remove). A step or turn on top
    /// runs on, and so does track 2, taking an Action's GPL agent. The parts
    /// hold their frame whatever `Animate` played, an after-set's too
    /// (`GenUnit` takes every type 5).
    fn stop(&mut self, now: u32) {
        if self.following() {
            self.walker.arrive();
        }

        if self.acting() {
            self.walker.hand_over();
        }

        self.walker.hold_frame(now);

        // `GenUnit` takes types 5, 6, 7 and 10: not a fade.
        self.intents
            .retain(|o| matches!(o.intent, Intent::Fade { .. }));
        self.animate = false;
    }

    /// `GenUnit`: every track's orders gone, track 1 as
    /// [`Tracks::stop`] takes them.
    pub(super) fn clear(&mut self, now: u32) {
        self.stop(now);
        self.intents.clear();
        self.walker.clear();
        self.shot = None;
        self.proc = 0;
        self.script = None;
        self.pending = None;
    }

    /// Waypoint last: a `FollowPath`'s waypoint
    /// before its next, the first while none is taken; `None` with no
    /// `FollowPath` on track 1.
    fn heading(&self) -> Option<(i32, i32)> {
        self.intents.iter().find_map(|o| match &o.intent {
            Intent::FollowPath {
                waypoints, next, ..
            } => waypoints.get(next.saturating_sub(1)).copied(),
            _ => None,
        })
    }

    /// `FollowPath`, if on track 1.
    fn path(&self) -> Option<(&[(i32, i32)], Callback)> {
        self.intents.iter().find_map(|o| match &o.intent {
            Intent::FollowPath {
                waypoints,
                callback,
                ..
            } => Some((waypoints.as_slice(), *callback)),
            _ => None,
        })
    }
}

impl World {
    /// Why the world does not run piece `id`'s orders; `None` if it does:
    /// frames are stepped, the ground and its class known, and every piece
    /// placed.
    fn unwalked(&self, id: u32) -> Option<Why> {
        let gait = self.body_of(id).is_some_and(|b| b.gait.is_some());

        if self.frames != super::frame::Frames::Stepped {
            Some(Why::NotStepped)
        } else if self.board.ground().is_none() {
            Some(Why::NoGround)
        } else if !gait {
            Some(Why::NoGait)
        } else if self.known(id).is_none() {
            Some(Why::Unknown)
        } else {
            self.first_unplaced().map(Why::Unplaced)
        }
    }

    /// Piece `id`'s tracks, made standing where it is on first use.
    fn tracks_of(&mut self, id: u32) -> Option<&mut Tracks> {
        if self.units.tracks(id).is_none() {
            let at = self.position(id)?;
            let class = self.descriptors.body(self.any_piece(id)?.code)?.gait?;
            let mut walker = Walker::standing(self.board.ground()?, at, FIRST_FACING, &class);

            // The art's `Walk` and `Stand`, which its own orders play, and
            // the set it was made in, which the parts keep
            // where the art lacks one. **Inferred**: made in method's first
            // set, not one a caller asked for.
            let poses = self
                .body_of(id)
                .and_then(|b| Some((b.poses.as_ref()?, b.first?)));

            if let Some((poses, first)) = poses {
                walker.loops(poses.get(&WALK).cloned(), poses.get(&STAND).cloned());
                walker.made_in(poses.get(&first).cloned().unwrap_or_default());
            }

            // Its `PATH_END_SET`, which a spent path plays instead
            // where the art has it.
            let end = self
                .body_of(id)
                .and_then(|b| b.poses.as_ref()?.get(&PATH_END_SET).cloned());

            if let Some(end) = end {
                walker.ends(end);
            }

            self.units.entry(id).tracks = Some(Tracks {
                walker,
                goal: Goal::Point(at),
                avoid: Avoid::default(),
                intents: Vec::new(),
                shot: None,
                proc: 0,
                script: None,
                pending: None,
                overlay: None,
                pending_overlay: None,
                sounding: None,
                pending_sounding: None,
                released: false,
                animate: false,
                flash: None,
            });
        }

        self.units.tracks_mut(id)
    }

    /// `GSUnit` refusing set `set` to piece `id`: a
    /// building below its `MaxHP` (`APU:0` under `APU:1`) gets no set but
    /// [`CRUMBLE_SET`], so `GE_OrderProcessor` pushes no `InitAction`
    /// of it. **Game**: the dwarves game's damaged ballista tower launches
    /// its bolts at its `PerformAction`, not after its set's seven frames.
    /// method's stale local is then the action descriptor's pointer,
    /// so the `InitAction` steps its part's own set.
    /// **Inferred**: part 0, one frame, so it peaks as it runs.
    pub(super) fn damaged(&self, id: u32, set: u32) -> bool {
        let piece = self.any_piece(id);
        let building = piece
            .and_then(|p| self.descriptors.body(p.code))
            .is_some_and(|b| b.class == super::bodies::Class::Building);
        let attribute = |Key { bank, index }| piece.and_then(|p| p.get(Key::new(bank, index)));

        match (
            attribute(crate::data::attrib::HP),
            attribute(crate::data::attrib::MAX_HP),
        ) {
            (Some(hp), Some(max)) => building && set != CRUMBLE_SET && hp < max,
            _ => false,
        }
    }

    /// A building's tracks for its Action ([`Walker::fixed`]): made on its
    /// first `PerformAction`, facing as it is drawn. A building's other
    /// orders are not followed.
    pub(super) fn fixed_tracks(&mut self, id: u32) {
        let building = self
            .body_of(id)
            .is_some_and(|b| b.class == super::bodies::Class::Building);

        if building && self.units.tracks(id).is_none() {
            let facing = self.facing_word(id);
            let walker = self
                .position(id)
                .zip(self.board.ground())
                .map(|(at, ground)| Walker::fixed(ground, at, facing));

            if let Some(walker) = walker {
                self.units.entry(id).tracks = Some(Tracks {
                    walker,
                    goal: Goal::Point((0, 0)),
                    avoid: Avoid::default(),
                    intents: Vec::new(),
                    shot: None,
                    proc: 0,
                    script: None,
                    pending: None,
                    overlay: None,
                    pending_overlay: None,
                    sounding: None,
                    pending_sounding: None,
                    released: false,
                    animate: false,
                    flash: None,
                });
            }
        }
    }

    /// The set piece `id`'s parts show, as its walker last played it.
    pub(super) fn playing(&self, id: u32) -> Option<u32> {
        self.units
            .tracks(id)
            .and_then(|t| t.walker.look())
            .map(crate::sim::movement::action::Look::set)
    }

    pub(super) fn body_of(&self, id: u32) -> Option<&super::bodies::Body> {
        self.descriptors.body(self.any_piece(id)?.code)
    }

    /// The body of agent `id`'s bound piece: [`World::body_of`] without
    /// the new game's unannounced.
    pub(super) fn piece_body(&self, id: u32) -> Option<&super::bodies::Body> {
        self.descriptors.body(self.pieces.code(id)?)
    }

    // -----------------------------------------------------------------------
    // The builtins

    /// What an Action of set `set` plays on piece `id`'s track 2: that set
    /// of its art; set 0, or one its art lacks, plays nothing.
    /// `None` without its art. A set that
    /// keeps the frame starts where the parts are ([`Pose::keeps`]).
    pub(super) fn pose(&self, id: u32, set: u32) -> Option<Pose> {
        let poses = self
            .descriptors
            .body(self.any_piece(id)?.code)?
            .poses
            .as_ref()?;

        let pose = poses
            .get(&set)
            .filter(|_| set != 0)
            .cloned()
            .unwrap_or_default();

        Some(pose)
    }

    /// What piece `id`'s `InitAction` of `act` does to its facing as it
    /// ends: with an after-set other than 0, it goes back
    /// to its slot's first direction, over the slots of the set the parts
    /// show: the action's, else, the art lacking it, `Stand`'s
    /// (**Inferred**: a walk's end plays `Stand`, `OrderMovePath`). `None`
    /// without the art.
    fn settle(&self, id: u32, act: Act) -> Option<Settle> {
        let slots = |set| self.pose(id, set).map(|p| p.frames.len());

        match act.after {
            0 | -1 => Some(Settle::Keep),
            _ => slots(act.set)
                .filter(|n| *n > 0)
                .or_else(|| slots(STAND))
                .map(Settle::Slot),
        }
    }

    /// Whether agent `id`'s threads are skipped: an order holding it, its
    /// Action or track 2 after it, sets its byte,
    /// which
    /// agent threads run tests; the order's end clears
    /// it. `Move` gives its orders none.
    /// **Game**: the inn probe's peasants run their threads
    /// while walking out by `Move` and home by `Hide`. False where the world does not follow the piece:
    /// the log then says when its threads run.
    #[must_use]
    pub fn holds(&self, id: u32) -> bool {
        held_of(self.units.motion(id), self.units.tracks(id))
    }

    /// Whether piece `id`'s track 2 holds an order (`GenUnit` method `(2)`).
    pub(super) fn track2(&self, id: u32) -> bool {
        self.units.tracks(id).is_some_and(|t| t.walker.acting())
    }
}

impl World {
    /// Piece `id`'s facing, what a save's `VHCL` record holds as its
    /// direction: its walker's, else [`FIRST_FACING`] for a piece the
    /// world made; `None` for a new game's piece never moved, or one whose
    /// place is unknown.
    #[must_use]
    pub fn facing(&self, id: u32) -> Option<i32> {
        match (self.known(id), self.units.tracks(id)) {
            (Some(_), Some(t)) => Some(t.walker.facing),
            (Some(_), None) => self.units.followed(id).then_some(FIRST_FACING),
            (None, _) => None,
        }
    }
}

/// Write a known boolean answer.
fn boolean(
    vm: &mut Vm<'_>,
    args: &[Handle],
    answer: Option<bool>,
) -> std::result::Result<Answer, Fault> {
    match answer {
        Some(b) => {
            let ret = vm.arg(args, 0)?;

            put_bool(vm, ret, b)?;
            Ok(Answer::Given)
        }
        None => Ok(Answer::Unknown),
    }
}

/// Whether `played` leaves a looping `Animate` on track 1, over art
/// `poses` at `facing`: unit play set pushes one (flags `0x50`,
/// `GE_OrderProcessor`) only for a set the art has
/// in more than one frame there. A spent `MovePath` plays set `0xb1`
/// instead where the art has it, once, on track 0
/// (path end stop, [`Walker`]): nothing on track 1.
fn loops(played: Track1, poses: &super::bodies::Poses, facing: i32) -> bool {
    let plays = |set: u32| set != 0 && poses.get(&set).is_some_and(|p| p.frames_at(facing) > 1);

    match played {
        Track1::Clear => false,
        Track1::Plays(set) => plays(set),
        Track1::PathEnd => !poses.contains_key(&PATH_END_SET) && plays(STAND),
    }
}

/// [`World::holds`] of a unit whose `motion` and `tracks` are read already.
pub(super) fn held_of(motion: Option<super::places::Motion>, tracks: Option<&Tracks>) -> bool {
    motion != Some(super::places::Motion::Walking) && tracks.is_some_and(Tracks::held)
}

#[cfg(test)]
mod tests {
    use super::{Flash, PATH_END_SET, Track1, World, loops};
    use crate::base::fixed::Fixed;
    use crate::sim::movement::Walker;
    use crate::sim::movement::{STAND, action::Pose};
    use crate::sim::placement::Heights;
    use crate::sim::world::kind;

    /// A set the art lacks plays nothing, so
    /// neither does a walk's `Stand` there; a walk's end plays set `0xb1`
    /// instead where the art has it, holding nothing; an after-set
    /// `Stand` loops either way.
    #[test]
    fn a_stand_loops_where_the_art_has_it_in_frames() {
        let stand = Pose {
            frames: vec![4; 8],
            ..Pose::default()
        };
        let still = Pose {
            frames: vec![1; 8],
            ..Pose::default()
        };
        let art = |sets: &[(u32, &Pose)]| {
            sets.iter()
                .map(|(s, p)| (*s, (*p).clone()))
                .collect::<super::super::bodies::Poses>()
        };
        let walker = art(&[(STAND, &stand)]);
        let dryad = art(&[(STAND, &stand), (PATH_END_SET, &still)]);

        assert!(loops(Track1::PathEnd, &walker, 0));
        assert!(loops(Track1::Plays(STAND), &walker, 0));
        assert!(!loops(Track1::PathEnd, &dryad, 0));
        assert!(loops(Track1::Plays(STAND), &dryad, 0));
        assert!(!loops(Track1::PathEnd, &art(&[]), 0));
        assert!(!loops(Track1::PathEnd, &art(&[(STAND, &still)]), 0));
        assert!(!loops(Track1::Clear, &walker, 0));
    }

    /// A flash pushed at 0 runs then and every 200 ms, its count up by
    /// one a run; the run finding it at 20, at 4,000 ms, ends it. A hit at
    /// 1,000 ms sets the count back: 20 runs more from the next, 1,200 ms.
    #[test]
    fn a_combat_signal_lasts_twenty_runs_of_200_ms() {
        let ends = |hit: Option<u32>| {
            let mut flash = Flash::pushed(0);

            // The orders run before the scripts that hit.
            (1..400).map(|f| f * 50).find(|now| {
                let ended = !flash.run(*now);

                if Some(*now) == hit {
                    flash.hit();
                }

                ended
            })
        };

        assert_eq!(ends(None), Some(4000));
        assert_eq!(ends(Some(1000)), Some(5200));
    }

    /// Level ground.
    struct Flat;

    impl Heights for Flat {
        fn across(&self) -> u32 {
            64
        }

        fn height(&self, _: u32, _: u32) -> Option<Fixed> {
            Some(Fixed::ZERO)
        }
    }

    /// A walk step moves the place, but is no jump.
    #[test]
    fn a_walk_step_is_no_jump() {
        let mut world = World::default();

        world.pieces.insert(
            1,
            super::super::ruin::tests::standing(*b"BVr1", 100, kind::Kind::VEHICLE, (10, 10)),
        );
        world
            .units
            .set_motion(1, super::super::places::Motion::Still);
        world.moved(1, &Walker::fixed(&Flat, (170, 160), 0));

        let shown = world.shown_one(1).map(|s| (s.at, s.jumps));

        assert_eq!(shown, Some(((170, 160), 0)));
    }
}
