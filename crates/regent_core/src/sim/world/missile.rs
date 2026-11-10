//! Missiles: where an action with a projectile aims, its launch, its
//! flight and its impact. **Binary** unless marked.
//!
//! ```text
//!  PerformAction(actor, action, target)   the Action arms
//!    the action turns: a projectile at a unit turns toward
//!      the target's aim point for the projectile (IsoGenUnit)
//!        = unit_pos + the target's offset (IsoGenUnit)
//!          offset: attachment point 2, else set 8 block 1's
//!                  second point, about the unit, in 3D
//!          a building, projectile without NoRandomTargetOnDestination:
//!                  x + draw(w / 2) - w / 4, then height + draw(h / 2) - h / 4
//!                  (not below 0); w, h the pad's ground pixels: two GPL
//!                  RNG draws
//!  the InitAction's cue or last frame (the peak)
//!    target unit gone ─> nothing: no missile, no function
//!    projectile ─> GenUnit makes a unit of kind 1 through
//!      GE_Level, so the next handle; no GPL agent (HEAD subtype 1),
//!      so no builtin sees it; it starts at the
//!      shooter's launch point (point 4 of its Cast or Attack slot)
//!        PRIM word 7 bit 2 clear, every HD projectile (homing):
//!          at a unit   GE_OrderProcessor: MoveMissile,
//!                      the target's offset again (two more draws),
//!                      info 0, rate and step its DMOV's, callback 0x100a
//!          at a point  GE_OrderProcessor: the point on the ground,
//!                      straight (World::at_point)
//!        bit 2 set, the catapult's ball: GE_OrderProcessor, the target's
//!               aim point fixes the point (a point on the ground);
//!               it lobs the ball there (World::lobbed, Lob), else
//!               straight, following no one (World::straight)
//!    an overlay, then the action's function (log: dispatched)
//!  each run of the MoveMissile (OrderMove)
//!    goal = target unit_pos + the offset, while the target stands
//!      (OrderMoveMissile); else the last goal
//!    farther than a step ─> a step in 3D, again after its rate
//!    else ─> on the goal; callback 0x100a, the impact hook
//!  impact: shooter and target by handle, the missile's point
//!    no function ─> nothing
//!    make_attack at a target with HasEffectMagicMirror ─> back at the
//!      shooter, homing again, or nothing
//!    else ─> the missile dies (set 0x60), then the script is dispatched:
//!      the projectile's function, by its Script type:
//!        0  (shooter, target)   1  (shooter, point)
//!        2  (target)            3  (point)
//!      an agent gone, nothing
//! ```
//!
//! **Game**, play5: a ratman shaman's `Infectious_Cloud` at an inn takes
//! two draws as it turns and two as it launches: the logged draws after
//! each come out (`tests/missile.rs`). The world dispatches the log's 37
//! followed impacts, the guardhouse's two among them; where it places
//! shooter and target as the probe does, each lands in the log's frame
//! (`tests/trace.rs`). Not followed: the overlay's effector.

use super::{Answer, Found, World, agent_ref, bodies::Class, string_ref};
use crate::base::fixed::{Fixed, div, from_whole, mul, whole};
use crate::data::attrib::HAS_EFFECT_MAGIC_MIRROR;
use crate::data::fourcc::FourCc;
use crate::data::imag;
use crate::data::settings;
use crate::gpl::ops::Value;
use crate::gpl::runtime::MS_PER_FRAME;
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::movement::{self, Finish, Leg};
use crate::sim::space::{self};
use crate::sim::trig;
use crate::sim::units::CAMERA_COS;
use crate::sim::units::from_pos32;

/// The attachment point method asks for: `2` in `IsoGenUnit` and
/// `GenUnit`, one-based ([`imag::attachment_point`]).
pub const HIT_ATTACHMENT: u32 = 2;

/// The set method falls back to without a `Hotspot`: `8` at
/// `IsoGenUnit`, with block 1 and its second pair (`IsoGenUnit`,
/// `IsoGenUnit`): point 3.
pub const HIT_SET: u32 = crate::data::imag::SetKind::Stand.id();

/// Point 3: block 1's second pair.
const HIT_POINT: usize = 3;

/// The sets `GenUnit` launches from: the shooter's own when it is
/// `Cast` or `Attack`, else `Attack` (compares with `0x80` and `0x10` in
/// `GenUnit`).
pub const ATTACK_SET: u32 = crate::data::imag::SetKind::Attack.id();
pub const CAST_SET: u32 = crate::data::imag::SetKind::Cast.id();

/// The launch point: block 2's first pair, point 4 (`GenUnit`,
/// `1`, `2` in `GenUnit`, into `IsoGenUnit`).
pub const LAUNCH_POINT: usize = 4;

/// The impact hook's one named function: `"make_attack"` at
/// missile impact.
pub const MAKE_ATTACK: &str = "make_attack";

/// A projectile's `Script type` bit 0: the point stands for the target.
pub const POINT_FORM: u32 = 1;

/// `Script type` bit 1: no shooter argument.
pub const NO_CALLER: u32 = 2;

/// Art pixels of height as 16.16 cells, as `IsoGenUnit`, method and
/// method turn them: `px × 32 / CAMERA_COS` floored, over 32.
///
/// ```
/// use regent_core::sim::units::from_pos32;
/// use regent_core::sim::world::missile::rise;
///
/// assert_eq!(rise(10), from_pos32(17)); // 17.37 floored, in 32nds
/// assert_eq!(rise(-10), from_pos32(-18));
/// ```
#[must_use]
pub const fn rise(px: i32) -> Fixed {
    let floored = whole(div(mul(from_whole(px), from_whole(32)), CAMERA_COS));

    div(from_whole(floored), from_whole(32))
}

/// 32nds of a cell as 16.16.
const fn cells(n: i32) -> Fixed {
    from_pos32(n)
}

/// Where on a unit a missile aims, about its position, from its art.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Hit {
    /// `Hotspot` slot 0: x of each
    /// attachment point, points 0 to 2; y [`imag::ATTACHMENT_BASE`]'s
    /// unprojected; each point's height over the base, art pixels.
    Attached {
        xs: [i32; 3],
        y: i32,
        rises: [i32; 3],
    },
    /// Set [`HIT_SET`]'s point 3, x and art y, per slot; the unit's drawn
    /// slot picks one. Its y is height (method moves z, not y).
    Standing(Vec<(i32, i32)>),
    /// Neither set: the unit's position.
    #[default]
    Centre,
}

impl Missile {
    /// The same projectile with spread `spread`.
    #[must_use]
    pub const fn with(self, spread: Spread) -> Missile {
        Missile { spread, ..self }
    }
}

impl Hit {
    /// A unit's, from its art.
    #[must_use]
    pub fn of(image: &imag::Image<'_>) -> Hit {
        let set = |id: u32| image.sets.iter().find(|s| s.id == id);

        match (set(imag::ATTACHMENT_SET), set(HIT_SET)) {
            (Some(hotspot), _) => {
                let points = hotspot
                    .slots
                    .first()
                    .map(imag::Slot::points)
                    .unwrap_or_default();
                let (_, base) = points[imag::ATTACHMENT_BASE];

                Hit::Attached {
                    xs: [0, 1, 2].map(|p| i32::from(points[p].0)),
                    y: crate::sim::placement::unproject(0, i32::from(base)).1,
                    rises: [0, 1, 2].map(|p| i32::from(base) - i32::from(points[p].1)),
                }
            }
            (None, Some(stand)) => Hit::Standing(
                stand
                    .slots
                    .iter()
                    .map(|s| s.points()[HIT_POINT])
                    .map(|(x, y)| (i32::from(x), i32::from(y)))
                    .collect(),
            ),
            (None, None) => Hit::Centre,
        }
    }

    /// The ground offset for a unit facing `facing`, 32nds of a cell.
    /// **Inferred**: the engine reads the slot the unit's part draws
    /// (`UnitIMGPartEntry`), taken as its facing's slot of set
    /// [`HIT_SET`].
    #[must_use]
    pub fn offset(&self, facing: i32) -> (i32, i32) {
        match self {
            Hit::Attached { .. } => self.attached(HIT_ATTACHMENT).unwrap_or_default(),
            Hit::Standing(points) => (standing(points, facing).0, 0),
            Hit::Centre => (0, 0),
        }
    }

    /// [`Hit::offset`] with its height: 16.16 cells, x, y and z.
    #[must_use]
    pub fn offset3(&self, facing: i32) -> [Fixed; 3] {
        let (x, y) = self.offset(facing);
        let z = match self {
            Hit::Attached { rises, .. } => imag::attachment_point(HIT_ATTACHMENT)
                .and_then(|p| rises.get(p))
                .map_or(Fixed::ZERO, |r| rise(*r)),
            Hit::Standing(points) => -rise(standing(points, facing).1),
            Hit::Centre => Fixed::ZERO,
        };

        [cells(x), cells(y), z]
    }

    /// The point of `AttachmentPointID` `id` about the unit in 3D, 16.16
    /// cells: `IsoGenUnit`'s, else [`Hit::offset3`] (**Inferred**:
    /// `GenUnit`'s set `0xd0` or 8 fallback not read).
    #[must_use]
    pub fn offset3_at(&self, id: u32, facing: i32) -> [Fixed; 3] {
        match (self, self.attached(id.max(1))) {
            (Hit::Attached { rises, .. }, Some((x, y))) => {
                let z = imag::attachment_point(id.max(1))
                    .and_then(|p| rises.get(p))
                    .map_or(Fixed::ZERO, |r| rise(*r));

                [cells(x), cells(y), z]
            }
            _ => self.offset3(facing),
        }
    }

    /// `IsoGenUnit` for `AttachmentPointID` `id`: the point about
    /// the unit, 32nds of a cell; `None` without a `Hotspot` or for an id
    /// it refuses ([`imag::attachment_point`]).
    #[must_use]
    pub fn attached(&self, id: u32) -> Option<(i32, i32)> {
        match self {
            Hit::Attached { xs, y, .. } => Some((*xs.get(imag::attachment_point(id)?)?, *y)),
            _ => None,
        }
    }
}

/// The point of `points` a unit facing `facing` draws.
fn standing(points: &[(i32, i32)], facing: i32) -> (i32, i32) {
    let slot = crate::sim::movement::action::slot(facing, points.len());

    points.get(slot).copied().unwrap_or_default()
}

/// Where a shooter's missile leaves it: [`LAUNCH_POINT`] of each slot of
/// [`ATTACK_SET`] and [`CAST_SET`], x and art y, by set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch(Vec<(u32, Vec<(i32, i32)>)>);

impl Launch {
    /// A unit's, from its art.
    #[must_use]
    pub fn of(image: &imag::Image<'_>) -> Launch {
        Launch(
            [ATTACK_SET, CAST_SET]
                .into_iter()
                .filter_map(|id| image.sets.iter().find(|s| s.id == id))
                .map(|s| {
                    let points = s
                        .slots
                        .iter()
                        .map(|slot| slot.points()[LAUNCH_POINT])
                        .map(|(x, y)| (i32::from(x), i32::from(y)))
                        .collect();

                    (s.id, points)
                })
                .collect(),
        )
    }

    /// Slots `set` has, where the art has it.
    #[must_use]
    pub fn slots(&self, set: u32) -> Option<usize> {
        self.0
            .iter()
            .find(|(id, _)| *id == set)
            .map(|(_, points)| points.len())
    }

    /// The launch point of slot `slot` of `set`.
    #[must_use]
    pub fn offset_in(&self, set: u32, slot: usize) -> [Fixed; 3] {
        let (x, y) = self
            .0
            .iter()
            .find(|(id, _)| *id == set)
            .and_then(|(_, points)| points.get(slot).copied())
            .unwrap_or_default();

        [cells(x), Fixed::ZERO, -rise(y)]
    }
}

/// Whether a projectile's point on a building is scattered: `Info` bit 18,
/// `NoRandomTargetOnDestination`, fixes it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spread {
    #[default]
    Scattered,
    Fixed,
}

/// An action's projectile: its descriptor and spread, and the set its
/// shooter launches it from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Missile {
    pub code: FourCc,
    pub spread: Spread,
    pub set: u32,
}

impl Missile {
    /// A projectile descriptor with `PRIM` `info`, launched from
    /// [`ATTACK_SET`].
    #[must_use]
    pub const fn of(code: FourCc, info: u32) -> Missile {
        let spread =
            if info & (1 << crate::data::desc::Info::NoRandomTargetOnDestination as u32) == 0 {
                Spread::Scattered
            } else {
                Spread::Fixed
            };

        Missile {
            code,
            spread,
            set: ATTACK_SET,
        }
    }

    /// Launched by a shooter playing `set`: [`CAST_SET`] or [`ATTACK_SET`]
    /// stand, any other is [`ATTACK_SET`].
    #[must_use]
    pub const fn from(self, set: u32) -> Missile {
        let set = if set == CAST_SET {
            CAST_SET
        } else {
            ATTACK_SET
        };

        Missile { set, ..self }
    }
}

/// Script dispatch's arguments for a function of `Script type`
/// `kind`: the actor and the target, or a point, as its bits ask; `None`
/// where an agent it needs is gone, and nothing runs.
///
/// | Bit 1 ([`NO_CALLER`]) | Bit 0 ([`POINT_FORM`]) | Arguments |
/// |---|---|---|
/// | 0 | 0 | actor, target |
/// | 0 | 1 | actor, point |
/// | 1 | 0 | target |
/// | 1 | 1 | point |
pub fn script_args(
    kind: u32,
    actor: Option<u32>,
    target: Option<u32>,
    point: (i32, i32),
) -> Option<Vec<Value>> {
    let point = Value::Coord(point.0, point.1);

    match (kind & NO_CALLER, kind & POINT_FORM) {
        (0, 0) => actor
            .zip(target)
            .map(|(s, t)| vec![Value::Agent(s), Value::Agent(t)]),
        (0, _) => actor.map(|s| vec![Value::Agent(s), point]),
        (_, 0) => target.map(|t| vec![Value::Agent(t)]),
        _ => Some(vec![point]),
    }
}

/// A projectile's GPL function, `PRIM`'s callback: its name and its
/// `Script type` ([`POINT_FORM`], [`NO_CALLER`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    pub function: String,
    pub kind: u32,
}

/// What an armed action will launch at its `InitAction`'s cue or last
/// frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shot {
    pub missile: Missile,
    pub target: Target,
}

/// What a missile flies at: `GenUnit`'s target unit, else its
/// point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A unit, by agent: gone, no missile.
    Unit(u32),
    /// A GPL coordinate, 32nds, which no death cancels.
    Point((i32, i32)),
}

/// How `GenUnit` flies a projectile: by its class's `PRIM` word 7
/// bit 2 ([`movement::Class::lobbed`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flies {
    /// missile homing: method at a unit, method at a point.
    Homing,
    /// method at a unit, method at a point.
    Lobbed,
}

/// A missile in flight: `OrderMoveMissile`'s words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flight {
    /// The missile's unit handle: its place in ORDR.
    pub(super) handle: u32,
    missile: Missile,
    /// The actor's agent,.
    shooter: u32,
    /// The target's agent,; `None` for a flight to a fixed point,
    /// which follows no one.
    target: Option<u32>,
    /// About the target,; fixed at launch.
    offset: [Fixed; 3],
    pub(super) at: [Fixed; 3],
    /// .
    goal: [Fixed; 3],
    /// .
    step: Fixed,
    /// Ms.
    rate: u32,
    when: u32,
    /// Game ms of its launch.
    launched: u32,
    /// A lobbed flight's `OrderPhysicsMissile`; `None` homing.
    lob: Option<Lob>,
    /// Its direction out of 32, set as it is made ([`World::facing_at`]).
    facing: i32,
}

impl Flight {
    /// Its projectile's descriptor.
    pub(super) const fn code(&self) -> FourCc {
        self.missile.code
    }

    /// Its direction out of 32.
    pub(super) const fn facing(&self) -> i32 {
        self.facing
    }
}

/// A lobbed missile's `OrderPhysicsMissile` (order 15, callback `0x100f`),
/// as `GE_OrderProcessor` arms it: a velocity, 16.16
/// cells a run, and gravity on its height; the point it falls on is fixed
/// at launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lob {
    /// Cells a run, by axis.
    pub velocity: [Fixed; 3],
    /// Added to the height's velocity each run.
    pub gravity: Fixed,
    /// The aimed point's height.
    pub floor: Fixed,
    ///  bit 0: still falling when it reaches the aimed point, so it
    /// ends coming down to its height, not going up to it.
    pub falling: bool,
}

/// method's steepest shot, over `tan` of the angle: `str2fixed("0.9")`
/// (`"0.9"`). **Inferred**: `str2fixed` truncates, as 9/10 of 2^16.
pub const LOB_STEEPEST: Fixed = Fixed::from_bits(0xe666);

/// Sin and cos entry 53: the index
/// [`settings::PARABOLA_ANGLE`]'s 75° gives through method's
/// `degrees × π / 180 × 256 / 2π` ([`lob_index`]).
pub const LOB_SIN: Fixed = trig::sine(53);
pub const LOB_COS: Fixed = trig::cosine(53);

/// method's trig table index for `degrees`: `float_to_fixed`, times π
/// (`0x3243f`) over 180 (`0xb40000`), times `0x28be60` (256 over 2π),
/// whole part, folded into the 256 entries.
///
/// ```
/// use regent_core::sim::world::missile::lob_index;
///
/// assert_eq!(lob_index(75), 53);
/// assert_eq!(lob_index(60), 42);
/// ```
#[must_use]
pub const fn lob_index(degrees: i32) -> i32 {
    let radians = div(
        mul(from_whole(degrees), Fixed::from_bits(0x3243f)),
        from_whole(180),
    );

    whole(mul(radians, trig::STEPS_PER_TURN)) & 0xff
}

impl Lob {
    /// method from `start` to `aim`, 16.16 cells: the launch angle
    /// [`settings::PARABOLA_ANGLE`] and gravity `settings::PARABOLA_GRAVITY`
    /// over 32, HD's (the expansion's `GOPT`). `None` where method refuses,
    /// straight down or steeper than [`LOB_STEEPEST`] times the angle's
    /// tangent: method then flies method, straight to the point.
    #[must_use]
    pub fn of(start: [Fixed; 3], aim: [Fixed; 3]) -> Option<Lob> {
        let [dx, dy, dz] = [0, 1, 2].map(|i| aim[i] - start[i]);
        // A plain `add`: it wraps, the root reads it unsigned.
        let d = movement::fsqrt(mul(dx, dx).wrapping_add(mul(dy, dy)));
        let tan = div(LOB_SIN, LOB_COS);

        if d == Fixed::ZERO || div(dz, d) >= mul(LOB_STEEPEST, tan) {
            return None;
        }

        // Gravity -g / 32, the launch speed from the parabola through the
        // aim at the angle, split over the ground by the aim's heading.
        #[allow(clippy::cast_possible_truncation, reason = "a whole setting")]
        let gravity = -div(
            from_whole(settings::PARABOLA_GRAVITY as i32),
            from_whole(32),
        );
        let cot = div(from_whole(1), tan);
        let across = mul(cot, d);
        let reach = mul(from_whole(2), mul(mul(cot, cot), dz) - across);
        let up = movement::fsqrt(div(mul(gravity, mul(d, d)), reach));
        let ahead = mul(cot, up);

        Some(Lob {
            velocity: [div(mul(dx, ahead), d), div(mul(dy, ahead), d), up],
            gravity,
            floor: aim[2],
            falling: mul(gravity, div(d, ahead)) + up < Fixed::ZERO,
        })
    }

    /// One run (`OrderPhysics`, then
    /// `OrderPhysicsMissile`): the point moves by the
    /// velocity, then gravity takes the height's; whether it has landed.
    pub fn step(&mut self, at: &mut [Fixed; 3]) -> bool {
        for (a, v) in at.iter_mut().zip(self.velocity) {
            *a += v;
        }

        self.velocity[2] += self.gravity;

        if self.falling {
            self.velocity[2] < Fixed::from_bits(1) && at[2] <= self.floor
        } else {
            self.velocity[2] >= Fixed::ZERO && self.floor <= at[2]
        }
    }
}

/// A missile in flight, as drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flown {
    /// Its unit handle.
    pub handle: u32,
    /// Its projectile's descriptor.
    pub code: FourCc,
    /// 16.16 cells; the third is height, as the ground's.
    pub at: [Fixed; 3],
    /// Toward its goal over the ground (**Inferred**: the step's heading
    /// is not read).
    pub facing: i32,
    /// Game ms of its launch.
    pub launched: u32,
}

/// Missiles in flight.
#[derive(Clone, Debug, Default)]
pub(super) struct Flights {
    /// In launch order.
    list: Vec<Flight>,
}

impl Flights {
    /// Whether the missile of `handle` flies.
    pub(super) fn flying(&self, handle: u32) -> bool {
        self.list.iter().any(|f| f.handle == handle)
    }

    /// Where the missile of `handle` is, if it flies.
    pub(super) fn at(&self, handle: u32) -> Option<[Fixed; 3]> {
        self.list.iter().find(|f| f.handle == handle).map(|f| f.at)
    }

    /// Every missile's handle, in launch order.
    pub(super) fn handles(&self) -> impl Iterator<Item = u32> + '_ {
        self.list.iter().map(|f| f.handle)
    }
}

impl World {
    /// Every missile in flight, in launch order.
    #[must_use]
    pub fn flown(&self) -> Vec<Flown> {
        self.effects
            .flights
            .list
            .iter()
            .map(|f| Flown {
                handle: f.handle,
                code: f.missile.code,
                at: f.at,
                facing: f.facing,
                launched: f.launched,
            })
            .collect()
    }

    /// `target.method(missile)`: where a missile at agent `target` flies,
    /// about the target, 16.16 cells, drawing as method does. `None` where
    /// its place is unknown, or its draws are: the GPL RNG is not lent
    /// ([`super::step::Draws::Kept`]).
    pub(super) fn aim(&mut self, target: u32, missile: Missile) -> Option<[Fixed; 3]> {
        self.position(target)?;

        let body = self.body_of(target)?.clone();
        let hit = body.hit.offset3(self.drawn_facing(target));
        let scatter = match (body.class, missile.spread) {
            (Class::Building, Spread::Scattered) => {
                let (wide, deep) = body.span;
                let across = self.gpl_draw(wide / 2)? - wide / 4;
                let up = self.gpl_draw(deep / 2)? - deep / 4;

                [cells(across), Fixed::ZERO, rise(up).max(Fixed::ZERO)]
            }
            _ => [Fixed::ZERO; 3],
        };

        Some([0, 1, 2].map(|i| hit[i] + scatter[i]))
    }

    /// `target.method(missile)`: [`World::aim`] on its position, 32nds.
    pub(super) fn aim_at(&mut self, target: u32, missile: Missile) -> Option<(i32, i32)> {
        let [x, y, _] = self.aim(target, missile)?;
        let at = self.position(target)?;

        Some((at.0 + space::pos32(x), at.1 + space::pos32(y)))
    }

    /// Init action peak for `shooter`'s `shot`: a missile made
    /// unless its target unit is gone, taking the next handle; at a unit,
    /// `GE_OrderProcessor` aims as [`World::aim`] does, at a point
    /// method ([`World::at_point`]), and the missile flies
    /// ([`World::fly`]). It has no GPL agent.
    pub(super) fn launch(&mut self, shooter: u32, shot: Shot) {
        // The peak keeps the target's unit handle, not its agent:
        // a building `DeleteAgent` left standing is still shot at.
        let alive = match shot.target {
            Target::Unit(t) => self.pieces.contains(t),
            Target::Point(_) => true,
        };

        if alive {
            self.handle += 1;

            let handle = self.handle;

            self.first_vents(handle, shooter, shot.missile.code);

            if self.damaged(shooter, shot.missile.set) {
                self.revent(shooter);
            }

            let start = self.launch_point(shooter, shot.missile.set);
            let lobbed = self
                .descriptors
                .body(shot.missile.code)
                .and_then(|b| b.gait)
                .is_some_and(|g| g.lobbed());
            let flies = if lobbed { Flies::Lobbed } else { Flies::Homing };
            let flight = match (shot.target, flies) {
                (Target::Unit(t), Flies::Lobbed) => {
                    self.lobbed(handle, shot.missile, shooter, t, start)
                }
                (Target::Unit(t), Flies::Homing) => {
                    self.homing(handle, shot.missile, shooter, t, start)
                }
                (Target::Point(p), _) => {
                    self.at_point(handle, (shot.missile, flies), shooter, p, start)
                }
            };

            if let Some(flight) = flight {
                self.effects.flights.list.push(flight);
                self.dispatches.join(super::Listed::Missile(handle));
            }
        }
    }

    /// `GenUnit`: where piece `id`, playing `set`, launches from.
    ///
    /// `IsoGenUnit` reads the slot of the first part on
    /// layer 0 (`IsoUnitIMG` method `(0, 0)`, `IsoUnitIMG`). A building's action
    /// sets the direction of its layer-0 parts at once (set facing
    /// with `OrderAction`, 0); switching them to the set
    /// takes the slot from that direction by
    /// `UnitIMGPartEntry`: rounded to the set's slots, or for a
    /// `Directionless` descriptor the direction itself, 0 past the last
    /// ([`super::bodies::Facing`]). **Game**: so all 133 bolts the dwarves game's
    /// Dwarven Settlement and `Directionless` ballista tower shoot land in
    /// the log's frames (**Measured**).
    fn launch_point(&self, id: u32, set: u32) -> Option<[Fixed; 3]> {
        let at = self.unit_pos3(id)?;
        let body = self.body_of(id)?;
        let facing = u16::try_from(self.drawn_facing(id)).unwrap_or(0);
        let slots = body.launch.slots(set).unwrap_or(1);
        let off = body.launch.offset_in(set, body.facing.slot(facing, slots));

        Some([at[0] + off[0], at[1] + off[1], at[2] + off[2]])
    }

    /// The direction a missile made at `at` faces, toward agent `target`:
    /// `GenUnit` turns its layer 0 by direction to unit,
    /// the target's unit pos32 less its own. No
    /// order turns it after (**Inferred**: `OrderMove` and
    /// `OrderPhysics` move it alone). **Game**: the defeat probe's
    /// two catapult balls in their save face 8 and 11, toward the palace's
    /// and the guild's middles, not their aimed points.
    fn facing_at(&self, at: [Fixed; 3], target: u32) -> Option<i32> {
        let (x, y) = self.unit_pos(target)?;

        Some(movement::direction(
            space::pos32(x) - space::pos32(at[0]),
            space::pos32(y) - space::pos32(at[1]),
        ))
    }

    /// `GE_OrderProcessor` method `(missile, shooter, target, 0, 0)`: a
    /// `MoveMissile` from `start`. Aims first, so draws even where the
    /// start or the movement is unknown.
    fn homing(
        &mut self,
        handle: u32,
        missile: Missile,
        shooter: u32,
        target: u32,
        start: Option<[Fixed; 3]>,
    ) -> Option<Flight> {
        let offset = self.aim(target, missile)?;
        let at = start?;
        let goal = self.unit_pos3(target)?;
        let gait = self.descriptors.body(missile.code)?.gait?;
        // `GSUnit`. **Inferred**: a missile has no attributes, so no
        // modifier.
        let rate = movement::rate(gait.move_rate, 0);

        Some(Flight {
            handle,
            missile,
            shooter,
            target: Some(target),
            offset,
            at,
            goal: [
                goal[0] + offset[0],
                goal[1] + offset[1],
                goal[2] + offset[2],
            ],
            step: movement::step_length(gait.move_dist, rate, Finish::Exact),
            rate,
            when: self.now,
            launched: self.now,
            lob: None,
            facing: self.facing_at(at, target)?,
        })
    }

    /// `GE_OrderProcessor` method `(missile, shooter, target)` for a
    /// [`movement::Class::lobbed`] projectile: the target's method fixes the
    /// point (the draws at a building), then method lobs it there from
    /// `start`. It runs once a frame from the next, as a homing one's
    /// first step (**Inferred**: `OrderPhysicsMissile` has no rate; the
    /// saves' ball fits its first run a frame after the launch). A shot
    /// method refuses, method flies straight to the point
    /// ([`World::straight`]).
    fn lobbed(
        &mut self,
        handle: u32,
        missile: Missile,
        shooter: u32,
        target: u32,
        start: Option<[Fixed; 3]>,
    ) -> Option<Flight> {
        let offset = self.aim(target, missile)?;
        let at = start?;
        let goal = self.unit_pos3(target)?;
        let goal = [0, 1, 2].map(|i| goal[i] + offset[i]);
        let facing = self.facing_at(at, target)?;

        self.fling(handle, missile, shooter, (at, goal), facing)
    }

    /// One of two `GE_OrderProcessor` orders:
    /// a missile at GPL point `p`, on the ground there,
    /// from `start`; one flies it straight
    /// ([`World::straight`]), the other lobs it. It faces the
    /// point. No draws.
    fn at_point(
        &self,
        handle: u32,
        (missile, flies): (Missile, Flies),
        shooter: u32,
        p: (i32, i32),
        start: Option<[Fixed; 3]>,
    ) -> Option<Flight> {
        let at = start?;
        let goal = movement::on_ground(self.board.ground()?, p);
        let facing = movement::direction(p.0 - space::pos32(at[0]), p.1 - space::pos32(at[1]));

        match flies {
            Flies::Homing => self.straight(handle, missile, shooter, (at, goal), facing),
            Flies::Lobbed => self.fling(handle, missile, shooter, (at, goal), facing),
        }
    }

    /// `GE_OrderProcessor` from `at` to `goal`: a lob; refused,
    /// method straight ([`World::straight`]).
    fn fling(
        &self,
        handle: u32,
        missile: Missile,
        shooter: u32,
        (at, goal): ([Fixed; 3], [Fixed; 3]),
        facing: i32,
    ) -> Option<Flight> {
        match Lob::of(at, goal) {
            Some(lob) => Some(Flight {
                handle,
                missile,
                shooter,
                target: None,
                offset: [Fixed::ZERO; 3],
                at,
                goal,
                step: Fixed::ZERO,
                rate: MS_PER_FRAME,
                when: self.now,
                launched: self.now,
                lob: Some(lob),
                facing,
            }),
            None => self.straight(handle, missile, shooter, (at, goal), facing),
        }
    }

    /// `GE_OrderProcessor`: a `MoveMissile` from
    /// `at` to `goal`, both fixed, as method's but for its target handle,
    /// left 0, so `OrderMoveMissile` moves no goal and
    /// callback `0x100a` names no target; rate `GSUnit`, info 0.
    fn straight(
        &self,
        handle: u32,
        missile: Missile,
        shooter: u32,
        (at, goal): ([Fixed; 3], [Fixed; 3]),
        facing: i32,
    ) -> Option<Flight> {
        let gait = self.descriptors.body(missile.code)?.gait?;
        let rate = movement::rate(gait.move_rate, 0);

        Some(Flight {
            handle,
            missile,
            shooter,
            target: None,
            offset: [Fixed::ZERO; 3],
            at,
            goal,
            step: movement::step_length(gait.move_dist, rate, Finish::Exact),
            rate,
            when: self.now,
            launched: self.now,
            lob: None,
            facing,
        })
    }

    /// The `MoveMissile` run of the missile of `handle`, in its place in
    /// ORDR: a step once its rate is up, or the impact. A missile is a unit,
    /// so it joins ORDR's back with its `MoveMissile`
    /// and runs among the units, a draw of its impact
    /// before a later joiner's.
    pub(super) fn fly(&mut self, handle: u32) {
        let now = self.now;
        let due = self
            .effects
            .flights
            .list
            .iter()
            .position(|f| f.handle == handle && now.wrapping_sub(f.when) >= f.rate);

        if let Some(n) = due
            && let Some(mut lob) = self.effects.flights.list[n].lob
        {
            let f = &mut self.effects.flights.list[n];
            let landed = lob.step(&mut f.at);

            f.lob = Some(lob);
            f.when = now;

            if landed {
                let f = self.effects.flights.list.remove(n);

                self.alight(&f);
            }
        } else if let Some(n) = due {
            // `OrderMoveMissile`: the goal follows the target.
            let target = self.effects.flights.list[n]
                .target
                .and_then(|t| self.unit_pos3(t));
            let f = &mut self.effects.flights.list[n];

            if let Some(t) = target {
                f.goal = [t[0] + f.offset[0], t[1] + f.offset[1], t[2] + f.offset[2]];
            }

            let (at, leg) = movement::advance(f.at, f.goal, f.step, Finish::Exact);

            f.at = at;
            f.when = now;

            if leg == Leg::Done {
                let f = self.effects.flights.list.remove(n);

                self.land(&f);
            }
        }
    }

    /// Callback `0x100f`, a lobbed flight's end: the impact
    /// hook with the shooter, no target and the missile's point.
    fn alight(&mut self, f: &Flight) {
        let script = self
            .descriptors
            .body(f.missile.code)
            .and_then(|b| b.script.clone())
            .filter(|s| !s.function.is_empty());
        let shooter = self.unit(f.shooter).map(|_| f.shooter);

        // No target, so neither the magic mirror nor a target argument.
        if let Some(s) = script {
            self.strike(&s, shooter, None, f);
        }
    }

    /// Callback `0x100a`, then the impact hook for flight
    /// `f`: its projectile's function, or the magic mirror's return.
    fn land(&mut self, f: &Flight) {
        let script = self
            .descriptors
            .body(f.missile.code)
            .and_then(|b| b.script.clone())
            .filter(|s| !s.function.is_empty());
        let shooter = self.unit(f.shooter).map(|_| f.shooter);
        let target = f.target.filter(|t| self.unit(*t).is_some());
        let mirror = target
            .and_then(|t| {
                self.unit(t)?
                    .attributes
                    .get(&HAS_EFFECT_MAGIC_MIRROR.packed())
                    .copied()
            })
            .is_some_and(|m| m != 0);

        match script {
            Some(s)
                if s.kind & POINT_FORM == 0
                    && s.function.eq_ignore_ascii_case(MAKE_ATTACK)
                    && mirror =>
            {
                // Back at the shooter, which is now the target too.
                if let Some(back) =
                    shooter.filter(|b| s.kind & NO_CALLER == 0 && target != Some(*b))
                {
                    let flight = self.homing(f.handle, f.missile, back, back, Some(f.at));

                    self.effects.flights.list.extend(flight);
                } else {
                    self.impact_vents(f, f.at);
                }
            }
            Some(s) => self.strike(&s, shooter, target, f),
            None => {}
        }
    }

    /// Script dispatch of a projectile's `script` at flight `f`'s
    /// point: its arguments by `Script type`; an agent it needs gone,
    /// nothing.
    fn strike(&mut self, script: &Script, shooter: Option<u32>, target: Option<u32>, f: &Flight) {
        let at = f.at;
        let point = (space::pos32(at[0]), space::pos32(at[1]));

        self.impact_vents(f, at);

        if let Some(args) = script_args(script.kind, shooter, target, point) {
            self.dispatch_script(&script.function, args);
        }
    }

    /// Unit pos in 3D: its place, and its height: a walker's own,
    /// on its step's line between two nodes, not the ground under it
    /// (`OrderMove` steps x, y and z); a flier's
    /// ([`World::aloft`]); else the ground's there. **Game**: the defeat
    /// probe's catapult 16, stopped 304/65536 cells above the ground, lobs
    /// ball 954 to where its save has it, 27/65536 short of the ground's
    /// launch. **Inferred**: a walker placed anew (not by a step) is on
    /// the ground, so a walker point elsewhere is stale.
    pub(super) fn unit_pos3(&self, id: u32) -> Option<[Fixed; 3]> {
        let (x, y) = self.unit_pos(id)?;
        let walked = self
            .units
            .tracks(id)
            .map(super::walking::Tracks::point)
            .filter(|p| (p[0], p[1]) == (x, y));

        match (walked, self.aloft(id)) {
            (Some(p), _) => Some(p),
            (None, Some(z)) => Some([x, y, z]),
            (None, None) => Some([x, y, movement::ground(self.board.ground()?, x, y)]),
        }
    }

    /// Piece `id`'s own height, unit height, where it flies: its walker's
    /// ([`movement::Walker::aloft`]); not yet walked, its class's height
    /// over the ground it stands on ([`movement::aloft`]), as
    /// unit move to places a unit on movement layer 4. `None` on the
    /// ground. **Corpus**: both ice dragons the united winter game saves
    /// hold 180/32, their least height.
    pub(super) fn aloft(&self, id: u32) -> Option<Fixed> {
        self.aloft_of(id, self.units.tracks(id))
    }

    /// [`World::aloft`] of piece `id` whose `tracks` are read already.
    pub(super) fn aloft_of(
        &self,
        id: u32,
        tracks: Option<&super::walking::Tracks>,
    ) -> Option<Fixed> {
        tracks.map_or_else(|| self.placed_aloft(id), super::walking::Tracks::aloft)
    }

    /// Where unit move to placed flier `id`: its class's height over
    /// the ground under it. `None` for a walker.
    fn placed_aloft(&self, id: u32) -> Option<Fixed> {
        let gait = self.descriptors.body(self.any_piece(id)?.code)?.gait?;
        let (x, y) = self.unit_pos(id)?;
        let ground = movement::ground(self.board.ground()?, x, y);

        Some(movement::aloft([x, y, ground], Some(gait.flight()?))[2])
    }

    /// The facing that picks the slot piece `id` draws: its walker's, else
    /// a new game piece's direction word, else [`super::walking::FIRST_FACING`].
    fn drawn_facing(&self, id: u32) -> i32 {
        match (self.units.tracks(id), self.units.direction(id)) {
            (Some(t), _) => t.facing(),
            (None, Some(d)) => i32::from(d),
            (None, None) => super::walking::FIRST_FACING,
        }
    }

    /// A GPL RNG draw by engine code, on the state lent for the call.
    fn gpl_draw(&mut self, bound: i32) -> Option<i32> {
        self.rngs.lent_draw(bound)
    }

    /// `CreateMissile(name, launcher, target)`: a descriptor by name or a
    /// throw; at a unit or a coordinate, [`World::launch`] from the
    /// launcher's set (its cast or attack set, else attack:
    /// `GenUnit`); any other target throws.
    pub(super) fn create_missile(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let name = string_ref(vm, args, 1)?;
        let launcher = agent_ref(vm, args, 2)?;
        let target = match vm.object(vm.arg(args, 3)?) {
            Value::Agent(t) => Some(Target::Unit(*t)),
            Value::Coord(x, y) => Some(Target::Point((*x, *y))),
            _ => None,
        };
        let missile = self
            .descriptors
            .body_named(&name)
            .map(|(code, b)| Missile::of(code, 0).with(b.spread))
            .ok_or_else(|| {
                Fault::throw(format!(
                    "CreateMissile() could not make unit of type {name} (could not find description) "
                ))
            })?;
        // Thrown after the descriptor's lookup.
        let target = target.ok_or_else(|| Fault::throw("CreateMissile() has no valid target"))?;
        // Whether the target is there: a point always is; `None` unsure.
        let there = match target {
            Target::Unit(t) => match self.found(t) {
                Found::Piece(_) => Some(true),
                Found::Gone => Some(false),
                Found::Unsure => None,
            },
            Target::Point(_) => Some(true),
        };

        match (self.found(launcher), there) {
            (Found::Piece(_), Some(true)) => {
                let set = match self.playing(launcher) {
                    Some(CAST_SET) => CAST_SET,
                    _ => ATTACK_SET,
                };
                let shot = Shot {
                    missile: missile.from(set),
                    target,
                };

                self.launch(launcher, shot);
                Ok(Answer::Given)
            }
            (Found::Gone, _) | (_, Some(false)) => Ok(Answer::Given),
            _ => Ok(Answer::Unknown),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 120² + 160² passes 16.16's 32767; method's sum is a plain `add`
    /// that wraps into an unsigned root.
    #[test]
    fn a_lob_past_181_tiles_wraps_as_the_engine() {
        let aim = [from_whole(120), from_whole(160), Fixed::ZERO];

        assert!(Lob::of([Fixed::ZERO; 3], aim).is_some());
    }

    #[test]
    fn a_standing_hit_follows_the_slot_its_facing_draws() {
        let hit = Hit::Standing((1..=8).map(|x| (x, -10)).collect());

        assert_eq!(hit.offset(0), (1, 0));
        assert_eq!(hit.offset(4), (2, 0));
        assert_eq!(hit.offset3(4), [from_pos32(2), Fixed::ZERO, from_pos32(18)]);
        assert_eq!(Hit::Centre.offset(9), (0, 0));
    }

    #[test]
    fn a_missile_leaves_from_its_cast_or_attack_set() {
        let missile = Missile::of(FourCc(*b"BPB8"), 0);

        assert_eq!(missile.from(CAST_SET).set, CAST_SET);
        assert_eq!(missile.from(ATTACK_SET).set, ATTACK_SET);
        assert_eq!(missile.from(0x40).set, ATTACK_SET);
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Spread {
        type Model = m::Spread;

        fn save(&self) -> m::Spread {
            match self {
                Spread::Scattered => m::Spread::Scattered,
                Spread::Fixed => m::Spread::Fixed,
            }
        }

        fn load(model: m::Spread) -> Result<Spread, Refused> {
            Ok(match model {
                m::Spread::Scattered => Spread::Scattered,
                m::Spread::Fixed => Spread::Fixed,
            })
        }
    }

    impl Saved for Missile {
        type Model = m::Missile;

        fn save(&self) -> m::Missile {
            m::Missile {
                code: self.code.save(),
                spread: self.spread.save(),
                set: self.set,
            }
        }

        fn load(model: m::Missile) -> Result<Missile, Refused> {
            Ok(Missile {
                code: FourCc::load(model.code)?,
                spread: Spread::load(model.spread)?,
                set: model.set,
            })
        }
    }

    impl Saved for Target {
        type Model = m::Target;

        fn save(&self) -> m::Target {
            match *self {
                Target::Unit(id) => m::Target::Unit(id),
                Target::Point(at) => m::Target::Point(at),
            }
        }

        fn load(model: m::Target) -> Result<Target, Refused> {
            Ok(match model {
                m::Target::Unit(id) => Target::Unit(id),
                m::Target::Point(at) => Target::Point(at),
            })
        }
    }

    impl Saved for Shot {
        type Model = m::Shot;

        fn save(&self) -> m::Shot {
            m::Shot {
                missile: self.missile.save(),
                target: self.target.save(),
            }
        }

        fn load(model: m::Shot) -> Result<Shot, Refused> {
            Ok(Shot {
                missile: Missile::load(model.missile)?,
                target: Target::load(model.target)?,
            })
        }
    }

    impl Saved for Script {
        type Model = m::Script;

        fn save(&self) -> m::Script {
            m::Script {
                function: self.function.clone(),
                kind: self.kind,
            }
        }

        fn load(model: m::Script) -> Result<Script, Refused> {
            Ok(Script {
                function: model.function,
                kind: model.kind,
            })
        }
    }

    impl Saved for Lob {
        type Model = m::Lob;

        fn save(&self) -> m::Lob {
            let Lob {
                velocity,
                gravity,
                floor,
                falling,
            } = *self;

            m::Lob {
                velocity: velocity.save(),
                gravity: gravity.save(),
                floor: floor.save(),
                falling,
            }
        }

        fn load(model: m::Lob) -> Result<Lob, Refused> {
            Ok(Lob {
                velocity: <[Fixed; 3]>::load(model.velocity)?,
                gravity: Fixed::load(model.gravity)?,
                floor: Fixed::load(model.floor)?,
                falling: model.falling,
            })
        }
    }

    impl Saved for Flight {
        type Model = m::Flight;

        fn save(&self) -> m::Flight {
            let Flight {
                handle,
                missile,
                shooter,
                target,
                offset,
                at,
                goal,
                step,
                rate,
                when,
                launched,
                lob,
                facing,
            } = self;

            m::Flight {
                handle: *handle,
                missile: missile.save(),
                shooter: *shooter,
                target: *target,
                offset: offset.save(),
                at: at.save(),
                goal: goal.save(),
                step: step.save(),
                rate: *rate,
                when: *when,
                launched: *launched,
                lob: lob.save(),
                facing: *facing,
            }
        }

        fn load(model: m::Flight) -> Result<Flight, Refused> {
            Ok(Flight {
                handle: model.handle,
                missile: Missile::load(model.missile)?,
                shooter: model.shooter,
                target: model.target,
                offset: <[Fixed; 3]>::load(model.offset)?,
                at: <[Fixed; 3]>::load(model.at)?,
                goal: <[Fixed; 3]>::load(model.goal)?,
                step: Fixed::load(model.step)?,
                rate: model.rate,
                when: model.when,
                launched: model.launched,
                lob: Option::load(model.lob)?,
                facing: model.facing,
            })
        }
    }

    impl Saved for Flights {
        type Model = m::Flights;

        fn save(&self) -> m::Flights {
            m::Flights {
                list: self.list.save(),
            }
        }

        fn load(model: m::Flights) -> Result<Flights, Refused> {
            Ok(Flights {
                list: Vec::load(model.list)?,
            })
        }
    }
}
