//! How a walker moves, as `MajestyHD.exe` runs it: orders, turns and
//! steps. **Binary** unless marked; notes in `docs/original/movement.md`.
//!
//! ```text
//!  game frame step      ── GE_Level, every frame
//!          every unit of list ORDR: GenUnit, tracks 0, 1, 2
//!            track 0  motion: the top order only    MovePath, Move, Turn
//!            track 1  intent: every order           FollowUnit, FollowPath
//!            track 2  action: the top order only    turn, InitAction
//!          units with no order leave ORDR
//!    GPL scheduler tick                             scripts: Move, IsMoving
//!
//!  an order runs when  now - when >= delta          (OrderStack)
//!
//!  Move(agent, unit)  ─> FollowUnit, every 500 ms: not adjacent and
//!                        track 0 empty ─> MovePath
//!  MovePath, every frame it is on top:
//!    plan when the path is spent ─> next half cell ─> claim it
//!    facing differs ─> Turn: 4 of 32 a frame; done ─> Move, exact
//!    else           ─> Move, overshooting: a full step past the node
//!    a walker there ─> Wait 1000 ms (wait_for), or plan round walkers
//!  Move: one step every `rate` ms, 3D, until within a step of the node
//!  Hide(agent, building) ─> FollowPath, every frame: its MovePath ended,
//!                           enter within a squared gap of ENTER_GAP2
//! ```
//!
//! [`Walker`] walks, alone or among what [`Around`] knows; [`plan`] is
//! the planner it calls.

pub mod action;
pub mod plan;

use crate::base::fixed::{Fixed, ONE, div, from_float, from_whole, mul, whole};
use crate::content::terrain::grid::Terrain;
use crate::data::desc::Part;
use crate::data::error::{Result, u32_at};
use crate::sim::placement::Heights;

use plan::{Blocking, Cell};

use super::units::{PER_CELL_FIXED, from_pos32, to_pos32};

use crate::gpl::runtime::MS_PER_FRAME;

// ---------------------------------------------------------------------------
// Settled numbers

/// Directions in a circle. 0 is north, `-y`; 8 east, `+x`; clockwise.
pub const DIRECTIONS: i32 = 32;

/// `FollowUnit`'s re-check: 500 written in
/// `GE_OrderProcessor`.
pub const FOLLOW_MS: u32 = 500;

/// `MovePath`'s rate: 0, so it runs in every frame it is on top.
pub const PATH_MS: u32 = 0;

/// A planned path keeps its first 10 nodes, the start among them:
/// `RepathLength`, set by path config. The walker
/// plans again at the ninth step.
pub const REPATH_LENGTH: usize = 10;

/// At most 1024 nodes expanded a plan: `PathHorizon`, the same singleton.
pub const PATH_HORIZON: usize = 1024;

/// Replans before the kept length doubles: `ExpandCount` at expand count.
pub const EXPAND_COUNT: u32 = 4;

/// How long a walker blocked by another waits: `CollisionDelay` at
/// collision delay, ms.
pub const COLLISION_MS: u32 = 1000;

/// Waits in a row before a blocked walker plans around: the word at
/// wait limit, read in `OrderMovePath`. The name is ours. The count is
/// reset only when it runs out or the walker does not wait.
pub const WAIT_LIMIT: u32 = 4;

/// Path nodes path after wait looks along, counted from the
/// path's start: a count of 5 or more is 5.
pub const NUDGE_NODES: usize = 5;

/// A walker home from `Hide` enters when the squared pad-rect gap to the
/// building is at most this, in half cells: the word at enter gap2,
/// read by callback `0x1003`; else it stays outside.
pub const ENTER_GAP2: i32 = 4;

/// A per-axis cap on an overshooting step: 15/32 of a cell,
/// `fixed_div(0xf0000, 0x200000)` in step cap.
pub const STEP_CAP: Fixed = from_pos32(15);

/// What `Move`'s third argument sets in an order's info word.
pub const MODES: [(&str, u32); 5] = [
    ("add_waypoint", 0x1),
    ("loop", 0x3),
    ("avoid_vehicles", 0x4),
    ("avoid_buildings", 0x8),
    ("avoid_everything", 0xc),
];

// The path orders' info bits, as `Walker` fields: `0x8000` its first `Move`
// steps at once ([`Walker`]'s `first`), `0x400` the next plan counts walkers
// (`walkers`), `0x2000` blocked again after a wait it nudges (`after_wait`),
// `0x4000` a path `Move` overshoots ([`Finish::Overshoot`]). With `0x200`
// planning counts only what stands still.

// ---------------------------------------------------------------------------
// The movement class

/// A version 2 `PRIM`'s words, by what [`Class`] takes of them. Word 5 is
/// unread here; words 9 to 12 are read and dropped. **Binary**.
mod word {
    pub const TURN_RATE: usize = 0;
    pub const TURN_STEP: usize = 1;
    pub const MOVE_RATE: usize = 2;
    pub const MOVE_DIST: usize = 3;
    pub const SIZE: usize = 4;
    pub const LEAST_HEIGHT: usize = 6;
    pub const FLAGS: usize = 7;
    pub const LIFT: usize = 8;
}

/// The tag of a `DMOV` record's `PRIM` part.
const PRIM: &[u8; 4] = b"PRIM";

/// A `DMOV` record's `PRIM`: `MovementDesc_Linear_Engine`, from its
/// first nine words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Class {
    /// Ms per turn step.
    pub turn_rate: u32,
    /// Directions a turn step.
    pub turn_step: u32,
    /// Ms per step.
    pub move_rate: u32,
    /// Thirty-seconds of a cell a step.
    pub move_dist: u32,
    /// Over 16, the ancestors a large unit's plan checks for
    /// turns. **Inferred**: a size.
    pub size: u32,
    /// A flier's least height and lift, in 32nds:
    /// `PRIM` words 6 and 8.
    pub lift: (u32, u32),
    /// `PRIM` word 7: bit 0 turns a path's first steps into an
    /// animation, bit 2 lobs a projectile ([`LOBBED`]),
    /// bit 5 keeps the facing through a one-node jog ([`KEEPS_FACING`]).
    pub flags: u32,
}

impl Class {
    /// `PRIM` bytes a `DMOV` holds: 13 words, `+0x04` to `+0x37`.
    pub const LEN: usize = 52;

    /// Read a `DMOV` record.
    ///
    /// # Errors
    ///
    /// If it has no `PRIM` or the `PRIM` is short.
    pub fn parse(payload: &[u8]) -> Result<Class> {
        let tree = Part::parse(payload)?;
        let prim = tree
            .parts
            .iter()
            .flat_map(|p| std::iter::once(p).chain(p.parts.iter()))
            .find(|p| &p.tag.0 == PRIM)
            .ok_or(crate::data::error::Error::BrokenInvariant {
                what: "DMOV PRIM",
                expected: 1,
                found: 0,
            })?;
        let read = |word: usize| u32_at(prim.body, "DMOV PRIM word", 4 * word);

        Ok(Class {
            turn_rate: read(word::TURN_RATE)?,
            turn_step: read(word::TURN_STEP)?,
            move_rate: read(word::MOVE_RATE)?,
            move_dist: read(word::MOVE_DIST)?,
            size: read(word::SIZE)?,
            lift: (read(word::LEAST_HEIGHT)?, read(word::LIFT)?),
            flags: read(word::FLAGS)?,
        })
    }

    /// A flier's least height and lift as heights, 16.16 cells: words,
    /// ([`Class::lift`]) over 32 (`MovementDesc_Linear_Engine` puts them).
    /// A path's step aims at the higher of the ground
    /// plus the lift and the least height (path step order,
    /// path step order), on movement layer 4. **Inferred**: the class with a
    /// least height, `ML04`, the Large Flyer, alone in HD, is that layer's.
    #[must_use]
    pub const fn flight(&self) -> Option<(Fixed, Fixed)> {
        if self.lift.0 == 0 {
            None
        } else {
            Some((
                div(from_whole(self.lift.0.cast_signed()), PER_CELL_FIXED),
                div(from_whole(self.lift.1.cast_signed()), PER_CELL_FIXED),
            ))
        }
    }

    /// Whether a projectile of this class is lobbed: `GenUnit` sends
    /// it to a point fixed at launch on a parabola (`GE_OrderProcessor`,
    /// method), not homing, when the class's flag word has bit 2
    /// (bit `0x04`, tested in `GenUnit`): [`Class::flags`],
    /// `PRIM` word 7 (1 on the floating and flying classes, 2 on the
    /// missiles, 16 cross terrain, 6 on `Parabolic_Missile` alone).
    /// **Game**: the catapult's ball, of `Parabolic_Missile`, flies
    /// method's parabola (**Measured**); the
    /// `fast missile` clouds and arrows home.
    ///
    /// The ball's class alone has `PRIM` word 7 bit 2: `GenUnit`
    /// flies the missile to a point fixed at launch,
    /// else homing:
    /// Bit `0x04`, tested in `GenUnit`.
    #[must_use]
    pub const fn lobbed(&self) -> bool {
        self.flags & LOBBED != 0
    }
}

/// A rate as a unit runs it: the class's plus a modifier attribute
/// (`MovementRateModifier`, `APV:34`, or `TurnRateModifier`, `APV:51`),
/// rounded to whole frames from a frame up, at least 1.
///
/// **Binary**: `GSUnit`, method in `GSUnit`.
///
/// ```
/// use regent_core::sim::movement::rate;
///
/// assert_eq!(rate(90, 0), 100); // Class 1 walks a step every 2 frames
/// assert_eq!(rate(50, 0), 50);
/// assert_eq!(rate(30, 0), 30);
/// ```
#[must_use]
pub const fn rate(base: u32, modifier: i32) -> u32 {
    let frame = MS_PER_FRAME.cast_signed();
    let r = base.cast_signed() + modifier;
    let r = if r >= frame {
        (frame / 2 + r) / frame * frame
    } else {
        r
    };

    if r > 1 { r.cast_unsigned() } else { 1 }
}

/// The flag bit [`Class::lobbed`] tests: `GenUnit`.
pub const LOBBED: u32 = 0x4;

/// [`Class::flags`] bit that keeps the facing through a one-node jog:
/// the Large Flyer's.
pub const KEEPS_FACING: u32 = 0x20;

/// Class flag `0x10`: its paths carry info `0x1000`, so it
/// walks through what `GSUnit` calls clearable, scatter and ruins
/// (blocker default, [`plan::Occupant::tramples`]).
pub const TRAMPLES: u32 = 0x10;

/// [`Class::flags`] bit that gives a path's `Move`s info `0x80`:
/// a step then leaves the frame alone.
pub const FRAMELESS: u32 = 0x1;

/// Game ms between two runs of an order re-armed with `rate`: a run sets
/// `when = now`, `delta = rate`, and the
/// order runs in the first frame at least `delta` later.
/// So never under a frame.
///
/// ```
/// use regent_core::sim::movement::interval;
///
/// assert_eq!(interval(100), 100);
/// assert_eq!(interval(30), 50); // once a frame
/// assert_eq!(interval(1), 50);
/// ```
#[must_use]
pub const fn interval(rate: u32) -> u32 {
    rate.div_ceil(MS_PER_FRAME) * MS_PER_FRAME
}

/// Whether a step is held to [`STEP_CAP`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    /// Arrive on the node: a `Move` after a turn.
    Exact,
    /// Always a full step, capped per axis: a path's `Move` (info
    /// `0x4000`).
    Overshoot,
}

/// A step's length in 16.16 cells: `move_dist` 32nds, times frames a
/// step when the rate is under a frame.
///
/// **Binary**: move step length.
///
/// ```
/// use regent_core::sim::movement::{Finish, step_length};
/// use regent_core::sim::units::from_pos32;
///
/// assert_eq!(step_length(6, 100, Finish::Overshoot), from_pos32(6));
/// ```
#[must_use]
pub fn step_length(move_dist: u32, rate: u32, finish: Finish) -> Fixed {
    let base = div(from_whole(move_dist.cast_signed()), PER_CELL_FIXED);
    let frame = MS_PER_FRAME.cast_signed();
    let scale = if rate.cast_signed() < frame {
        div(from_whole(frame), from_whole(rate.cast_signed()))
    } else {
        ONE
    };
    let step = mul(base, scale);

    match finish {
        Finish::Overshoot => step.clamp(-STEP_CAP, STEP_CAP),
        Finish::Exact => step,
    }
}

/// Directions a turn step, from the rate: `GE_OrderProcessor`.
#[must_use]
pub const fn turn_step(class_step: u32, rate: u32) -> i32 {
    let frame = MS_PER_FRAME;
    let steps = if rate < frame { frame / rate } else { 1 };

    (steps * class_step).cast_signed()
}

// ---------------------------------------------------------------------------
// Fixed point, as the engine's

/// `fixed_sqrt(v, 16)` in fixed sqrt: floored.
#[must_use]
pub const fn fsqrt(v: Fixed) -> Fixed {
    Fixed::from_bits(
        crate::gpl::vm::native::fixed_sqrt(v.to_bits().cast_unsigned(), 16).cast_signed(),
    )
}

// ---------------------------------------------------------------------------
// Directions

/// The direction from one point toward another.
///
/// **Binary**: direction of. Each quadrant splits on the ratio of the
/// shorter axis to the longer, `* 100 / 25`, so a diagonal is exact and
/// in between is coarse.
///
/// ```
/// use regent_core::sim::movement::direction;
///
/// assert_eq!(direction(0, -1), 0);
/// assert_eq!(direction(1, -1), 4);
/// assert_eq!(direction(-1, -1), 28);
/// assert_eq!(direction(16, -9), 6);
/// ```
#[must_use]
pub fn direction(dx: i32, dy: i32) -> i32 {
    let (ax, ay) = (dx.abs(), dy.abs());
    let q = |a: i32, b: i32| a * 100 / b / 25;
    let d = if ax == 0 {
        if dy < 0 { 0 } else { 16 }
    } else if ay == 0 {
        if dx < 0 { 24 } else { 8 }
    } else if dx > 0 && dy < 0 {
        if ay < ax { 8 - q(ay, ax) } else { q(ax, ay) }
    } else if dx > 0 {
        if ay < ax {
            q(ay, ax) + 8
        } else {
            16 - q(ax, ay)
        }
    } else if dy > 0 {
        if ay < ax {
            24 - q(ay, ax)
        } else {
            q(ax, ay) + 16
        }
    } else if ay < ax {
        q(ay, ax) + 24
    } else {
        DIRECTIONS - q(ax, ay)
    };

    if d >= DIRECTIONS { d - DIRECTIONS } else { d }
}

/// One run of a turn: toward `want` by `step` directions, or onto it.
///
/// **Binary**: `OrderTurn` with turn gap (the
/// shorter way) and direction add (the wrap). The wrap takes only values
/// over 32 back, so a facing can read 32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turned {
    /// Still turning; the new facing. Runs again after the turn rate.
    Toward(i32),
    /// Facing `want`: the turn order ends and its `Move` starts.
    Onto(i32),
}

/// See [`Turned`].
///
/// ```
/// use regent_core::sim::movement::{Turned, turn};
///
/// assert_eq!(turn(0, 28, 4), Turned::Onto(28));
/// assert_eq!(turn(16, 28, 4), Turned::Toward(20));
/// assert_eq!(turn(28, 4, 4), Turned::Toward(32));
/// ```
#[must_use]
pub const fn turn(facing: i32, want: i32, step: i32) -> Turned {
    let raw = (want - facing).abs();
    let (gap, sign) = if raw > DIRECTIONS / 2 {
        (DIRECTIONS - raw, if want <= facing { 1 } else { -1 })
    } else {
        (raw, if facing < want { 1 } else { -1 })
    };

    if gap > 0 && step > 0 && step < gap {
        let mut d = facing + sign * step;

        if d > DIRECTIONS {
            d -= DIRECTIONS;
        }

        if d < 0 {
            d += DIRECTIONS;
        }

        Turned::Toward(d)
    } else {
        Turned::Onto(want)
    }
}

// ---------------------------------------------------------------------------
// The ground and a step

/// The ground's height under a 16.16 point: ground height. Two triangles
/// a cell, cut by `(x ^ y)` parity, as the ground is drawn; 0 off the map
/// and on its last cell row and column: the test is the cell plus one
/// against the cells, not the vertices.
#[must_use]
pub fn ground(heights: &dyn Heights, x: Fixed, y: Fixed) -> Fixed {
    let (ix, iy) = (whole(x), whole(y));
    let cells = heights.across().cast_signed() - 1;

    if ix < 0 || iy < 0 || ix + 1 >= cells || iy + 1 >= cells {
        return Fixed::ZERO;
    }

    let (fx, fy) = (x - from_whole(ix), y - from_whole(iy));
    let h = |dx: i32, dy: i32| {
        heights
            .height((ix + dx).cast_unsigned(), (iy + dy).cast_unsigned())
            .unwrap_or(Fixed::ZERO)
    };
    let (v00, v10, v01, v11) = (h(0, 0), h(1, 0), h(0, 1), h(1, 1));

    if (ix ^ iy) & 1 == 0 {
        if fy < fx {
            mul(fy, v11 - v10) + mul(ONE - fx, v00 - v10) + v10
        } else {
            mul(ONE - fy, v00 - v01) + mul(fx, v11 - v01) + v01
        }
    } else if fx + fy < ONE {
        mul(fy, v01 - v00) + mul(fx, v10 - v00) + v00
    } else {
        mul(ONE - fy, v10 - v11) + mul(ONE - fx, v01 - v11) + v11
    }
}

/// A save's ground, as a walker stands on it: the file's `f32` heights
/// are 16.16 values ([`Terrain::FIXED`]), so the conversion is exact.
impl Heights for Terrain<'_> {
    fn across(&self) -> u32 {
        Terrain::across(self)
    }

    fn height(&self, x: u32, y: u32) -> Option<Fixed> {
        self.vertex(x, y).map(|v| from_float(v.height))
    }
}

/// A GPL coordinate as a 16.16 point on the ground:
/// Cell pos of pos32.
#[must_use]
pub fn on_ground(heights: &dyn Heights, at: (i32, i32)) -> [Fixed; 3] {
    let x = div(from_whole(at.0), PER_CELL_FIXED);
    let y = div(from_whole(at.1), PER_CELL_FIXED);

    [x, y, ground(heights, x, y)]
}

/// A point on the ground raised to a flier's height: the higher of the
/// ground plus its lift and its least height ([`Class::flight`]).
#[must_use]
pub fn aloft(at: [Fixed; 3], flight: Option<(Fixed, Fixed)>) -> [Fixed; 3] {
    match flight {
        Some((least, lift)) => {
            let over = at[2] + lift;

            [at[0], at[1], if over > least { over } else { least }]
        }
        None => at,
    }
}

/// Whether a step leaves its order running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leg {
    /// More than a step was left: the `Move` runs again after its rate.
    More,
    /// The last step: the `Move` ends.
    Done,
}

/// One run of a `Move`: from `at` toward `to`, in three dimensions, so a
/// slope shortens the step on the ground.
///
/// **Binary**: `OrderMove`. The distance is taken
/// before the step; within a step, [`Finish::Exact`] lands on `to` and
/// [`Finish::Overshoot`] still goes a full step.
#[must_use]
pub fn advance(at: [Fixed; 3], to: [Fixed; 3], step: Fixed, finish: Finish) -> ([Fixed; 3], Leg) {
    let d = [to[0] - at[0], to[1] - at[1], to[2] - at[2]];
    // The engine's plain `add`s wrap past 32767 and its root reads the
    // sum unsigned, so a goal up to 256 tiles off still measures right.
    let dist = fsqrt(
        mul(d[0], d[0])
            .wrapping_add(mul(d[1], d[1]))
            .wrapping_add(mul(d[2], d[2])),
    );
    let far = dist > Fixed::ZERO && step > Fixed::ZERO;
    let leg = if far && dist > step {
        Leg::More
    } else {
        Leg::Done
    };

    if far && (dist > step || finish == Finish::Overshoot) {
        let mut s = d.map(|c| div(mul(c, step), dist));

        if finish == Finish::Overshoot {
            s[0] = s[0].clamp(-STEP_CAP, STEP_CAP);
            s[1] = s[1].clamp(-STEP_CAP, STEP_CAP);
        }

        ([at[0] + s[0], at[1] + s[1], at[2] + s[2]], leg)
    } else {
        (to, leg)
    }
}

/// A 16.16 point as a GPL coordinate, what the probe prints.
#[must_use]
pub const fn pos32(at: [Fixed; 3]) -> (i32, i32) {
    (to_pos32(at[0]), to_pos32(at[1]))
}

// ---------------------------------------------------------------------------
// A walk

/// What a walker's track 0 holds on top of its `MovePath`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Top {
    Turn {
        want: i32,
        to: [Fixed; 3],
    },
    Move {
        to: [Fixed; 3],
        finish: Finish,
    },
    /// Order 9 for [`COLLISION_MS`]: its first run ends it.
    Wait,
    /// An `Animate` of [`PATH_END`], once: a spent path's.
    End,
}

/// A timed order: runs when `now - when >= delta`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Timed<T> {
    order: T,
    when: u32,
    delta: u32,
}

impl<T> Timed<T> {
    const fn due(&self, now: u32) -> bool {
        now.wrapping_sub(self.when) >= self.delta
    }
}

/// A planner as a walker calls it: from the half cell it holds, keeping
/// at most so many nodes, the start first ([`plan::plan`] with the start
/// and goal floods). One node or none ends the walk.
pub type Planner<'a> = dyn FnMut((i32, i32), usize) -> Vec<(i32, i32)> + 'a;

/// What a walker asks of the units about it, in `OrderMovePath`.
pub trait Around {
    /// A plan from half cell `from`, keeping `keep` nodes, the start first:
    /// the start and goal floods, then [`plan::plan`] counting `how`.
    fn plan(&mut self, from: Cell, keep: usize, how: Blocking) -> Vec<Cell>;

    /// Whether the start flood from `from` lands on the goal's cell.
    fn at_goal(&mut self, from: Cell) -> bool;

    /// Whether the walker fits on `at`, counting `how`: not
    /// footprint blocked.
    fn fits(&mut self, at: Cell, how: Blocking) -> bool;

    /// Whether to wait for whoever stands on `at`, the walker wanting it:
    /// Walker wait test, which [`wait_for`] models.
    fn wait(&mut self, at: Cell) -> bool;

    /// A walker that waited, planned round walkers in vain and is blocked
    /// again: path after wait looks along `ahead`,
    /// the kept nodes from the one held to the fifth, for a unit with a
    /// movement class, not frozen (`APV:63`) and not hostile;
    /// if its handle, 0 when still, is below the
    /// walker's, 0 when still, it gets a `FollowPath` to the walker's own
    /// goal.
    fn nudge(&mut self, ahead: &[Cell]);

    /// The `MovePath`'s point (, 32nds) where its half
    /// cell, over 16 toward zero, lies off the map (half cell on map
    /// in `OrderMovePath`): a spent path then ends in a last `Move` to it.
    /// By default a walk stays on the map.
    fn beyond(&mut self) -> Option<(i32, i32)> {
        None
    }
}

/// A planner alone on the map: nothing moves, so nobody waits.
struct Alone<'p, 'a>(&'p mut Planner<'a>);

impl Around for Alone<'_, '_> {
    fn plan(&mut self, from: Cell, keep: usize, _: Blocking) -> Vec<Cell> {
        (self.0)(from, keep)
    }

    fn at_goal(&mut self, _: Cell) -> bool {
        true
    }

    fn fits(&mut self, _: Cell, _: Blocking) -> bool {
        true
    }

    fn wait(&mut self, _: Cell) -> bool {
        false
    }

    fn nudge(&mut self, _: &[Cell]) {}
}

/// A unit as a blocked walker weighs it: walker wait test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Waiter {
    /// The half cell it waits for (the unit's wait
    /// record): set as it finds a walker in its way, cleared
    /// as it steps or its path ends.
    pub wants: Option<Cell>,
    /// Cells a ms, [`Walker::pace`]; 0 without a class.
    pub pace: Fixed,
    /// `IsMoving`: track 0 holds an order, a `Wait` too.
    pub moving: bool,
}

/// Whether walker `me` waits for the unit on the cell it wants.
///
/// **Binary**: walker wait test. No wait in a ring of walkers each waiting for
/// the next (wait ring clear follows `wants` through `blocker`, the first
/// unit in the way there, cell blocker first). Else the blocker's: none, wait;
/// else wait while it is moving and no slower than `me`: `jnle` at
/// walker wait test skips the wait when `me`'s pace is the greater.
#[must_use]
pub fn wait_for(
    me: usize,
    units: &[Waiter],
    blocker: &dyn Fn(usize, Cell) -> Option<usize>,
) -> bool {
    fn clear(
        i: usize,
        seen: &mut Vec<usize>,
        units: &[Waiter],
        blocker: &dyn Fn(usize, Cell) -> Option<usize>,
    ) -> bool {
        match units[i].wants {
            None => true,
            Some(_) if seen.contains(&i) => false,
            Some(c) => {
                seen.push(i);

                let ok = blocker(i, c).is_none_or(|b| clear(b, seen, units, blocker));

                seen.pop();
                ok
            }
        }
    }

    let wanted = units[me].wants.and_then(|c| blocker(me, c));
    // `clear` asks first for `me`'s blocker, just found: not again.
    let known = |i: usize, c: Cell| {
        if i == me && Some(c) == units[me].wants {
            wanted
        } else {
            blocker(i, c)
        }
    };

    clear(me, &mut vec![], units, &known)
        && wanted.is_none_or(|b| units[me].pace <= units[b].pace && units[b].moving)
}

/// The set a walker's own orders last played.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gait {
    /// Its walk: a step puts `Walk` on the parts.
    Walk,
    /// Set 8, `Stand`: a wait on a walker in the way plays it after the
    /// push. The wait on walkers short of the goal
    /// plays none.
    Stand,
}

/// The set the parts show, as far as a step cares. A step puts `Walk`
/// on (move set position: `UnitIMG` method `(1)` before the frame
/// moves on), which from any other set starts it on frame 0 unless the
/// sets share a frame-keeping bit ([`action::Phase::put`]); on `Walk`
/// already it changes nothing. A wait puts `Stand` on so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parts {
    Walk,
    Stand,
    /// An action's set, `Dead`, or the set a unit is made in.
    Other,
}

/// `Stand`, the set a `MovePath` spent on the map plays.
pub const STAND: u32 = 8;

/// `Walk`, the set a frameless class's path plays as it is pushed.
pub const WALK: u32 = 1;

/// The set a spent `MovePath` plays once on track 0 where the art has
/// it, instead of [`STAND`].
pub const PATH_END: u32 = crate::data::imag::SetKind::WalkToStand.id();

/// What a walker's own orders did to track 1's `Animate`s, for the world:
/// a looping `Animate` there is an order, so it keeps the unit in ORDR
/// (`GE_Level` drops only a unit with no order on any track).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Track1 {
    /// unit play set of a set: an `Animate` per part on track 1,
    /// flags `0x50`, so it loops until taken off (`GE_OrderProcessor`,
    /// `GE_OrderProcessor`); none for a set the art lacks or a slot of one
    /// frame. An `InitAction`'s after-set.
    Plays(u32),
    /// A spent `MovePath`: set `0xb1` where the
    /// art has it, else [`STAND`] as
    /// [`Track1::Plays`].
    PathEnd,
    /// `GenUnit` method `(0, -1, 1)`: an `InitAction`'s end takes track 1's
    /// `Animate`s off, its after-set 0.
    Clear,
}

/// One walker: track 0 as the engine runs it — `MovePath`, a turn, a
/// step, a wait — one run a frame, planning when its path is spent. Held
/// against the probe traces in `tests/movement.rs`.
#[derive(Clone, Debug)]
pub struct Walker {
    pub at: [Fixed; 3],
    pub facing: i32,
    /// The kept plan, its start first, and the index of the node held.
    nodes: Vec<(i32, i32)>,
    index: usize,
    /// Nodes a plan keeps, and plans since it last doubled.
    keep: usize,
    plans: u32,
    /// The half cell it holds,: claimed as a step toward it
    /// starts.
    cell: (i32, i32),
    first: bool,
    path: Option<Timed<()>>,
    top: Option<Timed<Top>>,
    /// The half cell a walker in the way keeps it from, and waits in a
    /// row.
    wants: Option<Cell>,
    waits: u32,
    /// Info `0x400`: the next plan counts walkers; info `0x2000`: blocked
    /// after a wait, it nudges.
    walkers: bool,
    after_wait: bool,
    /// A frame's step, uncapped: [`Walker::step_of`] caps it per `Move`.
    step: Fixed,
    move_rate: u32,
    /// The class's move rate, and `MovementRateModifier` as the world
    /// last gave it ([`Walker::modify_rate`]).
    class_rate: u32,
    rate_modifier: i32,
    move_dist: u32,
    turn_rate: u32,
    turn_step: i32,
    /// The class's flags: [`Class::flags`].
    flags: u32,
    /// A flier's least height and lift, cells: [`Class::flight`].
    flight: Option<(Fixed, Fixed)>,
    /// Track 2: an action's turn and `InitAction` ([`action`]).
    stage: Option<action::Stage>,
    gait: Gait,
    /// The sets track 2's action plays, and what the last left showing;
    /// a step, or a wait's `Stand`, clears it.
    sets: action::Sets,
    left: Option<action::Look>,
    /// A set put on the parts outside any order, shown at frame 0: a
    /// dying unit's `Dead` ([`Walker::lay`]). A step, a wait's `Stand` or
    /// the next action clears it.
    laid: Option<u32>,
    /// What its orders last did to track 1's `Animate`s, until the world
    /// takes it ([`Walker::take_played`]).
    played: Vec<Track1>,
    /// Whether the unit has a movement: [`Walker::fixed`]'s has none.
    movement: Movement,
    /// The frame its parts show, and what steps it ([`action::Phase`]).
    phase: action::Phase,
    parts: Parts,
    /// Its art's `Walk` and `Stand`, which its own orders play; `None`
    /// for a set the art lacks, which `UnitIMG` then leaves off.
    walk: Option<Box<action::Pose>>,
    stand: Option<Box<action::Pose>>,
    /// Its art's [`PATH_END`], which a spent path plays.
    end: Option<Box<action::Pose>>,
    /// The set its parts show, as its own orders last played it: what
    /// action face loops through a turn ([`Walker::act`]).
    shown: Option<Box<action::Pose>>,
}

/// Whether a unit has a movement, `unit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Movement {
    Moves,
    /// None, a building's: an action's turn sets the facing at once.
    Fixed,
}

impl Walker {
    /// A walker standing at `at` (a GPL coordinate on the ground), facing
    /// `facing`, whose `MovePath` is pushed at game time `now`.
    #[must_use]
    pub fn new(
        heights: &dyn Heights,
        at: (i32, i32),
        facing: i32,
        class: &Class,
        now: u32,
    ) -> Walker {
        let move_rate = rate(class.move_rate, 0);
        let turn_rate = rate(class.turn_rate, 0);

        let flight = class.flight();

        Walker {
            at: aloft(on_ground(heights, at), flight),
            facing,
            nodes: vec![],
            index: 0,
            keep: REPATH_LENGTH,
            plans: 0,
            cell: (at.0 >> 4, at.1 >> 4),
            first: true,
            path: Some(Timed {
                order: (),
                when: now,
                delta: PATH_MS,
            }),
            top: None,
            wants: None,
            waits: 0,
            walkers: false,
            after_wait: false,
            step: step_length(class.move_dist, move_rate, Finish::Exact),
            move_rate,
            class_rate: class.move_rate,
            rate_modifier: 0,
            move_dist: class.move_dist,
            turn_rate,
            turn_step: turn_step(class.turn_step, turn_rate),
            flags: class.flags,
            flight,
            stage: None,
            gait: Gait::Walk,
            sets: action::Sets { set: 0, after: 0 },
            left: None,
            laid: None,
            played: vec![],
            movement: Movement::Moves,
            phase: action::Phase::default(),
            parts: Parts::Other,
            walk: None,
            stand: None,
            end: None,
            shown: None,
        }
    }

    /// A unit with no movement, a building, at `at` facing `facing`: its
    /// track 2 alone runs. `GE_OrderProcessor` pushes
    /// no Turn without a movement, so action face sets the facing at
    /// once and plays the action's set.
    #[must_use]
    pub fn fixed(heights: &dyn Heights, at: (i32, i32), facing: i32) -> Walker {
        let still = Class {
            turn_rate: 0,
            turn_step: 0,
            move_rate: 0,
            move_dist: 0,
            size: 0,
            lift: (0, 0),
            flags: 0,
        };

        Walker {
            path: None,
            movement: Movement::Fixed,
            ..Walker::new(heights, at, facing, &still, 0)
        }
    }

    /// Whether it has no movement: [`Walker::fixed`].
    #[must_use]
    pub const fn is_fixed(&self) -> bool {
        matches!(self.movement, Movement::Fixed)
    }

    /// A walker standing at `at`, facing `facing`, track 0 empty.
    #[must_use]
    pub fn standing(heights: &dyn Heights, at: (i32, i32), facing: i32, class: &Class) -> Walker {
        Walker {
            path: None,
            ..Walker::new(heights, at, facing, class, 0)
        }
    }

    /// What its parts show now: the phase an action's turn keeps.
    #[must_use]
    pub const fn phase(&self) -> action::Phase {
        self.phase
    }

    /// The unit's `MovementRateModifier`, `APV:34`: `GSUnit` adds it
    /// to the class's rate. A `Move` takes the rate and its step as
    /// `GE_OrderProcessor` makes it, so a change takes
    /// with the next `Move`; the walker's pace reads it at once.
    pub const fn modify_rate(&mut self, modifier: i32) {
        self.rate_modifier = modifier;
    }

    /// A new `Move`'s rate and step: `GSUnit`, then
    /// move step length, as `GE_OrderProcessor` reads them.
    fn arm_move(&mut self) {
        self.move_rate = rate(self.class_rate, self.rate_modifier);
        self.step = step_length(self.move_dist, self.move_rate, Finish::Exact);
    }

    /// A `Move`'s step: move step length caps it at [`STEP_CAP`] only for
    /// info `0x4000`, a path's [`Finish::Overshoot`].
    #[must_use]
    pub fn step_of(&self, finish: Finish) -> Fixed {
        match finish {
            Finish::Overshoot => self.step.clamp(-STEP_CAP, STEP_CAP),
            Finish::Exact => self.step,
        }
    }

    /// Push a new `MovePath` at game time `now`: `GE_OrderProcessor`,
    /// a fresh plan and info.
    pub fn push_path(&mut self, now: u32) {
        self.nodes.clear();
        self.index = 0;
        self.keep = REPATH_LENGTH;
        self.plans = 0;
        self.first = true;
        // The wait record stays: only a path's run clears it, as it steps
        // or ends (`OrderMovePath`, the sole writer but the save).
        self.waits = 0;
        self.walkers = false;
        self.after_wait = false;
        self.path = Some(Timed {
            order: (),
            when: now,
            delta: PATH_MS,
        });

        // A frameless class plays `Walk` from the frame its parts show:
        // a looping `Animate` on
        // track 1, which a stop takes off.
        let walk = self.walk.as_deref().cloned();

        if let Some(walk) = walk.filter(|_| self.flags & FRAMELESS != 0) {
            self.played.push(Track1::Plays(WALK));
            self.play(&walk, self.phase.frame_at(now), now, Parts::Walk);
        }
    }

    /// Put the walker at `at`, a GPL coordinate, on its half cell: a move
    /// by unit move to, as `UnHide` does.
    pub fn put(&mut self, heights: &dyn Heights, at: (i32, i32)) {
        self.at = aloft(on_ground(heights, at), self.flight);
        self.cell = (at.0 >> 4, at.1 >> 4);
    }

    /// Cells a ms: a step's cells over the move rate (unit pace:
    /// `fixed_div(move_dist / 32, x)`, `x` from a `GSUnit` method: a speed, not a time).
    /// **Game**: play5's ratman waits 1000 ms for the faster champion in
    /// its way.
    #[must_use]
    pub const fn pace(&self) -> Fixed {
        let cells = div(from_whole(self.move_dist.cast_signed()), PER_CELL_FIXED);
        let rate = rate(self.class_rate, self.rate_modifier);

        div(cells, from_whole(rate.cast_signed()))
    }

    /// Game ms each `Walk` frame shows: a step puts the walker in set 1
    /// and moves its frame on by one (move set position,
    /// move set position), one step a move rate. `None` for a [`FRAMELESS`]
    /// class: its steps leave the frame, and an `Animate` plays `Walk` at
    /// the set's rate instead.
    #[must_use]
    pub const fn stride_ms(&self) -> Option<u32> {
        if self.flags & FRAMELESS != 0 {
            None
        } else {
            Some(interval(self.move_rate))
        }
    }

    /// The `Walk` frame its parts show at game time `now`: a step moves it
    /// on (`Walker::step_frame`), a frameless class's `Animate` loops it.
    /// `None` without its art's `Walk`.
    #[must_use]
    pub const fn walk_frame(&self, now: u32) -> Option<u32> {
        if self.walk.is_some() {
            Some(self.phase.frame_at(now))
        } else {
            None
        }
    }

    /// As another walker's wait weighs this one.
    #[must_use]
    pub const fn waiter(&self) -> Waiter {
        Waiter {
            wants: self.wants,
            pace: self.pace(),
            moving: self.moving(),
        }
    }

    /// Whether track 0 holds anything: `IsMoving` (gpl is moving,
    /// unit is moving).
    #[must_use]
    pub const fn moving(&self) -> bool {
        self.path.is_some() || self.top.is_some()
    }

    /// The set its own orders last played.
    #[must_use]
    pub const fn gait(&self) -> Gait {
        self.gait
    }

    /// What track 2 shows: its `InitAction`'s frame, else what the last
    /// left; `None` where its own orders' [`Gait`] shows.
    #[must_use]
    pub fn look(&self) -> Option<action::Look> {
        let staged = self.stage.as_ref().and_then(action::Stage::frame);

        match (self.laid, staged) {
            (Some(set), _) => Some(action::Look::Frame { set, frame: 0 }),
            (None, Some(frame)) => Some(action::Look::Frame {
                set: self.sets.set,
                frame,
            }),
            (None, None) => self.left,
        }
    }

    /// The next action plays `sets`.
    pub const fn plays(&mut self, sets: action::Sets) {
        self.sets = sets;
        self.laid = None;
    }

    /// What the last action left showing is gone: a building's own set
    /// is on its parts.
    pub const fn forget(&mut self) {
        self.left = None;
    }

    /// The parts show `set` from now, whatever track 2 plays:
    /// `UnitIMG` method `(set, 0)`, as unit lie down puts `Dead` on a
    /// dying unit. Its one frame holds through the rest
    /// of the `InitAction`, whose after-set, 0, plays nothing.
    pub const fn lay(&mut self, set: u32) {
        self.laid = Some(set);
        self.parts = Parts::Other;
    }

    /// Whether track 2 holds an order: `GenUnit` method `(2)`.
    #[must_use]
    pub const fn acting(&self) -> bool {
        self.stage.is_some()
    }

    /// A flier's height, the unit's own z (unit pos in 3D): it
    /// flies at [`aloft`]'s height, not on the ground. `None` for a walker
    /// on the ground.
    #[must_use]
    pub const fn aloft(&self) -> Option<Fixed> {
        match self.flight {
            Some(_) => Some(self.at[2]),
            None => None,
        }
    }

    /// Whether track 2 holds the Action's GPL agent.
    #[must_use]
    pub const fn holds(&self) -> bool {
        matches!(self.stage, Some(action::Stage { held: true, .. }))
    }

    /// The Action ends: track 2's order, if any, takes its GPL agent
    /// (`OrderAction` mode 1, `OrderAction`).
    pub const fn hand_over(&mut self) {
        if let Some(stage) = self.stage.as_mut() {
            stage.held = true;
        }
    }

    /// An Action arms at game time `now`: action face toward
    /// `want`, or nothing to turn to (`None`), then `pose`'s `InitAction`
    /// ([`action`]), whose end does `settle` to the facing.
    pub fn act(
        &mut self,
        want: Option<i32>,
        pose: action::Pose,
        settle: action::Settle,
        now: u32,
    ) -> action::Peak {
        let turn = (self.turn_step, self.turn_rate);
        let frames = pose.frames_at(want.unwrap_or(self.facing));

        // No movement: no Turn, the facing set at once.
        let want = match (self.movement, want) {
            (Movement::Fixed, Some(w)) => {
                self.facing = w;
                None
            }
            _ => want,
        };
        let keeps = pose.keeps;
        let shown = self.shown.replace(Box::new(pose.clone()));
        let (mut ran, peak) =
            action::Stage::begin(&mut self.facing, want, pose, settle, now, turn, self.phase);

        // A frameless class's turn left on track 2 loops the parts' own
        // set from frame 0 through it.
        if let (action::Ran::On(stage), Some(own)) = (&mut ran, shown)
            && self.flags & FRAMELESS != 0
            && stage.turning()
        {
            self.phase =
                action::Phase::running(0, now, own.rate(), own.frames_at(self.facing), own.keeps);
            stage.restart(self.phase);
        }

        // The parts show the action's set, where the art
        // has it.
        self.phase.keeps = keeps;

        if frames > 0 {
            self.parts = Parts::Other;
        }

        self.ran(ran, frames, now);

        peak
    }

    /// Run track 2's top order once, in the frame at game time `now`,
    /// after tracks 0 and 1.
    pub fn run_stage(&mut self, now: u32) -> action::Peak {
        match self.stage.take_if(|s| s.due(now)) {
            Some(stage) => {
                let turn = (self.turn_step, self.turn_rate);
                let frames = stage.frames();
                let (ran, peak) = stage.run(&mut self.facing, now, turn);

                self.ran(ran, frames, now);

                peak
            }
            None => action::Peak::Not,
        }
    }

    /// Track 2 after a run: its order kept, or ended, leaving what its
    /// sets leave after `frames` at `now` ([`action::Sets::left`]).
    fn ran(&mut self, ran: action::Ran, frames: u32, now: u32) {
        self.stage = match ran {
            action::Ran::On(stage) => Some(stage),
            action::Ran::Ended => {
                self.left = self.sets.left(frames, now).or(self.left);
                self.played.push(match self.sets.after {
                    0 => Track1::Clear,
                    after => Track1::Plays(after),
                });
                // The parts end on its last frame; an after-set plays on
                // from it where the sets share a frame-keeping bit, else
                // from frame 0.
                let stand = self.stand.as_deref().cloned();

                match stand {
                    Some(stand) if self.sets.after == STAND => {
                        let frame = if stand.keeps & self.phase.keeps != 0 {
                            frames.saturating_sub(1)
                        } else {
                            0
                        };

                        self.play(&stand, frame, now, Parts::Stand);
                    }
                    _ => {
                        self.phase =
                            action::Phase::still(frames.saturating_sub(1), self.phase.keeps);
                    }
                }

                None
            }
        };
    }

    /// Its art's `Walk` and `Stand`: what a frameless class's path plays
    /// as it is pushed, and a
    /// spent path's `Stand`, each from the frame the parts show; a set
    /// that keeps the frame then starts on it ([`action::Phase`]). `None`
    /// for a set the art lacks: the parts keep theirs.
    pub fn loops(&mut self, walk: Option<action::Pose>, stand: Option<action::Pose>) {
        self.walk = walk.map(Box::new);
        self.stand = stand.map(Box::new);
    }

    /// The set `UnitIMG` put on its parts as it was made, held on
    /// frame 0 until an order plays another.
    pub fn made_in(&mut self, pose: action::Pose) {
        self.shown = Some(Box::new(pose));
    }

    /// Its art's [`PATH_END`]: a spent path plays it once on track 0
    /// instead of looping `Stand`.
    pub fn ends(&mut self, pose: action::Pose) {
        self.end = Some(Box::new(pose));
    }

    /// The parts start looping `pose`, which is `parts`, at game time
    /// `now` from `frame`.
    fn play(&mut self, pose: &action::Pose, frame: u32, now: u32, parts: Parts) {
        let frames = pose.frames_at(self.facing);

        self.shown = Some(Box::new(pose.clone()));
        self.parts = parts;

        self.phase = action::Phase::running(frame, now, pose.rate(), frames, pose.keeps);
    }

    /// A stop takes track 1's `Animate`s off: the parts
    /// hold the frame they show.
    pub const fn hold_frame(&mut self, now: u32) {
        self.phase = action::Phase::still(self.phase.frame_at(now), self.phase.keeps);
    }

    /// What its orders did to track 1's `Animate`s since last asked.
    pub fn take_played(&mut self) -> Vec<Track1> {
        std::mem::take(&mut self.played)
    }

    /// The half cell it holds: what distances and adjacency measure from.
    #[must_use]
    pub const fn cell(&self) -> (i32, i32) {
        self.cell
    }

    /// Every order off tracks 0 and 2: path, step, turn, wait and action,
    /// as `GenUnit` clears tracks 1, 0 and 2 for a teleport
    /// (**Inferred**: method per track not read further).
    pub fn clear(&mut self) {
        self.path = None;
        self.top = None;
        self.stage = None;
        self.wants = None;
        self.waits = 0;
    }

    /// Arrival: `FollowUnit`, finding the walker adjacent, takes the
    /// `MovePath` off track 0; a running step or turn
    /// finishes. `StopMoving` does not touch track 0 (`GenUnit`
    /// clears track 1 only).
    pub const fn arrive(&mut self) {
        self.path = None;
    }

    /// The half cell a walker in the way keeps it from.
    #[must_use]
    pub const fn wants(&self) -> Option<Cell> {
        self.wants
    }

    /// Run track 0's top order once, in the frame at game time `now`, alone
    /// on the map: nothing in its way moves.
    pub fn frame(&mut self, heights: &dyn Heights, now: u32, planner: &mut Planner<'_>) {
        self.among(heights, now, &mut Alone(planner));
    }

    /// Run track 0's top order once, in the frame at game time `now`, among
    /// the units `around` knows.
    pub fn among(&mut self, heights: &dyn Heights, now: u32, around: &mut dyn Around) {
        if let Some(top) = self.top {
            if top.due(now) {
                self.run_top(top.order, now);
            }
        } else if let Some(path) = self.path
            && path.due(now)
        {
            self.run_path(heights, now, around);
        }
    }

    fn run_top(&mut self, order: Top, now: u32) {
        match order {
            Top::Turn { want, to } => match turn(self.facing, want, self.turn_step) {
                Turned::Toward(d) => {
                    self.facing = d;
                    self.top = Some(Timed {
                        order,
                        when: now,
                        delta: self.turn_rate,
                    });
                }
                Turned::Onto(d) => {
                    // Callback 0x1007: a Move, info 0.
                    self.facing = d;
                    self.arm_move();
                    self.top = Some(Timed {
                        order: Top::Move {
                            to,
                            finish: Finish::Exact,
                        },
                        when: now,
                        delta: self.move_rate,
                    });
                }
            },
            Top::Move { to, finish } => {
                let (at, leg) = advance(self.at, to, self.step_of(finish), finish);

                self.at = at;
                self.step_frame(finish, now);
                self.top = match leg {
                    Leg::More => Some(Timed {
                        order,
                        when: now,
                        delta: self.move_rate,
                    }),
                    Leg::Done => None,
                };
            }
            Top::Wait => self.top = None,
            Top::End => self.run_end(now),
        }
    }

    /// A step at game time `now` puts `Walk` on the parts ([`Parts`]),
    /// then moves part 0's frame on, wrapping at the frames of the set
    /// the parts show, unless its `Move`'s info has `0x80`:
    /// a frameless class's path `Move` carries
    /// the path's, the `Move` after a `Turn` info 0.
    fn step_frame(&mut self, finish: Finish, now: u32) {
        let frameless = finish == Finish::Overshoot && self.flags & FRAMELESS != 0;
        let walk = self
            .walk
            .as_ref()
            .map(|w| (w.frames_at(self.facing), w.keeps));

        // Art without `Walk`: `UnitIMG` fails, the parts keep their
        // set, and the frame wraps at its count.
        let frames = match walk {
            Some((frames, keeps)) => {
                if self.parts != Parts::Walk {
                    self.phase = self.phase.put(frames, keeps, now);
                    self.parts = Parts::Walk;
                }

                frames
            }
            None => self.shown.as_ref().map_or(0, |s| s.frames_at(self.facing)),
        };

        if !frameless {
            self.phase = self.phase.stepped(frames);
        }

        // Without `Walk`, what an action left stays on the parts, its
        // frame stepped.
        if let (None, Some(action::Look::Frame { set, .. })) = (walk, self.left) {
            self.left = Some(action::Look::Frame {
                set,
                frame: self.phase.frame_at(now),
            });
        }
    }

    /// Push a [`Top::Wait`]: `GE_OrderProcessor` through
    /// order wait push, track 0, rate [`COLLISION_MS`].
    const fn wait(&mut self, now: u32) {
        self.top = Some(Timed {
            order: Top::Wait,
            when: now,
            delta: COLLISION_MS,
        });
    }

    /// `OrderMovePath`.
    ///
    /// ```text
    ///  track 2 busy                           ─> wait, next frame again
    ///  path spent   plan, walkers blocking under info 0x400
    ///               none: 0x400 and not at the goal ─> 0x2000, wait
    ///                     else the MovePath ends: its point off the
    ///                     map ─> a last Move there, else `Stand`
    ///  next cell    free                      ─> a turn or a step
    ///               a walker in it, wait_for  ─> wait, 4 in a row at most
    ///               else, 0x2000              ─> nudge, plan next frame
    ///               else                      ─> 0x400, plan next frame
    ///               anything else in it       ─> plan next frame
    /// ```
    fn run_path(&mut self, heights: &dyn Heights, now: u32, around: &mut dyn Around) {
        if self.acting() {
            // Track 2 busy: wait for the action's turn and InitAction.
            return;
        }

        if self.index + 1 >= self.nodes.len() {
            // After ExpandCount plans the kept length doubles:
            // the count goes up, then to 0 on doubling.
            let before = self.plans;

            self.plans = self.plans.wrapping_add(1);

            if before > EXPAND_COUNT && self.keep < PATH_HORIZON {
                self.plans = 0;
                self.keep *= 2;
            }

            let how = if self.walkers {
                Blocking::Any
            } else {
                Blocking::Still
            };

            self.nodes = around.plan(self.cell, self.keep, how);
            self.index = 0;

            if self.nodes.len() < 2 {
                // Hemmed in by walkers short of the goal: wait.
                if self.walkers && !around.at_goal(self.cell) {
                    self.walkers = false;
                    self.after_wait = true;
                    self.wait(now);
                } else if let Some(point) = around.beyond() {
                    // path final move: the MovePath
                    // ends at the edge, and a Move, info 0, takes the
                    // walker on to its point off the map; no `Stand`.
                    self.path = None;
                    self.wants = None;
                    self.arm_move();
                    self.top = Some(Timed {
                        order: Top::Move {
                            to: aloft(on_ground(heights, point), self.flight),
                            finish: Finish::Exact,
                        },
                        when: now,
                        delta: self.move_rate,
                    });
                } else {
                    // The MovePath ends: the art's `PATH_END` once on
                    // track 0, else `Stand` on track 1,
                    // from the parts' frame under info
                    // 0x80, else frame 0.
                    self.path = None;
                    self.wants = None;
                    self.gait = Gait::Stand;
                    self.played.push(Track1::PathEnd);

                    let end = self.end.as_deref().cloned();
                    let stand = self.stand.as_deref().cloned();

                    if let Some(end) = end {
                        self.play_end(&end, now);
                    } else if let Some(stand) = stand {
                        let frame = if self.flags & FRAMELESS == 0 {
                            0
                        } else {
                            self.phase.frame_at(now)
                        };

                        self.play(&stand, frame, now, Parts::Stand);
                    }
                }

                return;
            }

            self.walkers = false;
        }

        let node = self.nodes[self.index + 1];

        if around.fits(node, Blocking::Any) {
            self.step_to(heights, now, node);
        } else if around.fits(node, Blocking::Still) {
            self.blocked(now, node, around);
        } else {
            self.nodes.clear();
        }
    }

    /// A walker stands on `node`, nothing else.
    fn blocked(&mut self, now: u32, node: Cell, around: &mut dyn Around) {
        self.wants = Some(node);

        if around.wait(node) {
            let before = self.waits;

            self.waits = self.waits.wrapping_add(1);

            if before < WAIT_LIMIT {
                self.wait(now);
                self.stand(now);
                self.gait = Gait::Stand;

                // Art without `Stand` keeps what the parts show.
                if self.stand.is_some() {
                    self.left = None;
                    self.laid = None;
                }

                return;
            }
        }

        self.waits = 0;

        if self.after_wait {
            // path after wait reads at most five nodes.
            let ahead = self
                .nodes
                .get(self.index..self.nodes.len().min(NUDGE_NODES));

            self.after_wait = false;
            around.nudge(ahead.unwrap_or_default());
        } else {
            self.walkers = true;
        }

        self.nodes.clear();
    }

    /// A spent path where the art has [`PATH_END`]: `UnitIMG` puts
    /// it on the parts, then path end stop pushes an
    /// `Animate` of part 0 on track 0 from frame 0, flags `0x150`, no
    /// loops left (`GE_OrderProcessor`, `OrderAnimate` 0).
    /// Not run on push, `when` and `delta` 0: it first runs in the next
    /// frame. None for a one-frame slot, which keeps what method left.
    fn play_end(&mut self, end: &action::Pose, now: u32) {
        let frames = end.frames_at(self.facing);
        let animates = frames > 1;

        // The Animate's frame argument, 0, sets the frame method left.
        let kept = if animates { 0 } else { end.keeps };

        self.phase = self.phase.put(frames, kept, now);
        self.phase.keeps = end.keeps;
        self.parts = Parts::Other;
        self.shown = Some(Box::new(end.clone()));
        self.end_frame(now);

        if animates {
            self.top = Some(Timed {
                order: Top::End,
                when: 0,
                delta: 0,
            });
        }
    }

    /// A run of track 0's [`PATH_END`] `Animate`:
    /// part 0's frame on by one; out of loops, the run past the last frame
    /// ends it, the frame held. Before track 1's runs.
    fn run_end(&mut self, now: u32) {
        let (frames, rate) = self
            .end
            .as_ref()
            .map_or((0, 0), |e| (e.frames_at(self.facing), e.rate()));

        if self.phase.frame_before(now) + 1 < frames {
            self.phase = self.phase.stepped(frames);
            self.top = Some(Timed {
                order: Top::End,
                when: now,
                delta: rate,
            });
        } else {
            self.top = None;
        }

        self.end_frame(now);
    }

    /// What the parts show: [`PATH_END`]'s frame at game time `now`.
    const fn end_frame(&mut self, now: u32) {
        self.left = Some(action::Look::Frame {
            set: PATH_END,
            frame: self.phase.frame_at(now),
        });
    }

    /// A wait puts `Stand` on the parts, no `Animate` (`UnitIMG` method `(8)`
    /// in `OrderMovePath`); on `Stand` already, nothing changes.
    fn stand(&mut self, now: u32) {
        let stand = self
            .stand
            .as_ref()
            .map(|s| (s.frames_at(self.facing), s.keeps));

        if let Some((frames, keeps)) = stand.filter(|_| self.parts != Parts::Stand) {
            self.phase = self.phase.put(frames, keeps, now);
            self.parts = Parts::Stand;
        }
    }

    /// The direction of the leg from `from` to `node`:
    /// with [`KEEPS_FACING`], where it turns from
    /// the facing but the leg after turns back to it, the facing stays.
    fn leg(&self, from: Cell, node: Cell) -> i32 {
        let want = direction(node.0 - from.0, node.1 - from.1);
        let after = self
            .nodes
            .get(self.index + 2)
            .map(|n| direction(n.0 - node.0, n.1 - node.1));

        match after {
            Some(after)
                if self.flags & KEEPS_FACING != 0
                    && want != self.facing
                    && after != want
                    && after == self.facing =>
            {
                self.facing
            }
            _ => want,
        }
    }

    /// Claim `node` and push the turn or step toward it.
    fn step_to(&mut self, heights: &dyn Heights, now: u32, node: Cell) {
        let from = self.nodes[self.index];
        let want = self.leg(from, node);
        let to = aloft(on_ground(heights, (node.0 << 4, node.1 << 4)), self.flight);
        let first = self.first;

        self.first = false;
        self.index += 1;
        self.cell = node;
        self.wants = None;
        self.gait = Gait::Walk;

        // Art without `Walk` keeps what the parts show.
        if self.walk.is_some() {
            self.left = None;
            self.laid = None;
        }

        if want == self.facing {
            // `GE_OrderProcessor` in `GE_OrderProcessor`: at once when FIRST,
            // else a frame short of the rate.
            self.arm_move();

            let delta = if first {
                0
            } else {
                self.move_rate.saturating_sub(MS_PER_FRAME)
            };

            self.top = Some(Timed {
                order: Top::Move {
                    to,
                    finish: Finish::Overshoot,
                },
                when: now,
                delta,
            });
        } else {
            self.top = Some(Timed {
                order: Top::Turn { want, to },
                when: now,
                delta: 0,
            });
        }

        // Pushed with bit 2 or a zero delta: runs as it is pushed.
        if let Some(top) = self.top
            && top.delta == 0
        {
            self.run_top(top.order, now);
        }
    }
}

#[cfg(test)]
mod tests;

/// To and from the save model.
mod snap {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Finish {
        type Model = m::Finish;

        fn save(&self) -> m::Finish {
            match self {
                Finish::Exact => m::Finish::Exact,
                Finish::Overshoot => m::Finish::Overshoot,
            }
        }

        fn load(model: m::Finish) -> Result<Finish, Refused> {
            Ok(match model {
                m::Finish::Exact => Finish::Exact,
                m::Finish::Overshoot => Finish::Overshoot,
            })
        }
    }

    impl Saved for Top {
        type Model = m::Step;

        fn save(&self) -> m::Step {
            match *self {
                Top::Turn { want, to } => m::Step::Turn {
                    want,
                    to: to.save(),
                },
                Top::Move { to, finish } => m::Step::Move {
                    to: to.save(),
                    finish: finish.save(),
                },
                Top::Wait => m::Step::Wait,
                Top::End => m::Step::End,
            }
        }

        fn load(model: m::Step) -> Result<Top, Refused> {
            Ok(match model {
                m::Step::Turn { want, to } => Top::Turn {
                    want,
                    to: <[Fixed; 3]>::load(to)?,
                },
                m::Step::Move { to, finish } => Top::Move {
                    to: <[Fixed; 3]>::load(to)?,
                    finish: Finish::load(finish)?,
                },
                m::Step::Wait => Top::Wait,
                m::Step::End => Top::End,
            })
        }
    }

    impl Saved for Parts {
        type Model = m::Parts;

        fn save(&self) -> m::Parts {
            match self {
                Parts::Walk => m::Parts::Walk,
                Parts::Stand => m::Parts::Stand,
                Parts::Other => m::Parts::Other,
            }
        }

        fn load(model: m::Parts) -> Result<Parts, Refused> {
            Ok(match model {
                m::Parts::Walk => Parts::Walk,
                m::Parts::Stand => Parts::Stand,
                m::Parts::Other => Parts::Other,
            })
        }
    }

    impl Saved for Gait {
        type Model = m::Gait;

        fn save(&self) -> m::Gait {
            match self {
                Gait::Walk => m::Gait::Walk,
                Gait::Stand => m::Gait::Stand,
            }
        }

        fn load(model: m::Gait) -> Result<Gait, Refused> {
            Ok(match model {
                m::Gait::Walk => Gait::Walk,
                m::Gait::Stand => Gait::Stand,
            })
        }
    }

    impl Saved for Track1 {
        type Model = m::Track1;

        fn save(&self) -> m::Track1 {
            match *self {
                Track1::Plays(set) => m::Track1::Plays(set),
                Track1::PathEnd => m::Track1::PathEnd,
                Track1::Clear => m::Track1::Clear,
            }
        }

        fn load(model: m::Track1) -> Result<Track1, Refused> {
            Ok(match model {
                m::Track1::Plays(set) => Track1::Plays(set),
                m::Track1::PathEnd => Track1::PathEnd,
                m::Track1::Clear => Track1::Clear,
            })
        }
    }

    impl Saved for Movement {
        type Model = m::Movement;

        fn save(&self) -> m::Movement {
            match self {
                Movement::Moves => m::Movement::Moves,
                Movement::Fixed => m::Movement::Fixed,
            }
        }

        fn load(model: m::Movement) -> Result<Movement, Refused> {
            Ok(match model {
                m::Movement::Moves => Movement::Moves,
                m::Movement::Fixed => Movement::Fixed,
            })
        }
    }

    impl Saved for Walker {
        type Model = m::Walker;

        fn save(&self) -> m::Walker {
            let Walker {
                at,
                facing,
                nodes,
                index,
                keep,
                plans,
                cell,
                first,
                path,
                top,
                wants,
                waits,
                walkers,
                after_wait,
                step,
                move_rate,
                class_rate,
                rate_modifier,
                move_dist,
                turn_rate,
                turn_step,
                flags,
                flight,
                stage,
                gait,
                sets,
                left,
                laid,
                played,
                movement,
                phase,
                parts,
                walk,
                stand,
                end,
                shown,
            } = self;

            m::Walker {
                at: at.save(),
                facing: *facing,
                nodes: nodes.clone(),
                index: index.save(),
                keep: keep.save(),
                plans: *plans,
                cell: *cell,
                first: *first,
                path: path.map(|p| (p.when, p.delta)),
                top: top.map(|t| (t.order.save(), t.when, t.delta)),
                wants: *wants,
                waits: *waits,
                walkers: *walkers,
                after_wait: *after_wait,
                step: step.save(),
                move_rate: *move_rate,
                class_rate: *class_rate,
                rate_modifier: *rate_modifier,
                move_dist: *move_dist,
                turn_rate: *turn_rate,
                turn_step: *turn_step,
                flags: *flags,
                flight: flight.save(),
                stage: stage.save(),
                gait: gait.save(),
                sets: sets.save(),
                left: left.save(),
                laid: *laid,
                played: played.save(),
                movement: movement.save(),
                phase: phase.save(),
                parts: parts.save(),
                walk: walk.save(),
                stand: stand.save(),
                end: end.save(),
                shown: shown.save(),
            }
        }

        fn load(model: m::Walker) -> Result<Walker, Refused> {
            Ok(Walker {
                at: <[Fixed; 3]>::load(model.at)?,
                facing: model.facing,
                nodes: model.nodes,
                index: usize::load(model.index)?,
                keep: usize::load(model.keep)?,
                plans: model.plans,
                cell: model.cell,
                first: model.first,
                path: model.path.map(|(when, delta)| Timed {
                    order: (),
                    when,
                    delta,
                }),
                top: match model.top {
                    Some((t, when, delta)) => Some(Timed {
                        order: Top::load(t)?,
                        when,
                        delta,
                    }),
                    None => None,
                },
                wants: model.wants,
                waits: model.waits,
                walkers: model.walkers,
                after_wait: model.after_wait,
                step: Fixed::load(model.step)?,
                move_rate: model.move_rate,
                class_rate: model.class_rate,
                rate_modifier: model.rate_modifier,
                move_dist: model.move_dist,
                turn_rate: model.turn_rate,
                turn_step: model.turn_step,
                flags: model.flags,
                flight: Option::load(model.flight)?,
                stage: Option::load(model.stage)?,
                gait: Gait::load(model.gait)?,
                sets: action::Sets::load(model.sets)?,
                left: Option::load(model.left)?,
                laid: model.laid,
                played: Vec::load(model.played)?,
                movement: Movement::load(model.movement)?,
                phase: action::Phase::load(model.phase)?,
                parts: Parts::load(model.parts)?,
                walk: Option::load(model.walk)?,
                stand: Option::load(model.stand)?,
                end: Option::load(model.end)?,
                shown: Option::load(model.shown)?,
            })
        }
    }
}
