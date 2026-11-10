//! Minimap marks: what `MiniMapAnimation` makes, a unit only the minimap
//! draws, flashing its set 300 twice and then deleted. **Binary** unless
//! marked.
//!
//! ```text
//!  MiniMapAnimation(agent | coord, name)
//!    a coord: owner -1; an agent: its unit's unit_pos32, its owner
//!    GE_Level makes the unit (kind 1, the code, x, y, owner, 0x40): next handle,
//!      no agent, no place search, no world draw, no sound
//!    set play 0x40: no such set on a mark, nothing
//!    flag_wave_order(unit, 0)
//!      set 300 absent or empty: no order, the unit stays
//!      OrderCounter (kind 12) on track 1, run on push:
//!        start 0, end frames - 1, loop count 2, info 0,
//!        rate the set's playback word 2 (none or 0: 100), callback 0x2002
//!  OrderCounter, each run
//!    callback 0x2002 mode 0: the unit's frame = the counter, the frame the
//!      minimap draws (wave callback)
//!    at end: loop count - 1; at 0 the order ends; else back to start
//!    else the counter steps on
//!  the end: callback 0x2002 mode 2, info bit 2 clear: GenUnit queues the
//!    deferred delete, drained after the frame's orders
//! ```
//!
//! So a mark shows frames 0 to n − 1 twice, a frame a run, and goes in
//! the run that reports the second last frame: it stands
//! `(2n − 1) × interval(rate)` game ms. Message and reward flags pass
//! info 2, which holds the loop count at 2: their set 300 loops for good.
//!
//! The minimap draws a unit
//! whose art has set 300 as that set's frame unit at its point,
//! else as a dot. It shows a unit seen by the view's players,
//! not held, not hidden by `ShowMinimap`; a
//! kind 1 unit only while the minimap's unit filter, bit 3 of
//! Minimap filter, is on. A mark's descriptor
//! is `NotVisibleInISOView`: the main view never draws it.
//!
//! `tests/marks.rs` runs the builtin. Marks held in saves, each with its counter
//! in `ORDR` as above: **Inferred** here, the evidence is in
//! `local/archive/linux-port.md`.

use super::{Answer, Found, Sight, World, agent_ref, bodies::Body, string_ref};
use crate::data::fourcc::FourCc;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::movement;

/// The set a minimap mark flashes and the minimap draws: `0x12c`
/// in flag wave order and
/// `GSOverheadGameView`.
pub const MINIMAP_SET: u32 = crate::data::imag::SetKind::Minimap.id();

/// A mark's frame time where its set gives no playback (`0x64`
/// in flag wave order).
pub const MARK_RATE: u32 = 100;

/// How often a mark plays its set: the loop count flag wave order
/// gives the counter (2, in flag wave order).
pub const MARK_PASSES: u32 = 2;

/// The owner a mark made at a coordinate takes (-1 in
/// gpl mini map animation): none.
pub const NO_OWNER: i32 = -1;

/// How a mark's set plays: the counter flag wave order pushes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wave {
    /// Frames in set [`MINIMAP_SET`]'s first slot (slot 0).
    pub frames: u32,
    /// Game ms between the counter's runs: the set's rate, else
    /// [`MARK_RATE`], rounded up to whole game frames, as the order
    /// re-arms at the run's clock.
    pub interval: u32,
}

impl Wave {
    /// The wave of a set of `frames` frames and playback word 2
    /// `playback` (0 for none); `None` without frames, which makes no
    /// order.
    ///
    /// ```
    /// use regent_core::sim::world::minimap_mark::Wave;
    ///
    /// assert_eq!(Wave::of(20, 0), Some(Wave { frames: 20, interval: 100 }));
    /// assert_eq!(Wave::of(10, 150), Some(Wave { frames: 10, interval: 150 }));
    /// assert_eq!(Wave::of(0, 100), None);
    /// ```
    #[must_use]
    pub const fn of(frames: u32, playback: u32) -> Option<Wave> {
        let rate = if playback == 0 { MARK_RATE } else { playback };

        if frames == 0 {
            None
        } else {
            Some(Wave {
                frames,
                interval: movement::interval(rate),
            })
        }
    }

    /// The wave of `body`'s set [`MINIMAP_SET`], its first slot's
    /// frames; `None` without it.
    #[must_use]
    pub fn of_body(body: &Body) -> Option<Wave> {
        let pose = body.poses.as_ref()?.get(&MINIMAP_SET)?;
        let frames = pose.frames.first().copied().map_or(0, u32::from);

        Wave::of(frames, pose.playback)
    }

    /// How long a mark of this wave stands: [`MARK_PASSES`] passes, less
    /// the run that ends it.
    ///
    /// ```
    /// use regent_core::sim::world::minimap_mark::Wave;
    ///
    /// // hero_death: 20 frames at 100 ms.
    /// assert_eq!(Wave { frames: 20, interval: 100 }.lasts(), 3_900);
    /// ```
    #[must_use]
    pub const fn lasts(&self) -> u32 {
        (MARK_PASSES * self.frames - 1) * self.interval
    }

    /// The frame shown `since` game ms after the push: a frame a run,
    /// from 0, wrapping at the end.
    #[must_use]
    pub const fn frame(&self, since: u32) -> u32 {
        (since / self.interval) % self.frames
    }

    /// A looping counter `since` game ms after its push, as `ORDR` saves
    /// it: info 2, a message or reward flag's. Run `k` shows frame `k`
    /// and leaves the next; the end goes back to 0, its loop count set to
    /// 2 and less one: 2 before the first wrap,
    /// then 1 for good.
    ///
    /// ```
    /// use regent_core::sim::world::minimap_mark::{Counted, Wave};
    ///
    /// let w = Wave { frames: 8, interval: 100 };
    ///
    /// assert_eq!(w.looped(0), Counted { last: 0, counter: 1, loops: 2 });
    /// assert_eq!(w.looped(750), Counted { last: 700, counter: 0, loops: 1 });
    /// ```
    #[must_use]
    pub const fn looped(&self, since: u32) -> Counted {
        let runs = since / self.interval + 1;

        Counted {
            last: (runs - 1) * self.interval,
            counter: runs % self.frames,
            loops: if runs < self.frames { MARK_PASSES } else { 1 },
        }
    }
}

/// An `OrderCounter` as `ORDR` saves it: its header's `when` and its
/// last two words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counted {
    /// Game ms from the push to the last run.
    pub last: u32,
    /// The frame the next run shows.
    pub counter: u32,
    /// The loop count.
    pub loops: u32,
}

/// A minimap mark standing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    /// Its unit handle.
    pub handle: u32,
    /// Its descriptor: `MVa1` `event_beacon`, `MVa2` `hero_death`,
    /// `MVa4` `treasure_find` in the shipped scripts.
    pub code: FourCc,
    /// Unit owner: the agent's owner, [`NO_OWNER`] at a coordinate.
    pub owner: i32,
    /// Where it stands, 32nds of a cell; it never moves.
    pub at: (i32, i32),
    /// Game ms it was made: its set plays from here.
    pub made: u32,
    /// Its set's counter; `None` where the art has no set
    /// [`MINIMAP_SET`] frame: it then stands for good.
    pub wave: Option<Wave>,
    /// Who may see it: `NotVisibleToOpponent` (`hero_death`,
    /// `treasure_find`) its owner alone.
    pub sight: Sight,
}

impl Mark {
    /// The game ms it is deleted, after that frame's orders; `None` for
    /// never.
    #[must_use]
    pub fn ends(&self) -> Option<u32> {
        self.wave.map(|w| self.made + w.lasts())
    }

    /// Whether it stands at game ms `now`, as drawn after that frame.
    #[must_use]
    pub fn stands(&self, now: u32) -> bool {
        self.ends().is_none_or(|end| now < end)
    }

    /// The set [`MINIMAP_SET`] frame the minimap draws at game ms `now`:
    /// unit. `None` without the set.
    #[must_use]
    pub fn frame(&self, now: u32) -> Option<u32> {
        self.wave.map(|w| w.frame(now.saturating_sub(self.made)))
    }
}

/// Minimap marks made, in order.
#[derive(Clone, Debug, Default)]
pub(super) struct Marks(Vec<Mark>);

impl World {
    /// `MiniMapAnimation`'s unit: a mark of the descriptor named `name`
    /// at `at`, 32nds, owned by `owner`; it takes the next handle. No
    /// mark is kept where `at` is unknown or `name` names no unit.
    pub(super) fn raise_mark(&mut self, name: &str, at: Option<(i32, i32)>, owner: i32) {
        self.handle += 1;

        let body = self.descriptors.body_named(name);

        if let (Some((code, body)), Some(at)) = (body, at) {
            self.outbox.marks.0.push(Mark {
                handle: self.handle,
                code,
                owner,
                at,
                made: self.now,
                wave: Wave::of_body(body),
                sight: body.sight,
            });
        }
    }

    /// Marks whose order has ended deleted: after the frame's orders.
    pub(super) fn marks_step(&mut self) {
        let now = self.now;

        self.outbox.marks.0.retain(|m| m.stands(now));
    }

    /// The minimap marks standing, in the order made. Each draws set
    /// [`MINIMAP_SET`]'s frame [`Mark::frame`] at its point, for the
    /// players [`World::mark_seen`] passes.
    #[must_use]
    pub fn minimap_marks(&self) -> Vec<Mark> {
        self.outbox
            .marks
            .0
            .iter()
            .filter(|m| m.stands(self.now))
            .copied()
            .collect()
    }

    /// Whether the players `players` names see `mark`: unit seen level
    /// above 0. Its owner always; another player where its cell is
    /// explored, unless its sight is [`Sight::Owner`].
    #[must_use]
    pub fn mark_seen(&self, mark: &Mark, players: u8) -> bool {
        let owned = u32::try_from(mark.owner).is_ok_and(|o| players & 1u8.wrapping_shl(o) != 0);
        let ground = || super::reveal::level_of(&self.point_words(mark.at), players) > 0;

        owned || (mark.sight == Sight::Everyone && ground())
    }

    /// `MiniMapAnimation(agent | coord, name)`, gpl mini map animation: at a
    /// coordinate, or at an agent's unit (none: nothing); a descriptor by
    /// name, else a throw; then a unit of kind 1 at that point
    /// (`GE_Level`, flags `0x40`): it takes the next handle,
    /// and stands as a minimap mark ([`super::minimap_mark`]).
    pub(super) fn minimap_mark(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let at = vm.arg(args, 1)?;
        let place = if let Value::Coord(x, y) = vm.object(at) {
            Some(Some((Some((*x, *y)), NO_OWNER)))
        } else {
            let id = agent_ref(vm, args, 1)?;

            match self.found(id) {
                Found::Piece(p) => Some(Some((self.position(id), p.owner.cast_signed()))),
                Found::Gone => Some(None),
                Found::Unsure => None,
            }
        };
        let name = string_ref(vm, args, 2)?;

        match place {
            Some(Some(_)) if !self.descriptors.described(&name.to_ascii_lowercase()) => {
                Err(Fault::throw(format!(
                    "MiniMapAnimation( {name} ) could not make unit (could not find description) "
                )))
            }
            Some(Some((at, owner))) => {
                self.raise_mark(&name, at, owner);
                Ok(Answer::Given)
            }
            Some(None) => Ok(Answer::Given),
            None => Ok(Answer::Unknown),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `hero_death`'s wave.
    const HERO_DEATH: Wave = Wave {
        frames: 20,
        interval: 100,
    };

    /// A mark made at 1,000 ms.
    fn mark(wave: Option<Wave>) -> Mark {
        Mark {
            handle: 1,
            code: FourCc(*b"MVa2"),
            owner: 0,
            at: (0, 0),
            made: 1_000,
            wave,
            sight: Sight::Owner,
        }
    }

    /// Two passes of 20 frames: frame 19 at 2,900 ms, frame 0 again at
    /// 3,000; frame 18 the last drawn, at 4,800; gone at 4,900.
    #[test]
    fn a_mark_plays_its_set_twice_and_goes() {
        let m = mark(Some(HERO_DEATH));

        assert_eq!(m.frame(1_000), Some(0));
        assert_eq!(m.frame(1_050), Some(0));
        assert_eq!(m.frame(2_900), Some(19));
        assert_eq!(m.frame(3_000), Some(0));
        assert_eq!(m.frame(4_800), Some(18));
        assert!(m.stands(4_850));
        assert!(!m.stands(4_900));
        assert_eq!(m.ends(), Some(4_900));
    }

    /// A rate under a game frame runs a frame a game frame.
    #[test]
    fn a_short_rate_runs_each_game_frame() {
        assert_eq!(Wave::of(4, 20).map(|w| w.interval), Some(50));
    }

    /// Without set 300 no order is made, and nothing deletes the mark.
    #[test]
    fn a_mark_without_its_set_stands_for_good() {
        let m = mark(None);

        assert!(m.stands(u32::MAX));
        assert_eq!(m.frame(5_000), None);
    }
}

mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Marks {
        type Model = Vec<m::Mark>;

        fn save(&self) -> Self::Model {
            self.0.save()
        }

        fn load(model: Self::Model) -> Result<Marks, Refused> {
            Vec::load(model).map(Marks)
        }
    }
}
