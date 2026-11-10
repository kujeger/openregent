//! Track 2: an action's turn, then its `InitAction`. **Binary** unless
//! marked; notes in `docs/original/movement.md`.
//!
//! ```text
//!  OrderAction arms: tracks 0 and 2 empty        OrderAction
//!    action face
//!      facing differs ─> Turn on track 2, run at once
//!                        (GE_OrderProcessor); every turn
//!                        rate, turn_step a run; on the direction callback
//!                        0x1008 (action turn done) pushes the InitAction
//!      else           ─> facing set, InitAction at once
//!  InitAction, order 11 (GE_OrderProcessor), run by
//!    OrderAnimate      at once and every `rate` ms: frame 0
//!    of the action's set, a frame a run; the run past the last ends it
//!
//!  waiting for track 2: a MovePath (OrderMovePath), the next Action
//!  the Action's GPL agent: handed to track 2 as the Action ends
//!    (OrderAction); an order holding it skips the agent's threads
//! ```

use super::{Timed, interval, rate};
use crate::data::imag;

/// An `InitAction`'s frame time without a set playback: `GenUnit`
/// returns 100.
pub const INIT_MS: u32 = 100;

/// The rate an order plays a set at, from its playback word 2 (0 none):
/// that, else [`INIT_MS`], rounded to whole frames from a frame up.
/// Both the `InitAction` and an `Animate`
/// (`GE_OrderProcessor`) take it.
const fn set_rate(playback: u32) -> u32 {
    let ms = if playback == 0 { INIT_MS } else { playback };

    rate(ms, 0)
}

/// Game ms each frame of a set shows while an order plays it, from its
/// playback word 2 (0 none): a frame a run (`OrderAnimate`,
/// `OrderAnimate`), re-armed with the set's rate, so at
/// least a game frame. Not `Walk` while walking: a frame a step
/// ([`super::Walker::stride_ms`]).
///
/// ```
/// use regent_core::sim::movement::action::frame_ms;
///
/// assert_eq!(frame_ms(0), 100); // no playback
/// assert_eq!(frame_ms(150), 150);
/// assert_eq!(frame_ms((-50_i32).cast_unsigned()), 50); // rate 1
/// ```
#[must_use]
pub const fn frame_ms(playback: u32) -> u32 {
    interval(set_rate(playback))
}

/// Directions a slot spans at most: `UnitIMGPartEntry` divides 32 by
/// the slot count.
const SPAN: i32 = 32;

/// What an `InitAction` reads of one animation set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pose {
    /// Frames per slot, in slot order: `ImagSlot`, which the part's
    /// frame count reads.
    pub frames: Vec<u16>,
    /// The set's playback word 2, 0 without one: `ImagSet`, word 2,
    /// as `GenUnit` reads it. Not the slot's.
    pub playback: u32,
    /// Its bits of [`FRAME_KEEPING_BITS`]: a change from a set sharing one
    /// keeps the frame, so the `InitAction` may not start on frame 0
    /// ([`Phase`]).
    pub keeps: u32,
    /// Per slot, the first frame after 0 whose reference carries
    /// `imag::CUE`: the `InitAction` fires its callback there, not at
    /// its last frame.
    pub cues: Vec<Option<u16>>,
}

impl Pose {
    /// Set `set` as an `InitAction` reads it.
    #[must_use]
    pub fn of(set: &imag::Set<'_>) -> Pose {
        Pose {
            frames: set.slots.iter().map(|s| s.frames).collect(),
            playback: set.playback().map_or(0, |p| p.value),
            keeps: set.layer_flags() & FRAME_KEEPING_BITS,
            cues: set.slots.iter().map(cue).collect(),
        }
    }

    /// Frames of the slot a unit facing `direction` draws; 0 without
    /// slots: an `InitAction` then ends as it is pushed.
    #[must_use]
    pub fn frames_at(&self, direction: i32) -> u32 {
        self.frames
            .get(slot(direction, self.frames.len()))
            .map_or(0, |&f| u32::from(f))
    }

    /// An `InitAction`'s frame time: the set's playback, else [`INIT_MS`],
    /// rounded to whole frames from a frame up (`GSUnit`,
    /// `GSUnit`).
    ///
    /// ```
    /// use regent_core::sim::movement::action::Pose;
    ///
    /// let pose = |playback| Pose { frames: vec![4], playback, keeps: 0, cues: vec![] };
    ///
    /// assert_eq!(pose(0).rate(), 100);
    /// assert_eq!(pose(130).rate(), 150);
    /// assert_eq!(pose(20).rate(), 20);
    /// ```
    #[must_use]
    pub const fn rate(&self) -> u32 {
        set_rate(self.playback)
    }

    /// How long an `InitAction` holds track 2 from its push, facing
    /// `direction`: `frames - 1` runs after the one at its push.
    ///
    /// ```
    /// use regent_core::sim::movement::action::Pose;
    ///
    /// // A peasant's Carry-2: four frames at 100 ms.
    /// let carry = Pose { frames: vec![4; 8], playback: 100, keeps: 0, cues: vec![] };
    ///
    /// assert_eq!(carry.init_ms(12), 300);
    /// assert_eq!(Pose::default().init_ms(0), 0);
    /// ```
    #[must_use]
    pub fn init_ms(&self, direction: i32) -> u32 {
        self.frames_at(direction).saturating_sub(1) * self.rate()
    }

    /// The frame whose run fires an `InitAction`'s callback `0x1000`,
    /// facing `direction`: its cue, else its last (`OrderAnimate`,
    /// `OrderAnimate`).
    #[must_use]
    pub fn peak_frame(&self, direction: i32) -> u32 {
        let last = self.frames_at(direction).saturating_sub(1);
        let cue = self
            .cues
            .get(slot(direction, self.cues.len()))
            .copied()
            .flatten()
            .map(u32::from);

        cue.filter(|c| *c < last).unwrap_or(last)
    }

    /// How long from an `InitAction`'s push, facing `direction`, to its
    /// callback `0x1000`: the push run shows frame 1.
    ///
    /// ```
    /// use regent_core::sim::movement::action::Pose;
    ///
    /// let cast = |cue| Pose { frames: vec![9], cues: vec![cue], ..Pose::default() };
    ///
    /// assert_eq!(cast(None).peak_ms(0), 700);
    /// assert_eq!(cast(Some(4)).peak_ms(0), 300);
    /// ```
    #[must_use]
    pub fn peak_ms(&self, direction: i32) -> u32 {
        self.peak_frame(direction).saturating_sub(1) * self.rate()
    }
}

/// The slot of `slots` a part facing `direction` (0 to 31) draws:
/// rounded to the nearest, wrapping (`UnitIMGPartEntry`,
/// `UnitIMGPartEntry`). A `Directionless` descriptor draws slot `direction`
/// instead, 0 past the last; not modelled.
///
/// ```
/// use regent_core::sim::movement::action::slot;
///
/// assert_eq!(slot(1, 8), 0);
/// assert_eq!(slot(2, 8), 1);
/// assert_eq!(slot(30, 8), 0);
/// assert_eq!(slot(17, 1), 0);
/// ```
#[must_use]
pub fn slot(direction: i32, slots: usize) -> usize {
    let count = i32::try_from(slots).unwrap_or(i32::MAX).max(1);
    let span = (SPAN / count).max(1);
    let at = (span / 2 + direction) / span % count;

    usize::try_from(at).unwrap_or(0)
}

/// The sets an action plays: its `InitAction`'s, `ImageSet`, then its
/// after-set, `CompletionImageSet`, 0 for none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sets {
    pub set: u32,
    pub after: u32,
}

impl Sets {
    /// What the parts show as an `InitAction` of `frames` ends at game
    /// time `now`: the after-set, played from frame 0;
    /// without one, nothing plays, so the last frame stays (**Inferred**).
    /// `None` for a set of no frames: it played nothing.
    ///
    /// ```
    /// use regent_core::sim::movement::action::{Look, Sets};
    ///
    /// let attack = |after| Sets { set: 0x10, after };
    ///
    /// assert_eq!(attack(8).left(6, 900), Some(Look::Plays { set: 8, since: 900 }));
    /// assert_eq!(attack(0).left(6, 900), Some(Look::Frame { set: 0x10, frame: 5 }));
    /// assert_eq!(attack(0).left(0, 900), None);
    /// ```
    #[must_use]
    pub const fn left(self, frames: u32, now: u32) -> Option<Look> {
        match (self.after, frames) {
            (0, 0) => None,
            (0, f) => Some(Look::Frame {
                set: self.set,
                frame: f - 1,
            }),
            (after, _) => Some(Look::Plays {
                set: after,
                since: now,
            }),
        }
    }
}

/// The set a piece shows, for a drawing: a mobile's action, a building's
/// damage stage or fall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// Frame `frame` of set `set`, held: an `InitAction`'s, which steps it
    /// a run, or its last where no after-set plays.
    Frame { set: u32, frame: u32 },
    /// Set `set` from frame 0 at game ms `since`, at its rate: an after-set
    /// (**Inferred**: an `Animate` that loops), or a building's set.
    Plays { set: u32, since: u32 },
}

impl Look {
    /// The set shown.
    #[must_use]
    pub const fn set(self) -> u32 {
        match self {
            Look::Frame { set, .. } | Look::Plays { set, .. } => set,
        }
    }
}

/// What an `InitAction`'s end does to the facing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settle {
    /// The action's after-set is 0: nothing plays, the facing stays.
    Keep,
    /// An after-set plays; first the facing goes back to its slot's first
    /// direction, over this many slots.
    Slot(usize),
}

/// The direction a part facing `direction` takes back from its slot of
/// `slots`: the slot's share of [`PART_TURN`], over `SPAN`, truncated;
/// fewer than 2 slots count as 1 (`UnitIMGPartEntry`,
/// the count by method). One below the slot's centre, so 12 of 8 slots
/// is 11, 9 is 7.
///
/// ```
/// use regent_core::sim::movement::action::settled;
///
/// assert_eq!(settled(12, 8), 11);
/// assert_eq!(settled(9, 8), 7);
/// assert_eq!(settled(1, 8), 0);
/// assert_eq!(settled(20, 1), 0);
/// ```
#[must_use]
pub fn settled(direction: i32, slots: usize) -> i32 {
    let count = i32::try_from(slots).unwrap_or(i32::MAX).max(1);
    let at = i32::try_from(slot(direction, slots)).unwrap_or(0);

    at * PART_TURN / count / SPAN
}

/// A whole turn as `UnitIMGPartEntry` scales a slot back to a
/// direction: 1000, not 1024, so every slot but 0 lands one short of
/// its centre (times 1000).
pub const PART_TURN: i32 = 1000;

/// Track 2's top order.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Top {
    /// The action's turn, `OrderTurn` from `GE_OrderProcessor`; what
    /// the parts play meanwhile.
    Turn { want: i32, pose: Pose, phase: Phase },
    /// The `InitAction` at `frame` of `frames`, every `rate` ms; its
    /// callback fires on frame `fires`.
    Init {
        frame: u32,
        frames: u32,
        rate: u32,
        fires: u32,
    },
}

/// Track 2, while it holds an order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stage {
    order: Timed<Top>,
    /// Whether it holds the Action's GPL agent, whose
    /// threads then do not run (agent threads run skips an agent
    /// with byte set, agent threads run).
    pub held: bool,
    /// What its `InitAction`'s end does to the facing.
    settle: Settle,
}

/// What a run of track 2 leaves.
pub(super) enum Ran {
    On(Stage),
    Ended,
}

/// Whether a run of track 2 reached its `InitAction`'s cue or last frame, where
/// callback `0x1000` launches the
/// action's projectile and dispatches its function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Peak {
    Not,
    Reached,
}

impl Stage {
    /// Track 2 as action face leaves it at game time `now`, the
    /// unit facing `facing`: a turn toward `want` when it differs, else
    /// the `InitAction`; either run at once. `None` for `want` turns
    /// nothing: the actor is its own target.
    pub(super) fn begin(
        facing: &mut i32,
        want: Option<i32>,
        pose: Pose,
        settle: Settle,
        now: u32,
        turn: (i32, u32),
        phase: Phase,
    ) -> (Ran, Peak) {
        let top = match want {
            Some(w) if w != *facing => Top::Turn {
                want: w,
                pose,
                phase,
            },
            _ => init(&pose, *facing, phase, now),
        };
        let stage = Stage {
            order: Timed {
                order: top,
                when: now,
                delta: 0,
            },
            held: false,
            settle,
        };

        stage.run(facing, now, turn)
    }

    /// Whether its top order is still the turn.
    pub(super) const fn turning(&self) -> bool {
        matches!(self.order.order, Top::Turn { .. })
    }

    /// The turn's `InitAction` to start on what `phase` shows then.
    pub(super) const fn restart(&mut self, phase: Phase) {
        if let Top::Turn { phase: p, .. } = &mut self.order.order {
            *p = phase;
        }
    }

    /// Whether it runs at game time `now`.
    pub(super) const fn due(&self, now: u32) -> bool {
        self.order.due(now)
    }

    /// The `InitAction`'s frame on show; `None` while it turns.
    pub(super) const fn frame(&self) -> Option<u32> {
        match self.order.order {
            Top::Init { frame, .. } => Some(frame),
            Top::Turn { .. } => None,
        }
    }

    /// Frames of the set its `InitAction` plays, or will once turned.
    pub(super) fn frames(&self) -> u32 {
        match &self.order.order {
            Top::Init { frames, .. } => *frames,
            Top::Turn { want, pose, .. } => pose.frames_at(*want),
        }
    }

    /// One run at game time `now`; `turn` is the unit's turn step and rate.
    ///
    /// The `InitAction` steps its frame by one a run from the set's frame
    /// 0, so its push run shows frame 1. Reaching its cue, else the last
    /// frame, fires callback `0x1000`; a set of one frame or none fires it
    /// at once. So `cue - 1`, else
    /// `frames - 2`, runs after the push, at least 0.
    pub(super) fn run(self, facing: &mut i32, now: u32, turn: (i32, u32)) -> (Ran, Peak) {
        let (step, turn_rate) = turn;
        let held = self.held;
        let settle = self.settle;
        let at = |order: Top, delta: u32| {
            Ran::On(Stage {
                order: Timed {
                    order,
                    when: now,
                    delta,
                },
                held,
                settle,
            })
        };

        match self.order.order {
            Top::Turn { want, pose, phase } => match super::turn(*facing, want, step) {
                super::Turned::Toward(d) => {
                    *facing = d;
                    (at(Top::Turn { want, pose, phase }, turn_rate), Peak::Not)
                }
                super::Turned::Onto(d) => {
                    // Callback 0x1008: the InitAction, run as it is pushed,
                    // takes the turn's agent.
                    *facing = d;

                    Stage {
                        order: Timed {
                            order: init(&pose, d, phase, now),
                            when: now,
                            delta: 0,
                        },
                        held,
                        settle,
                    }
                    .run(facing, now, turn)
                }
            },
            Top::Init {
                frame,
                frames,
                rate,
                fires,
            } => {
                let shown = frame + 1;
                let peak = if shown == fires || (frame == 0 && frames < 2) {
                    Peak::Reached
                } else {
                    Peak::Not
                };

                if shown < frames {
                    let next = Top::Init {
                        frame: shown,
                        frames,
                        rate,
                        fires,
                    };

                    (at(next, rate), peak)
                } else {
                    // Callback 0x1001, `GenUnit`: an after-set other
                    // than 0 first sets each part back to its slot,
                    // then plays.
                    if let Settle::Slot(slots) = settle {
                        *facing = settled(*facing, slots);
                    }

                    (Ran::Ended, peak)
                }
            }
        }
    }
}

/// An `InitAction` pushed facing `facing` at game time `now`: frame 0, as
/// `GenUnit` leaves it when its set and the one the
/// parts play, `phase`, share no bit of [`FRAME_KEEPING_BITS`];
/// else the frame they show. In HD only
/// `BVm1`'s `Walk`, `Stand` and `Attack` carry one ([`Pose::keeps`]). Its
/// callback fires on the cue, else, started past it, on the last frame,
/// or, started on the last, at the run past it, which ends it.
/// The world pushes it as `PerformAction` runs, at the engine's push.
fn init(pose: &Pose, facing: i32, phase: Phase, now: u32) -> Top {
    let frames = pose.frames_at(facing);
    let frame = if pose.keeps & phase.keeps != 0 {
        phase.frame_at(now).min(frames.saturating_sub(1))
    } else {
        0
    };
    let fires = pose.peak_frame(facing);

    // Started on the last frame, the run past it fires as it ends.
    Top::Init {
        frame,
        frames,
        rate: pose.rate(),
        fires: if fires > frame {
            fires
        } else if frame + 1 >= frames {
            frames
        } else {
            frames.saturating_sub(1)
        },
    }
}

/// The frame a unit's parts show and what steps it: a looping `Animate`
/// of a set (`OrderAnimate`, a frame a run at the set's rate), or
/// nothing. **Inferred**: an `Animate` steps first a rate after its push.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Phase {
    frame: u32,
    when: u32,
    rate: u32,
    frames: u32,
    /// The set's [`FRAME_KEEPING_BITS`].
    pub keeps: u32,
}

impl Phase {
    /// Frame `frame` held: nothing steps it.
    #[must_use]
    pub const fn still(frame: u32, keeps: u32) -> Phase {
        Phase {
            frame,
            when: 0,
            rate: 0,
            frames: 0,
            keeps,
        }
    }

    /// A looping set of `frames` from frame `frame`, pushed at `when`,
    /// stepping every `rate` ms.
    #[must_use]
    pub const fn running(frame: u32, when: u32, rate: u32, frames: u32, keeps: u32) -> Phase {
        Phase {
            frame,
            when,
            rate,
            frames,
            keeps,
        }
    }

    /// A step's frame: one on, wrapping at `frames`, what steps it
    /// unchanged.
    #[must_use]
    pub const fn stepped(&self, frames: u32) -> Phase {
        if frames == 0 {
            *self
        } else {
            Phase {
                frame: (self.frame + 1) % frames,
                ..*self
            }
        }
    }

    /// Another set of `frames`, keeping bits `keeps`, put on the parts at
    /// game time `now` by `UnitIMG`: frame 0, unless the sets share
    /// a frame-keeping bit, then the frame shown. Before any run due at
    /// `now`: an order puts it on track 0, and a looping `Animate` runs
    /// later, on track 1. That `Animate` is left
    /// running and steps the new set's frames at its own rate, wrapping at
    /// their count (`OrderAnimate` reads the part's).
    #[must_use]
    pub const fn put(&self, frames: u32, keeps: u32, now: u32) -> Phase {
        let runs = self.runs_before(now);
        let frame = if self.keeps & keeps == 0 {
            0
        } else if self.rate == 0 || self.frames <= 1 {
            self.frame
        } else {
            (self.frame + runs) % self.frames
        };

        if self.rate == 0 || self.frames <= 1 || frames == 0 {
            Phase::still(frame, keeps)
        } else {
            // From `frame` now, as the runs so far count on from it.
            let base = (frame + frames - runs % frames) % frames;

            Phase::running(base, self.when, self.rate, frames, keeps)
        }
    }

    /// The frame shown at game time `now` before its `Animate`'s run
    /// then: what an order on track 0 finds.
    #[must_use]
    pub const fn frame_before(&self, now: u32) -> u32 {
        if self.rate == 0 || self.frames <= 1 {
            self.frame
        } else {
            (self.frame + self.runs_before(now)) % self.frames
        }
    }

    /// Runs of its `Animate` before game time `now`.
    const fn runs_before(&self, now: u32) -> u32 {
        if self.rate == 0 || now <= self.when {
            0
        } else {
            (now - self.when - 1) / self.rate
        }
    }

    /// The frame shown at game time `now`. A one-frame set steps nothing
    /// (unit play set pushes no `Animate` for a one-frame slot), so
    /// its frame is what steps left.
    #[must_use]
    pub const fn frame_at(&self, now: u32) -> u32 {
        if self.rate == 0 || self.frames <= 1 {
            self.frame
        } else {
            (self.frame + now.saturating_sub(self.when) / self.rate) % self.frames
        }
    }
}

/// Slot `slot`'s cue: the first frame after 0 whose reference carries
/// [`imag::CUE`]. Frame 0 is shown before the first run, which checks the
/// frame it steps onto. **Inferred**: stream 0's frames.
fn cue(slot: &imag::Slot<'_>) -> Option<u16> {
    slot.stream(0)
        .skip(1)
        .position(|f| f.reference & imag::CUE != 0)
        .and_then(|at| u16::try_from(at + 1).ok())
}

/// `ImagSet` bits 17 to 19: a set change keeps the frame when the
/// old and new sets share one.
pub const FRAME_KEEPING_BITS: u32 = 7 << 17;

/// To and from the save model.
mod snap {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Pose {
        type Model = m::Pose;

        fn save(&self) -> m::Pose {
            let Pose {
                frames,
                playback,
                keeps,
                cues,
            } = self;

            m::Pose {
                frames: frames.clone(),
                playback: *playback,
                keeps: *keeps,
                cues: cues.clone(),
            }
        }

        fn load(model: m::Pose) -> Result<Pose, Refused> {
            Ok(Pose {
                frames: model.frames,
                playback: model.playback,
                keeps: model.keeps,
                cues: model.cues,
            })
        }
    }

    impl Saved for Sets {
        type Model = m::Sets;

        fn save(&self) -> m::Sets {
            m::Sets {
                set: self.set,
                after: self.after,
            }
        }

        fn load(model: m::Sets) -> Result<Sets, Refused> {
            Ok(Sets {
                set: model.set,
                after: model.after,
            })
        }
    }

    impl Saved for Look {
        type Model = m::Look;

        fn save(&self) -> m::Look {
            match *self {
                Look::Frame { set, frame } => m::Look::Frame { set, frame },
                Look::Plays { set, since } => m::Look::Plays { set, since },
            }
        }

        fn load(model: m::Look) -> Result<Look, Refused> {
            Ok(match model {
                m::Look::Frame { set, frame } => Look::Frame { set, frame },
                m::Look::Plays { set, since } => Look::Plays { set, since },
            })
        }
    }

    impl Saved for Settle {
        type Model = m::Settle;

        fn save(&self) -> m::Settle {
            match self {
                Settle::Keep => m::Settle::Keep,
                Settle::Slot(s) => m::Settle::Slot(s.save()),
            }
        }

        fn load(model: m::Settle) -> Result<Settle, Refused> {
            Ok(match model {
                m::Settle::Keep => Settle::Keep,
                m::Settle::Slot(s) => Settle::Slot(usize::load(s)?),
            })
        }
    }

    impl Saved for Phase {
        type Model = m::Phase;

        fn save(&self) -> m::Phase {
            let Phase {
                frame,
                when,
                rate,
                frames,
                keeps,
            } = *self;

            m::Phase {
                frame,
                when,
                rate,
                frames,
                keeps,
            }
        }

        fn load(model: m::Phase) -> Result<Phase, Refused> {
            Ok(Phase {
                frame: model.frame,
                when: model.when,
                rate: model.rate,
                frames: model.frames,
                keeps: model.keeps,
            })
        }
    }

    impl Saved for Top {
        type Model = m::Act;

        fn save(&self) -> m::Act {
            match self {
                Top::Turn { want, pose, phase } => m::Act::Turn {
                    want: *want,
                    pose: pose.save(),
                    phase: phase.save(),
                },
                Top::Init {
                    frame,
                    frames,
                    rate,
                    fires,
                } => m::Act::Init {
                    frame: *frame,
                    frames: *frames,
                    rate: *rate,
                    fires: *fires,
                },
            }
        }

        fn load(model: m::Act) -> Result<Top, Refused> {
            Ok(match model {
                m::Act::Turn { want, pose, phase } => Top::Turn {
                    want,
                    pose: Pose::load(pose)?,
                    phase: Phase::load(phase)?,
                },
                m::Act::Init {
                    frame,
                    frames,
                    rate,
                    fires,
                } => Top::Init {
                    frame,
                    frames,
                    rate,
                    fires,
                },
            })
        }
    }

    impl Saved for Stage {
        type Model = m::Stage;

        fn save(&self) -> m::Stage {
            let Stage {
                order,
                held,
                settle,
            } = self;

            m::Stage {
                order: order.order.save(),
                when: order.when,
                delta: order.delta,
                held: *held,
                settle: settle.save(),
            }
        }

        fn load(model: m::Stage) -> Result<Stage, Refused> {
            Ok(Stage {
                order: Timed {
                    order: Top::load(model.order)?,
                    when: model.when,
                    delta: model.delta,
                },
                held: model.held,
                settle: Settle::load(model.settle)?,
            })
        }
    }
}
