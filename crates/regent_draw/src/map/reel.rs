//! The map's clock: which frame of each cycling picture is up.

use bevy::prelude::*;

use regent_core::data::desc::UnitKind;
#[cfg(doc)]
use regent_core::sim::movement::action::frame_ms;

/// Whether a thing of this kind cycles its frames: buildings, scatter and
/// effects do; characters hold their pose, as a save gives position and
/// facing, not stride.
///
/// Kind from the descriptor, not the chunk: `VHCL` also holds spell auras
/// and `Got_Gold`'s coin (`Overlay`).
pub(super) const fn cycles(kind: Option<UnitKind>) -> bool {
    !matches!(kind, Some(UnitKind::Character))
}

/// The map's clock, game ms. The viewer runs it a real ms a game ms, the
/// engine's default pace: a 50 ms frame each `TimePerFrame`, 50 real ms.
/// openregent sets it from its game.
#[derive(Resource, Default)]
pub struct Reel {
    /// Game ms elapsed.
    pub ms: u32,
    /// Real ms not yet counted.
    owed: f32,
}

/// One frame of a cycling picture, hidden unless current.
///
/// Frames differ in size and anchor, so each has its own entity; the clock
/// only writes [`Visibility`].
#[derive(Component, Clone, Copy)]
pub struct Cycle {
    /// Which frame of the stream this entity draws.
    pub(super) index: u16,
    /// How long the stream is.
    pub(super) frames: u16,
    /// The saved frame, where the cycle starts, so objects are out of step.
    pub(super) start: u16,
    /// Game ms each frame shows: [`frame_ms`] of its set.
    pub(super) ms: u32,
    /// Game time `start` is up at.
    pub(super) from: u32,
}

impl Cycle {
    /// Frame `index` of a stream of `frames`, starting at frame `start`,
    /// each shown `ms` game ms.
    pub const fn new(index: u16, frames: u16, start: u16, ms: u32) -> Self {
        Cycle {
            index,
            frames,
            start,
            ms,
            from: 0,
        }
    }

    /// The same, `start` up at game time `from`: a set put on then.
    #[must_use]
    pub const fn from(self, from: u32) -> Self {
        Cycle { from, ..self }
    }

    /// Which frame of the stream this entity draws.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.index
    }

    /// How long the stream is.
    #[must_use]
    pub const fn frames(self) -> u16 {
        self.frames
    }

    /// Whether this frame is the one up at game time `ms`.
    pub fn showing(self, ms: u32) -> bool {
        let frames = u32::from(self.frames).max(1);
        let tick = ms.wrapping_sub(self.from) / self.ms.max(1);
        let now = (u32::from(self.start).wrapping_add(tick)) % frames;

        u32::from(self.index) == now
    }
}

/// Step the map's clock by real time, and show the frame that is up.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn animate(time: Res<Time>, mut reel: ResMut<Reel>, cels: Query<(&Cycle, &mut Visibility)>) {
    reel.owed += time.delta_secs() * 1000.0;

    let whole = reel.owed.floor();

    reel.owed -= whole;

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "one drawn frame's ms: small, not negative"
    )]
    let add = whole as u32;

    reel.ms = reel.ms.wrapping_add(add);
    reveal(reel.ms, cels);
}

/// Show the frame that is up, the clock set elsewhere.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn show(reel: Res<Reel>, cels: Query<(&Cycle, &mut Visibility)>) {
    reveal(reel.ms, cels);
}

/// Show each cel whose frame is up at game time `ms`.
fn reveal(ms: u32, mut cels: Query<(&Cycle, &mut Visibility)>) {
    // Inherited, not Visible: a hidden parent (a held piece) hides its cels.
    for (cycle, mut visibility) in &mut cels {
        let want = if cycle.showing(ms) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        // Only on change, to spare change detection.
        if *visibility != want {
            *visibility = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cycle_opens_on_the_frame_the_save_caught_it_on() {
        let stream = |start, ms| {
            (0..6)
                .filter(|&index| {
                    Cycle {
                        index,
                        frames: 6,
                        start,
                        ms: 100,
                        from: 0,
                    }
                    .showing(ms)
                })
                .collect::<Vec<_>>()
        };

        assert_eq!(stream(4, 0), [4], "the frame the save named");
        assert_eq!(stream(4, 99), [4], "held its set's 100 ms");
        assert_eq!(stream(4, 100), [5]);
        assert_eq!(stream(4, 200), [0], "and round");
        assert_eq!(stream(0, 200), [2]);
    }

    #[test]
    fn two_cycles_of_one_kind_keep_their_own_phase() {
        let up = |start: u16, tick| {
            (0..6)
                .find(|&index| {
                    Cycle {
                        index,
                        frames: 6,
                        start,
                        ms: 1,
                        from: 0,
                    }
                    .showing(tick)
                })
                .expect("one frame is always up")
        };

        for tick in 0..12 {
            assert_ne!(up(1, tick), up(4, tick), "at tick {tick}");
        }
    }

    /// Every corpus object is inside its slot, so this is a guard.
    #[test]
    fn a_start_past_the_stream_still_lands_on_a_frame() {
        let cycle = Cycle {
            index: 1,
            frames: 3,
            start: 100,
            ms: 1,
            from: 0,
        };

        assert!(cycle.showing(0), "100 % 3 is 1");
    }
}
