//! The earthquake: a shake of the view, nothing more. **Binary** unless
//! marked.
//!
//! ```text
//!  StartEarthquakeSpell(unit, min, max, [ms])
//!    no unit: throw; GS_OrderProcessor (unit, ms or -1, min, max, 0)
//!  GS_OrderProcessor
//!    GS_Order_Earthquake on track 1, kind 0x4008, run on push, rate 50;
//!    runs ms / 50, or ms itself where not above 0 (-1 endless)
//!  GS_Order_Earthquake, each run
//!    views 1-4 follow none; views 1-4 and 0 step min to max pixels
//!    each way from where they are (the paint RNG, the clock's stream)
//!    runs - 1 below 1 ends it; -1 never does
//!  StopEarthquakeSpell(unit)
//!    no unit: throw; the order off track 1 (1, 0x4008)
//! ```
//!
//! The damage is the scripts': `Earthquake_Active` hits buildings and
//! lairs on a thread; `Earthquake_Unit_Created` asks a shake of 5 to 30
//! pixels for 6,000 ms. The world keeps the order, so the unit stays in
//! ORDR, and each run's [`Shake`] for the view to take.

use super::{Answer, Found, Listed, World, agent_ref, int_ref};
use crate::gpl::vm::{Fault, Handle, Vm};

/// Game ms between runs: `0x32` written in `GS_OrderProcessor`.
pub const QUAKE_MS: u32 = 50;

/// `StartEarthquakeSpell`'s length without a fourth argument: endless
/// (-1 in gpl start earthquake spell).
pub const ENDLESS: i32 = -1;

/// One run's shake of the view, pixels: each axis steps `min` to `max`
/// each way, drawn on a clock-seeded stream ([`Shake::step`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shake {
    pub min: i32,
    pub max: i32,
}

impl Shake {
    /// One axis's step from two draws' worth: `n` drawn below
    /// `2 (max - min)` (`rng_next`, 0 for a bound under 2), less
    /// `max - min`, then `min` more away from 0: `-max` to `-min - 1`, or
    /// `min` to `max - 1`.
    ///
    /// ```
    /// use regent_core::sim::world::quake::Shake;
    ///
    /// let s = Shake { min: 5, max: 30 };
    /// assert_eq!(s.step(0), -30);
    /// assert_eq!(s.step(24), -6);
    /// assert_eq!(s.step(25), 5);
    /// assert_eq!(s.step(49), 29);
    /// ```
    #[must_use]
    pub const fn step(self, n: i32) -> i32 {
        let off = n + self.min - self.max;

        if off < 0 {
            off - self.min
        } else {
            off + self.min
        }
    }

    /// The bound each axis draws below: `2 (max - min)`.
    #[must_use]
    pub const fn bound(self) -> i32 {
        (self.max - self.min) * 2
    }
}

/// `GS_Order_Earthquake`: its unit, its shake, and its runs left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quake {
    unit: u32,
    shake: Shake,
    /// Runs left; [`ENDLESS`] never ends.
    runs: i32,
    /// Its last run, game ms.
    when: u32,
}

impl Quake {
    /// `GS_OrderProcessor`'s count for `ms`: `ms / 50` unsigned
    /// where above 0, else `ms`.
    #[must_use]
    pub const fn runs_of(ms: i32) -> i32 {
        if ms > 0 {
            ms.cast_unsigned().div_euclid(QUAKE_MS).cast_signed()
        } else {
            ms
        }
    }

    /// Its count after a run: whether it lasts.
    const fn count(&mut self) -> bool {
        if self.runs == ENDLESS {
            true
        } else {
            self.runs = self.runs.wrapping_sub(1);
            self.runs >= 1
        }
    }
}

/// Earthquake orders, in the order pushed.
#[derive(Clone, Debug, Default)]
pub(super) struct Quakes(Vec<Quake>);

impl World {
    /// `StartEarthquakeSpell(unit, min, max, [ms])`: throws with no unit;
    /// else a [`Quake`] on it, run as pushed. Returns nothing.
    pub(super) fn start_quake(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Piece(_) => {
                let min = int_ref(vm, args, 2)?;
                let max = int_ref(vm, args, 3)?;
                let ms = if args.len() == 5 {
                    int_ref(vm, args, 4)?
                } else {
                    ENDLESS
                };

                self.push_quake(id, Shake { min, max }, ms);
                Ok(Answer::Given)
            }
            Found::Gone => Err(Fault::throw(
                "Function $StartEarthquakeSpell: unit is null.".to_owned(),
            )),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// `StopEarthquakeSpell(unit)`: throws with no unit; else its quake
    /// gone. Returns nothing.
    pub(super) fn stop_quake(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Piece(_) => {
                self.unquake(id);
                Ok(Answer::Given)
            }
            Found::Gone => Err(Fault::throw(
                "Function $StopEarthquakeSpell: unit is null.".to_owned(),
            )),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// `GS_OrderProcessor` on agent `id`: the order pushed and run
    /// at once (header bit 2); the unit joins the back of ORDR.
    fn push_quake(&mut self, id: u32, shake: Shake, ms: i32) {
        let mut quake = Quake {
            unit: id,
            shake,
            runs: Quake::runs_of(ms),
            when: self.now,
        };

        self.output.shakes.push(shake);

        if quake.count() {
            self.effects.quakes.0.push(quake);

            self.dispatches.list(Listed::Unit(id));
        }
    }

    /// The order `(1, 0x4008)`: the first earthquake order off agent
    /// `id`'s track 1. **Inferred**: the first found, where two run.
    fn unquake(&mut self, id: u32) {
        if let Some(i) = self.effects.quakes.0.iter().position(|q| q.unit == id) {
            self.effects.quakes.0.remove(i);
        }
    }

    /// Agent `id`'s quakes in its ORDR turn: each due shakes and counts.
    pub(super) fn run_quakes(&mut self, id: u32) {
        let now = self.now;
        let mut shakes = Vec::new();

        self.effects.quakes.0.retain_mut(|q| {
            let due = q.unit == id && now.wrapping_sub(q.when) >= QUAKE_MS;

            if due {
                shakes.push(q.shake);
                q.when = now;
                q.count()
            } else {
                true
            }
        });

        self.output.shakes.extend(shakes);
    }

    /// Whether agent `id`'s piece stands with a quake on it: an order, so
    /// it stays in ORDR.
    pub(super) fn quaking(&self, id: u32) -> bool {
        self.pieces.contains(id) && self.effects.quakes.0.iter().any(|q| q.unit == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Earthquake_Unit_Created`'s 6,000 ms: 120 runs, the first as
    /// pushed, the last 5,950 ms on.
    #[test]
    fn an_earthquake_of_six_seconds_shakes_a_hundred_and_twenty_times() {
        let mut world = World::default();

        world.pieces.insert(40, super::super::Piece::default());
        world.now = 1000;
        world.push_quake(40, Shake { min: 5, max: 30 }, 6000);

        while world.quaking(40) {
            world.now += QUAKE_MS;
            world.run_quakes(40);
        }

        assert_eq!(world.take_output().shakes.len(), 120);
        assert_eq!(world.now, 1000 + 119 * QUAKE_MS);
    }

    /// The count: `ms / 50` above 0, else `ms`; -1 endless, 0 one run.
    #[test]
    fn a_quakes_runs_are_its_length_over_fifty() {
        assert_eq!(Quake::runs_of(6000), 120);
        assert_eq!(Quake::runs_of(49), 0);
        assert_eq!(Quake::runs_of(0), 0);
        assert_eq!(Quake::runs_of(ENDLESS), ENDLESS);

        let mut world = World::default();

        world.pieces.insert(40, super::super::Piece::default());
        world.push_quake(40, Shake { min: 5, max: 30 }, 0);

        assert!(!world.quaking(40));
        assert_eq!(world.take_output().shakes.len(), 1);
    }

    /// `StopEarthquakeSpell` takes an endless quake off.
    #[test]
    fn an_endless_quake_runs_until_stopped() {
        let mut world = World::default();

        world.pieces.insert(40, super::super::Piece::default());
        world.push_quake(40, Shake { min: 5, max: 30 }, ENDLESS);

        for _ in 0..500 {
            world.now += QUAKE_MS;
            world.run_quakes(40);
        }

        assert!(world.quaking(40));

        world.unquake(40);

        assert!(!world.quaking(40));
        assert_eq!(world.take_output().shakes.len(), 501);
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Shake {
        type Model = m::Shake;

        fn save(&self) -> m::Shake {
            m::Shake {
                min: self.min,
                max: self.max,
            }
        }

        fn load(model: m::Shake) -> Result<Shake, Refused> {
            Ok(Shake {
                min: model.min,
                max: model.max,
            })
        }
    }

    impl Saved for Quake {
        type Model = m::Quake;

        fn save(&self) -> m::Quake {
            m::Quake {
                unit: self.unit,
                shake: self.shake.save(),
                runs: self.runs,
                when: self.when,
            }
        }

        fn load(model: m::Quake) -> Result<Quake, Refused> {
            Ok(Quake {
                unit: model.unit,
                shake: Shake::load(model.shake)?,
                runs: model.runs,
                when: model.when,
            })
        }
    }

    impl Saved for Quakes {
        type Model = Vec<m::Quake>;

        fn save(&self) -> Self::Model {
            self.0.save()
        }

        fn load(model: Self::Model) -> Result<Quakes, Refused> {
            Vec::load(model).map(Quakes)
        }
    }
}
