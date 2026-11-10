//! Particles: what a particle system's unit (`sim::world::plume`) emits,
//! moves, ages and draws. **Binary** unless marked: `CY3D_ParticleSystem`,
//! `UnitIMGParticleSystem` and `UnitIMGParticle` in `MajestyHD.exe`.
//!
//! ```text
//!  particle manager, each frame
//!   each system, last made first     CY3D_ParticleSystem
//!    clock: game ms / 1000, s        UnitIMGParticleSystem
//!    first update: InitialParticles  UnitIMGParticleSystem
//!    past its window: all die             CY3D_ParticleSystem
//!    emitting: Rate x dt into a           CY3D_ParticleSystem,      a birth per free
//!      fraction; whole ones born          slot from a cursor:      CY3D_ParticleSystem
//!    then each live one, last first       CY3D_ParticleSystem:       dies past its life
//!      or out of Bounds; else          or box; gravity, step, jitter; its
//!      moves                           sprite's frame (UnitIMGParticle)
//!  drawn: a part of the system's     IsoGenUnit,       mode
//!    unit, its own place, its frame  0x1000 parts (IsoUnitIMGPartEntry
//!    of the system's set, blitted    IsoUnitIMGPartEntry     ): the tile's own blit, no
//!    as any unit's frame; alpha      scale or tint; alpha below 1 mode 8
//!    below 1 glows                   (IsoUnitIMGPartEntry)
//! ```
//!
//! Units: cells across, the map's height units up (a unit's
//! Unit pos, as floats), seconds.
//!
//! Colour, size and spin are worked out per particle every update; the
//! draw reads only the alpha, as mode 8 below 1
//! (`IsoUnitIMGPartEntry`, [`crate::view::raster::blend::fade`]).
//! [`Particle::colour`] and [`Particle::size`] say what the engine
//! computes.
//!
//! No rule reads a particle: a system's unit ends by its window and
//! emitter, not its particles. So they are not the world's: a front end
//! owns [`Sparks`] and steps it after each world frame, from the
//! particle manager's pass the world records ([`World::particle_sweep`]). Nor
//! are they saved: after a load each system starts afresh, as the
//! engine's clock-seeded draws would differ anyway (**Inferred**).
//!
//! ```text
//!  World::step ──> plume_step ──> Sweep: each standing system's handle,
//!                                 code, made, place, window
//!  front end ──> Sparks::step(world) ──> System::update, last made first
//!            └─> Sparks::motes(world) ──> what to draw
//! ```

use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, PI};

use crate::base::fixed::Fixed;
use crate::data::desc::EmitterShape;
use crate::data::fourcc::FourCc;
use crate::sim::world::World;
use crate::sim::world::particles::{Emitting, Past, STEP_MIN, Spec, Sprite, Window};

/// Rng next particle: an LCG with the *Numerical
/// Recipes* constants, no scramble. Another generator than
/// [`crate::base::rng`]: the multiplier is the same, the addend is not.
pub const MULTIPLIER: u32 = crate::base::rng::MULTIPLIER.cast_unsigned();
pub const ADDEND: u32 = 0x3c6e_f35f;

/// The bound every particle draw passes, `0x7fffffff`: the state's
/// low 31 bits.
const WHOLE: u32 = 0x7fff_ffff;

/// Two pow minus, the double `2^-31`: a draw over it is in `[0, 1)`.
pub const UNIT: f64 = 1.0 / 2_147_483_648.0;

/// Azimuth span: the azimuth's span, `2 pi` less a degree.
pub const AZIMUTH_SPAN: f32 = 6.265_732;

/// Degrees to radians: degrees to radians, as the float the engine keeps.
pub const RADIANS: f32 = 0.017_453_292;

/// Game ms in a second: `fdiv` by scroll ramp consts, 1000.0.
const MS_PER_S: f64 = 1000.0;

/// 16.16 fixed point: the engine multiplies by `1.5258789e-05`.
const FIXED_ONE: f32 = 65_536.0;

/// The size factor `CY3D_ParticleSystem` draws a particle's size
/// over (in-memory particle): no file field fills them.
/// **Inferred**: particle desc default's defaults, 1 and 0.
const SCALE: (f32, f32) = (1.0, 0.0);

/// The particle RNG: its own
/// state, which no other stream shares. The engine seeds it from the
/// clock as it starts (a static initialiser, `rng_seed(0)`), so its draws are
/// not reproducible; a seed here is a stand-in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rng(pub u32);

impl Rng {
    /// `rng_next_particle(0x7fffffff)`: the state's low 31 bits.
    pub const fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND);
        self.0 & WHOLE
    }

    /// A draw over `2^31`, as a float: `[0, 1)`.
    #[allow(clippy::cast_possible_truncation, reason = "the engine's fstp")]
    pub fn unit(&mut self) -> f32 {
        (f64::from(self.next()) * UNIT) as f32
    }

    /// Particle between: `low` to `high`.
    pub fn between(&mut self, low: f32, high: f32) -> f32 {
        (high - low) * self.unit() + low
    }

    /// Rng next of `count` on this state, as `UnitIMGParticle`
    /// picks a slot: the world's scrambled formula.
    pub fn pick(&mut self, count: u16) -> u16 {
        let drawn = crate::base::rng::draw(&mut self.0, i32::from(count));

        u16::try_from(drawn).unwrap_or(0)
    }
}

/// One live particle (`CY3D_PS_Particle`, `UnitIMGParticle`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    /// Its place, absolute: cells across, height up.
    pub at: [f32; 3],
    /// Per second.
    pub velocity: [f32; 3],
    /// Born at, s of the system's clock.
    pub born: f32,
    /// Its life, s.
    pub life: f32,
    /// Its size factor.
    pub scale: f32,
    /// The sprite's slot and frame (part).
    pub slot: u16,
    pub frame: u16,
    /// Game ms its frame last stepped, and each frame's ms,
    /// 0 for a still one.
    pub stepped: u32,
    pub frame_ms: u32,
}

impl Particle {
    /// Its colour at system time `now`: birth to midlife colour over the
    /// first half of life, midlife to death after; alpha by `FadeTime`
    /// instead. The draw reads the alpha.
    #[must_use]
    pub fn colour(&self, spec: &Spec, now: f32) -> [f32; 4] {
        let [birth, mid, death] = spec.colours;
        let half = self.born + self.life * 0.5;
        let mut out = if now < half {
            lerp4(birth, mid, (now - self.born) / (half - self.born))
        } else {
            lerp4(mid, death, (now - half) / (self.end() - half))
        };

        out[3] = self.ramp(spec.fade_time, [birth[3], mid[3], death[3]], now);
        out
    }

    /// Its size at `now`: birth to max over `SizeTime` in, max to death
    /// over `SizeTime` out, times its factor. The draw reads none of it.
    #[must_use]
    pub fn size(&self, spec: &Spec, now: f32) -> f32 {
        self.ramp(spec.size_time, spec.sizes, now) * self.scale
    }

    /// The engine's three-stage ramp: `values[0]` to `values[1]` over
    /// `share.0` of life, held, then to `values[2]` over `share.1`.
    fn ramp(&self, share: (f32, f32), values: [f32; 3], now: f32) -> f32 {
        let rise = self.born + share.0 * self.life;
        let fall = self.end() - share.1 * self.life;

        // method keeps each stage's reciprocal; method multiplies by it.
        if now < rise {
            lerp(
                values[0],
                values[1],
                (now - self.born) * (1.0 / (rise - self.born)),
            )
        } else if now <= fall {
            values[1]
        } else {
            lerp(
                values[1],
                values[2],
                (now - fall) * (1.0 / (self.end() - fall)),
            )
        }
    }

    fn end(&self) -> f32 {
        self.born + self.life
    }

    /// `UnitIMGParticle` at game ms `clock`: a frame
    /// on once its time is up, round at the last.
    fn step_frame(&mut self, clock: u32, sprite: &Sprite) {
        let due = self.frame_ms != 0 && clock.wrapping_sub(self.stepped) >= self.frame_ms;

        if due {
            let frames = sprite.slots.get(usize::from(self.slot)).map_or(0, |s| s.0);
            let next = self.frame + 1;

            self.frame = if next >= frames { 0 } else { next };
            self.stepped = clock;
        }
    }
}

/// One system's particles (`CY3D_ParticleSystem`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct System {
    /// One per `MaxParticles`; `None` free (alive a field).
    pub slots: Vec<Option<Particle>>,
    /// Whether each slot was ever born, for `Recycle`.
    born: Vec<Option<Particle>>,
    /// Where the next birth looks first.
    cursor: usize,
    /// Births owed, a fraction.
    owed: f32,
    /// The clock at this update and the last, s.
    now: f32,
    before: f32,
    /// `InitialParticles` still to come (system bit 2).
    primed: bool,
    /// The emitter's place about the unit and its velocity.
    offset: [f32; 3],
    velocity: [f32; 3],
}

impl System {
    /// A new system at game ms `clock`: its clock read
    /// twice, so its first update steps the shortest
    /// step.
    #[must_use]
    pub fn new(spec: &Spec, clock: u32) -> System {
        let now = seconds(clock);

        System {
            slots: vec![None; spec.max as usize],
            born: vec![None; spec.max as usize],
            cursor: 0,
            owed: 0.0,
            now,
            before: now,
            primed: true,
            offset: spec.offset,
            velocity: spec.velocity,
        }
    }

    /// Its clock at the last update, s.
    #[must_use]
    pub const fn now(&self) -> f32 {
        self.now
    }

    /// The live particles.
    pub fn live(&self) -> impl Iterator<Item = &Particle> {
        self.slots.iter().flatten()
    }

    /// `CY3D_ParticleSystem` method `(0)` at `at`.
    pub fn update(&mut self, at: Moment<'_>, rng: &mut Rng, window: Window) {
        self.before = self.now;
        self.now = seconds(at.clock);

        if self.primed {
            self.emit(at, rng, at.spec.initial);
            self.primed = false;
        }

        // The box is the emitter's before this update moves it.
        let origin = self.origin(at.unit);
        let low = add(origin, at.spec.bounds.0);
        let high = add(origin, at.spec.bounds.1);

        if window.past == Past::Beyond {
            self.slots.iter_mut().for_each(|s| *s = None);
        } else {
            let step = (self.now - self.before).max(STEP_MIN);
            let rate = at.spec.rate;

            if rate > 0.0 && window.emitting == Emitting::Yes {
                self.owed += rate * step;

                if self.owed >= 1.0 {
                    // float to int truncates.
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a few births"
                    )]
                    let whole = self.owed as u32;

                    self.emit(at, rng, whole);

                    #[allow(clippy::cast_precision_loss, reason = "a few births")]
                    let spent = whole as f32;

                    self.owed -= spent;
                }
            }

            self.travel(at, rng, step, (low, high));
        }
    }

    /// The emitter's place: the unit's, plus its offset in 16.16
    /// (`UnitIMGParticleSystem`, unattached).
    /// **Inferred**: system, an attachment point, is 0; no
    /// writer but the save reader is found.
    fn origin(&self, unit: [Fixed; 3]) -> [f32; 3] {
        #[allow(clippy::cast_possible_truncation, reason = "float to fixed")]
        let fixed = |v: f32| Fixed::from_bits((v * FIXED_ONE) as i32);

        #[allow(clippy::cast_precision_loss, reason = "16.16 of a map")]
        std::array::from_fn(|i| {
            unit[i].wrapping_add(fixed(self.offset[i])).to_bits() as f32 / FIXED_ONE
        })
    }

    /// `method(n)`: up to `n` births, one per free slot from the cursor, a
    /// look at each slot at most.
    fn emit(&mut self, at: Moment<'_>, rng: &mut Rng, n: u32) {
        let mut left = n;
        let count = self.slots.len();

        for _ in 0..count {
            if left == 0 {
                break;
            }

            let i = self.cursor;

            if self.slots[i].is_none() {
                self.birth(at, rng, i);
                left -= 1;
            }

            self.cursor = (self.cursor + 1) % count;
        }
    }

    /// `method(i)`: a particle in slot `i`.
    fn birth(&mut self, at: Moment<'_>, rng: &mut Rng, i: usize) {
        let spec = at.spec;
        let speed = (1.0 - spec.speed_variance * rng.unit()) * spec.speed;
        let azimuth = rng.unit() * AZIMUTH_SPAN;
        let (out, heading) = launch(spec, rng, azimuth);
        let origin = self.origin(at.unit);
        let life = spec.life * (1.0 - spec.life_variance * rng.unit());
        let scale = (1.0 - SCALE.1 * rng.unit()) * SCALE.0;
        let kept = self.born[i].filter(|_| spec.recycle);
        let (slot, frame, stepped, frame_ms) = match kept {
            Some(p) => (p.slot, p.frame, p.stepped, p.frame_ms),
            None => dress(at, rng),
        };
        let particle = Particle {
            at: add(origin, out),
            velocity: add(self.velocity, heading.map(|d| d * speed)),
            born: self.now,
            life,
            scale,
            slot,
            frame,
            stepped,
            frame_ms,
        };

        self.slots[i] = Some(particle);
        self.born[i] = Some(particle);
    }

    /// method: the emitter moves, then each live particle,
    /// last slot first, dies or moves.
    fn travel(&mut self, at: Moment<'_>, rng: &mut Rng, step: f32, (low, high): Box3) {
        let spec = at.spec;

        self.velocity = add(self.velocity, spec.acceleration.map(|a| a * step));
        self.offset = add(self.offset, self.velocity.map(|v| v * step));

        for i in (0..self.slots.len()).rev() {
            let Some(mut p) = self.slots[i] else {
                continue;
            };
            let outside = (0..3).any(|k| p.at[k] < low[k] || p.at[k] > high[k]);

            if self.now - p.born > p.life || outside {
                self.slots[i] = None;
                continue;
            }

            p.velocity = add(p.velocity, spec.gravity.map(|g| g * step));
            p.at = add(p.at, p.velocity.map(|v| v * step));

            for k in 0..3 {
                p.at[k] += spec.jitter[k] * rng.unit();
            }

            // Particle spread twice; the turn they give is kept nowhere.
            for k in 0..2 {
                let reach = spec.turn_jitter[k] * PI;

                rng.between(-reach, reach);
            }

            p.step_frame(at.clock, at.sprite);
            self.slots[i] = Some(p);
            self.born[i] = Some(p);
        }
    }
}

/// An update's inputs: game ms, the unit's place (16.16 cells and
/// height, unit pos), the descriptor, the art.
#[derive(Clone, Copy, Debug)]
pub struct Moment<'a> {
    pub clock: u32,
    pub unit: [Fixed; 3],
    pub spec: &'a Spec,
    pub sprite: &'a Sprite,
}

/// A box, low and high corners.
type Box3 = ([f32; 3], [f32; 3]);

/// Where a birth starts about the emitter, and its heading, by shape.
/// Each draws on `rng` as the engine does.
///
/// ```text
///  Point:         heading tilted 0 (Disk: 90 degrees; Column: up to the
///                 variance) off +z at the azimuth; starts at the emitter
///  Area+Column:   out r = Radius x (1 - RadiusVariance .. 1); tilt
///                 variance x r / Radius; starts r out, then up to one
///                 cell along the heading
///  Area+Disk:     tilt 90 degrees +- variance; starts r along the heading
///  else:          no heading, at the emitter
/// ```
fn launch(spec: &Spec, rng: &mut Rng, azimuth: f32) -> ([f32; 3], [f32; 3]) {
    let tilt = spec.direction_variance * RADIANS;

    if spec.has(EmitterShape::Point) {
        let polar = if spec.has(EmitterShape::Disk) {
            FRAC_PI_2
        } else if spec.has(EmitterShape::Column) {
            rng.between(0.0, tilt)
        } else {
            0.0
        };

        ([0.0; 3], heading(azimuth, polar))
    } else if spec.has(EmitterShape::Area) {
        let reach = rng.between(
            spec.radius - spec.radius_variance * spec.radius,
            spec.radius,
        );

        if spec.has(EmitterShape::Column) {
            let dir = heading(azimuth, tilt * (reach / spec.radius));
            let along = rng.unit();
            let out = add(
                heading(azimuth, FRAC_PI_2).map(|v| v * reach),
                dir.map(|v| v * along),
            );

            (out, dir)
        } else if spec.has(EmitterShape::Disk) {
            let dir = heading(azimuth, rng.between(FRAC_PI_2 - tilt, FRAC_PI_2 + tilt));

            (dir.map(|v| v * reach), dir)
        } else {
            ([0.0; 3], [0.0; 3])
        }
    } else {
        ([0.0; 3], [0.0; 3])
    }
}

/// A new particle's sprite: slot 0, else a draw
/// over the set's slots; frame 0, its time from now.
fn dress(at: Moment<'_>, rng: &mut Rng) -> (u16, u16, u32, u32) {
    let count = u16::try_from(at.sprite.slots.len()).unwrap_or(u16::MAX);
    let slot = if count > 1 { rng.pick(count) } else { 0 };
    let frame_ms = at.sprite.slots.get(usize::from(slot)).map_or(0, |s| s.1);

    (slot, 0, at.clock, frame_ms)
}

/// Heading: the unit vector `polar` off `+z`, at `azimuth` from `+x`.
fn heading(azimuth: f32, polar: f32) -> [f32; 3] {
    let out = polar.sin();

    [azimuth.cos() * out, azimuth.sin() * out, polar.cos()]
}

/// Game ms as the system's clock, s.
#[allow(clippy::cast_possible_truncation, reason = "the engine's fstp")]
fn seconds(clock: u32) -> f32 {
    (f64::from(clock) / MS_PER_S) as f32
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a) * t + a
}

fn lerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|i| lerp(a[i], b[i], t))
}

/// Every system's particles, by its unit's handle, and the RNG they
/// draw: the particle manager's state, which a front end owns.
#[derive(Clone, Debug, Default)]
pub struct Sparks {
    rng: Rng,
    systems: BTreeMap<u32, System>,
    /// The game ms it started at: a system the world made before it
    /// starts there instead (a resumed game's).
    start: u32,
    /// The game ms of the last pass followed.
    last: Option<u32>,
}

impl Sparks {
    /// None yet, the RNG seeded with `seed`, started at game ms `start`.
    #[must_use]
    pub fn new(seed: u32, start: u32) -> Sparks {
        Sparks {
            rng: Rng(seed),
            start,
            ..Sparks::default()
        }
    }

    /// The particle manager's pass over each system not yet freed, last
    /// made first, as the world's last
    /// frame recorded it ([`World::particle_sweep`]): its particles born, moved
    /// and killed ([`System::update`]) about its unit's place. A freed
    /// or deleted system's particles go with it; a new one starts at
    /// its making. Nothing where the pass was
    /// followed already.
    pub fn step(&mut self, world: &World) {
        let sweep = world.particle_sweep().filter(|s| self.last != Some(s.at));

        if let Some(sweep) = sweep {
            let particles = |code| world.descriptors().body(code)?.particles.as_ref();

            self.systems
                .retain(|h, _| sweep.sources.iter().any(|s| s.handle == *h));

            for s in &sweep.sources {
                if let (false, Some(body)) =
                    (self.systems.contains_key(&s.handle), particles(s.code))
                {
                    let clock = s.made.max(self.start);

                    self.systems
                        .insert(s.handle, System::new(&body.spec, clock));
                }
            }

            for s in sweep.sources.iter().rev() {
                let system = self.systems.get_mut(&s.handle);

                if let (Some(body), Some(unit), Some(system)) = (particles(s.code), s.unit, system)
                {
                    let moment = Moment {
                        clock: sweep.at,
                        unit,
                        spec: &body.spec,
                        sprite: &body.sprite,
                    };

                    system.update(moment, &mut self.rng, s.window);
                }
            }

            self.last = Some(sweep.at);
        }
    }

    /// Each live particle, system by system in the order the world made
    /// them.
    #[must_use]
    pub fn motes(&self, world: &World) -> Vec<Mote> {
        world
            .plumes()
            .iter()
            .filter_map(|p| {
                let system = self.systems.get(&p.handle)?;
                let particles = world.descriptors().body(p.code)?.particles.as_ref()?;
                let (spec, set, now) = (&particles.spec, particles.sprite.set, system.now());

                let pooled = system.slots.iter().enumerate();

                Some(
                    pooled
                        .filter_map(|(i, m)| m.as_ref().map(|m| (i, m)))
                        .map(move |(i, m)| Mote {
                            id: (p.handle, i),
                            born: m.born,
                            age: now - m.born,
                            code: p.code,
                            set,
                            slot: m.slot,
                            frame: m.frame,
                            at: m.at,
                            alpha: m.colour(spec, now)[3],
                        }),
                )
            })
            .flatten()
            .collect()
    }
}

/// One particle as drawn: its system's art, set, slot and frame, and its
/// place, cells across and height up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mote {
    /// Which particle: its system's unit handle and pool slot, and when
    /// it was born there; a slot is reused, so a birth tells one from
    /// the next.
    pub id: (u32, usize),
    pub born: f32,
    /// Seconds since its birth, by its system's clock.
    pub age: f32,
    pub code: FourCc,
    pub set: u32,
    pub slot: u16,
    pub frame: u16,
    pub at: [f32; 3],
    /// Its alpha at the last update ([`Particle::colour`]): below 1 it
    /// draws as mode 8 ([`crate::view::raster::blend::fade`]).
    pub alpha: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::fixed::from_whole;
    use crate::sim::world::particles::tests::fountain;

    const STILL: Window = Window {
        emitting: Emitting::Yes,
        past: Past::Within,
    };

    fn moment<'a>(clock: u32, spec: &'a Spec, sprite: &'a Sprite) -> Moment<'a> {
        Moment {
            clock,
            unit: [from_whole(2), from_whole(3), Fixed::ZERO],
            spec,
            sprite,
        }
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    /// `Smoke_Large`'s alpha: 0 to 1 over a tenth of life, then back
    /// to 0; so it draws mode 8 all its life but at its peak.
    #[test]
    fn a_smoke_particle_fades_in_then_out() {
        let spec = Spec {
            colours: [[0.0; 4], [0.0, 0.0, 0.0, 1.0], [0.0; 4]],
            fade_time: (0.1, 0.9),
            ..fountain()
        };
        let p = Particle {
            at: [0.0; 3],
            velocity: [0.0; 3],
            born: 10.0,
            life: 4.0,
            scale: 1.0,
            slot: 0,
            frame: 0,
            stepped: 0,
            frame_ms: 0,
        };
        let alpha = |now| p.colour(&spec, now)[3];

        assert!(near(alpha(10.0), 0.0));
        assert!(near(alpha(10.2), 0.5));
        assert!(near(alpha(10.4), 1.0));
        assert!(near(alpha(12.2), 0.5));

        // Exact ends: `view::raster::blend::fade` draws only 0 at level 9, and only
        // 1 as the tile's own mode.
        #[allow(clippy::float_cmp, reason = "the fade's ends are exact")]
        let ends = (alpha(10.0) == 0.0, alpha(10.4) == 1.0);

        assert_eq!(ends, (true, true));
    }

    #[test]
    fn a_particle_is_born_at_the_emitter_and_falls_under_gravity() {
        let spec = fountain();
        let sprite = Sprite {
            set: 0x40,
            slots: vec![(4, 100)],
        };
        let mut rng = Rng(1);
        let mut system = System::new(&spec, 1_000);

        // First update: the clock has not moved, so the shortest step.
        system.update(moment(1_000, &spec, &sprite), &mut rng, STILL);

        let p = *system.live().next().expect("the initial particle");

        assert!(
            near(p.at[0], 2.5) && near(p.at[1], 3.0),
            "the unit plus the offset"
        );
        assert!(near(p.velocity[2], 2.0 - 4.0 * STEP_MIN));
        assert!(near(p.at[2], p.velocity[2] * STEP_MIN));

        // Four draws to be born (speed, azimuth, life, size), five to move.
        let mut expected = Rng(1);

        (0..9).for_each(|_| {
            expected.next();
        });
        assert_eq!(rng, expected);

        // 50 ms on: velocity 2 - 4 x 0.05, then a step of it.
        system.update(moment(1_050, &spec, &sprite), &mut rng, STILL);

        let q = *system.live().next().expect("still alive");
        let step = 1.05_f32 - 1.0;

        assert!(near(q.velocity[2], p.velocity[2] - 4.0 * step));
        assert!(near(q.at[2], p.at[2] + q.velocity[2] * step));
        assert_eq!(q.frame, 0, "a 100 ms frame shows two updates");

        system.update(moment(1_100, &spec, &sprite), &mut rng, STILL);
        assert_eq!(system.live().next().map(|p| p.frame), Some(1));

        // Past its life of 1 s it dies.
        system.update(moment(2_050, &spec, &sprite), &mut rng, STILL);
        assert_eq!(system.live().count(), 0);
    }

    #[test]
    fn births_follow_the_rate_and_stop_with_the_emitter() {
        let spec = Spec {
            initial: 0,
            rate: 30.0,
            ..fountain()
        };
        let sprite = Sprite::default();
        let mut rng = Rng(7);
        let mut system = System::new(&spec, 0);

        system.update(moment(0, &spec, &sprite), &mut rng, STILL);
        assert_eq!(system.live().count(), 0, "30 x 1e-5 is no birth");

        // 30 a second over 50 ms is 1.5: one born, a half owed.
        system.update(moment(50, &spec, &sprite), &mut rng, STILL);
        assert_eq!(system.live().count(), 1);
        system.update(moment(100, &spec, &sprite), &mut rng, STILL);
        assert_eq!(system.live().count(), 2, "the max");

        let stopped = Window {
            emitting: Emitting::No,
            past: Past::Within,
        };
        let gone = Window {
            emitting: Emitting::No,
            past: Past::Beyond,
        };

        system.update(moment(150, &spec, &sprite), &mut rng, stopped);
        assert_eq!(system.live().count(), 2, "a stop births no more");
        system.update(moment(200, &spec, &sprite), &mut rng, gone);
        assert_eq!(system.live().count(), 0, "past its window all die");
    }

    #[test]
    fn a_column_births_inside_its_radius_tilted_at_most_its_variance() {
        let spec = Spec {
            initial: 0,
            max: 200,
            shape: 1 << EmitterShape::Area as u32 | 1 << EmitterShape::Column as u32,
            offset: [0.0; 3],
            radius: 0.5,
            radius_variance: 1.0,
            direction_variance: 20.0,
            gravity: [0.0; 3],
            ..fountain()
        };
        let mut rng = Rng(3);
        let tilt = 20.0 * RADIANS;

        for _ in 0..200 {
            let azimuth = rng.unit() * AZIMUTH_SPAN;
            let (out, dir) = launch(&spec, &mut rng, azimuth);
            let across = out[0].hypot(out[1]);

            assert!(dir[2] >= tilt.cos() - 1e-6, "tilted {dir:?}");
            assert!(across <= 0.5 + tilt.sin() + 1e-6);
            assert!((0.0..=1.0).contains(&out[2]));
        }
    }

    #[test]
    fn the_particle_rng_is_an_lcg_with_no_scramble() {
        let mut rng = Rng(0);

        assert_eq!(rng.next(), ADDEND & WHOLE);
        assert_eq!(
            rng.next(),
            ADDEND.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND) & WHOLE
        );
    }

    #[test]
    fn the_ramps_run_birth_to_middle_to_death() {
        let spec = Spec {
            colours: [[0.0; 4], [0.0, 0.0, 0.0, 1.0], [0.0; 4]],
            fade_time: (0.1, 0.9),
            sizes: [1.0, 3.0, 1.0],
            size_time: (0.5, 0.5),
            ..fountain()
        };
        let p = Particle {
            at: [0.0; 3],
            velocity: [0.0; 3],
            born: 10.0,
            life: 4.0,
            scale: 1.0,
            slot: 0,
            frame: 0,
            stepped: 0,
            frame_ms: 0,
        };

        assert!(near(p.colour(&spec, 10.2)[3], 0.5), "half way in");
        assert!(near(p.colour(&spec, 10.4)[3], 1.0), "faded in");
        assert!(near(p.colour(&spec, 12.2)[3], 0.5), "half way out");
        assert!(near(p.size(&spec, 11.0), 2.0));
        assert!(near(p.size(&spec, 12.0), 3.0));
    }
}
