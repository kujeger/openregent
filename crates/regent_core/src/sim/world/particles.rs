//! What a particle system's unit (`super::plume`) tells the particle
//! maths ([`crate::view::map::particles`]): its descriptor's [`Spec`], the art
//! its particles draw ([`Sprite`]), and each frame's pass ([`Sweep`]).
//! **Binary** unless marked: `CY3D_ParticleSystem` in `MajestyHD.exe`.
//!
//! No rule reads a particle: a system's unit ends by its window and
//! emitter (`super::plume`), not its particles, and they draw on their
//! own clock-seeded RNG. So the particles themselves are not the
//! world's, nor in its save: a front end steps them from the pass the
//! world records each frame.
//!
//! ```text
//!  descriptor ──> Spec, Sprite (sim::setup, per body)
//!  plume_step ──> Sweep: game ms; per standing system, last made last:
//!                 handle, code, when made, its unit's place, its window
//!                 ──> view::map::particles::Sparks::step
//! ```

use crate::base::fixed::Fixed;
use crate::data::desc::{EmitterShape, ParticleSystem};
use crate::data::fourcc::FourCc;
use crate::data::imag::Image;

/// Particle step min: the shortest step, s; a step of 0 takes it.
pub const STEP_MIN: f32 = 1e-5;

/// Game ms in a second: `fdiv` by scroll ramp consts, 1000.0.
const MS_PER_S: f64 = 1000.0;

/// Particle system init's box when both corners are within `1e-5` of the
/// origin: -10, -10, 0 to 10, 10, 10.
pub const DEFAULT_BOUNDS: ([f32; 3], [f32; 3]) = ([-10.0, -10.0, 0.0], [10.0, 10.0, 10.0]);

/// Particle system init's clamp on `DirectionVariance`, degrees.
pub const DIRECTION_MAX: f32 = 90.0;

/// What the engine reads of a `ParticleSystem` block, as it holds it
/// after particle system init's clamps. Compared by bits.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    /// `InitialParticles`: born on the first update.
    pub initial: u32,
    /// `MaxParticles`: the slots; a birth needs a free one.
    pub max: u32,
    /// `Bounds` about the emitter: a particle outside dies.
    pub bounds: ([f32; 3], [f32; 3]),
    /// `Emitter` `Type` bits ([`EmitterShape`]).
    pub shape: u32,
    /// `Offset`, `Velocity`, `Acceleration`: the emitter's place about
    /// its unit, moved each update (in-memory emitter).
    pub offset: [f32; 3],
    pub velocity: [f32; 3],
    pub acceleration: [f32; 3],
    /// `Radius`, and `RadiusVariance` clamped to 0..1: an `Area` birth's
    /// distance out, `Radius × (1 − variance)` to `Radius`.
    pub radius: f32,
    pub radius_variance: f32,
    /// `DirectionVariance`, clamped to 0..90 degrees: the tilt off `+z`.
    pub direction_variance: f32,
    /// `Rate`, births a second.
    pub rate: f32,
    /// `Recycle`: a reborn particle keeps its sprite slot and frame.
    pub recycle: bool,
    /// The particle's `Lifespan`, s (in-memory particle).
    pub life: f32,
    /// `LifespanVariance`, clamped to 0..1: life `× (1 − v × draw)`.
    pub life_variance: f32,
    /// `InitialSpeed`, and `InitialSpeedVariance` clamped to 0..1.
    pub speed: f32,
    pub speed_variance: f32,
    /// `Gravity`: added to velocity each second.
    pub gravity: [f32; 3],
    /// `JitterDisplacement`: up to this added to the place each update.
    pub jitter: [f32; 3],
    /// `JitterOrientation`: its x and y scale two draws an update whose
    /// results nothing keeps.
    pub turn_jitter: [f32; 3],
    /// `BirthColor`, `MidlifeColor`, `DeathColor`.
    pub colours: [[f32; 4]; 3],
    /// `BirthSize`, `MaxSize`, `DeathSize`.
    pub sizes: [f32; 3],
    /// `SizeTime`, `FadeTime` in and out, and the spin's (block `0x100`,
    /// `0x104`), shares of life clamped to 0..1.
    pub size_time: (f32, f32),
    pub fade_time: (f32, f32),
    pub spin_time: (f32, f32),
    /// `MidlifeSpinRate`: the turn grows by it times `2 pi` a second
    /// while the spin runs; nothing keeps the turn. Birth and death rates
    /// are read by nothing.
    pub spin: f32,
}

impl PartialEq for Spec {
    fn eq(&self, other: &Spec) -> bool {
        self.bits() == other.bits()
    }
}

impl Eq for Spec {}

impl Spec {
    /// `system`'s, as particle system init holds it: variances clamped, a box
    /// at the origin replaced by [`DEFAULT_BOUNDS`].
    #[must_use]
    pub fn of(system: &ParticleSystem<'_>) -> Option<Spec> {
        let (emitter, particle) = (system.emitter(), system.particle());
        let unit = |v: f32| v.clamp(0.0, 1.0);
        let pair = |(a, b): (f32, f32)| (unit(a), unit(b));
        let v3 = |(x, y, z): (f32, f32, f32)| [x, y, z];
        let c4 = |(r, g, b, a): (f32, f32, f32, f32)| [r, g, b, a];
        let (low, high) = system.bounds()?;
        let tiny = |v: [f32; 3]| length(v) < STEP_MIN;
        let bounds = if tiny(v3(low)) && tiny(v3(high)) {
            DEFAULT_BOUNDS
        } else {
            (v3(low), v3(high))
        };

        #[allow(clippy::cast_possible_truncation, reason = "a lifespan in ms")]
        let life = (f64::from(particle.lifespan()?) / MS_PER_S) as f32;

        Some(Spec {
            initial: system.initial_particles()?,
            max: system.max_particles()?,
            bounds,
            shape: emitter.shape_bits()?,
            offset: v3(emitter.offset()?),
            velocity: v3(emitter.velocity()?),
            acceleration: v3(emitter.acceleration()?),
            radius: emitter.radius()?,
            radius_variance: unit(emitter.radius_variance()?),
            direction_variance: emitter.direction_variance()?.clamp(0.0, DIRECTION_MAX),
            rate: emitter.rate()?,
            recycle: emitter.recycle()? != 0,
            life,
            life_variance: unit(particle.lifespan_variance()?),
            speed: particle.initial_speed()?,
            speed_variance: unit(particle.initial_speed_variance()?),
            gravity: v3(particle.gravity()?),
            jitter: v3(particle.jitter_displacement()?),
            turn_jitter: v3(particle.jitter_orientation()?),
            colours: [
                c4(particle.birth_colour()?),
                c4(particle.midlife_colour()?),
                c4(particle.death_colour()?),
            ],
            sizes: [
                particle.birth_size()?,
                particle.max_size()?,
                particle.death_size()?,
            ],
            size_time: pair(particle.size_time()?),
            fade_time: pair(particle.fade_time()?),
            spin_time: pair(particle.spin_time()?),
            spin: particle.midlife_spin_rate()?,
        })
    }

    /// Whether its emitter's `Type` has `shape`.
    #[must_use]
    pub const fn has(&self, shape: EmitterShape) -> bool {
        self.shape >> shape as u32 & 1 == 1
    }

    /// Every float's bits and every word, for equality.
    fn bits(&self) -> Vec<u32> {
        let floats: [&[f32]; 15] = [
            &self.bounds.0,
            &self.bounds.1,
            &self.offset,
            &self.velocity,
            &self.acceleration,
            &[self.radius, self.radius_variance, self.direction_variance],
            &[self.rate, self.life, self.life_variance, self.speed],
            &[self.speed_variance, self.spin],
            &self.gravity,
            &self.jitter,
            &self.turn_jitter,
            self.colours.as_flattened(),
            &self.sizes,
            &[self.size_time.0, self.size_time.1, self.fade_time.0],
            &[self.fade_time.1, self.spin_time.0, self.spin_time.1],
        ];

        [self.initial, self.max, self.shape, u32::from(self.recycle)]
            .into_iter()
            .chain(floats.into_iter().flatten().map(|f| f.to_bits()))
            .collect()
    }
}

/// What a particle draws: frames of the system's own art, in the set its
/// unit plays.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sprite {
    /// The set, whole id.
    pub set: u32,
    /// Per slot: its frames, and the game ms each shows, 0 for none.
    pub slots: Vec<(u16, u32)>,
}

impl Sprite {
    /// Set `set` of `image`: a slot's frame time is its own playback's
    /// word 2, else its set's, where it has more than one frame.
    #[must_use]
    pub fn of(image: &Image<'_>, set: u32) -> Sprite {
        let found = image.sets.iter().find(|s| s.id == set);
        let shared = found
            .and_then(crate::data::imag::Set::playback)
            .map_or(0, |p| p.value);
        let slots = found
            .map(|s| {
                s.slots
                    .iter()
                    .map(|slot| {
                        let own = slot.playback().map_or(0, |p| p.value);
                        let ms = if own == 0 { shared } else { own };

                        (slot.frames, if slot.frames > 1 { ms } else { 0 })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Sprite { set, slots }
    }
}

/// Whether a system is still emitting, and whether its window is past
/// ([`super::plume::Plume`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub emitting: Emitting,
    pub past: Past,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emitting {
    Yes,
    No,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Past {
    Within,
    Beyond,
}

/// A standing system as the particle manager's pass finds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StandingSystem {
    /// Its unit's handle and descriptor.
    pub handle: u32,
    pub code: FourCc,
    /// When its unit was made, game ms: a new system's clock.
    pub made: u32,
    /// Its unit's place, 16.16 cells and height; `None`
    /// where its holder has none, so it is not updated.
    pub unit: Option<[Fixed; 3]>,
    pub window: Window,
}

/// The particle manager's pass of a frame (particle manager update,
/// after the orders' deletes): its game ms, and each system not yet
/// freed, in the order made; the pass updates them last first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sweep {
    pub at: u32,
    pub sources: Vec<StandingSystem>,
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A point emitter, one particle at the start, straight up at 2 a
    /// second under gravity 4, no variance.
    pub(crate) fn fountain() -> Spec {
        Spec {
            initial: 1,
            max: 2,
            bounds: ([-100.0; 3], [100.0; 3]),
            shape: 1 << EmitterShape::Point as u32,
            offset: [0.5, 0.0, 0.0],
            velocity: [0.0; 3],
            acceleration: [0.0; 3],
            radius: 0.0,
            radius_variance: 0.0,
            direction_variance: 0.0,
            rate: 0.0,
            recycle: false,
            life: 1.0,
            life_variance: 0.0,
            speed: 2.0,
            speed_variance: 0.0,
            gravity: [0.0, 0.0, -4.0],
            jitter: [0.0; 3],
            turn_jitter: [0.0; 3],
            colours: [[0.0; 4]; 3],
            sizes: [1.0; 3],
            size_time: (0.0, 0.0),
            fade_time: (0.0, 0.0),
            spin_time: (0.0, 0.0),
            spin: 0.0,
        }
    }
}
