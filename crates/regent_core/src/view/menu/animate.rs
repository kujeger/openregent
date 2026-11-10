//! An interface picture's frames over time: the image widget's
//! playback.

use crate::data::imag::Playback;

/// How an interface picture's frames move.
///
/// **Binary**: anim playback sets it from a
/// [`Playback`] ([`Mode::of`]); with none it keeps the last. A new
/// picture is [`Mode::Still`]. Widget step
/// steps by it ([`Mode::step`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// 0 and 1: no step.
    #[default]
    Still,
    /// 2: forward, past the last to the first.
    Loop,
    /// 4: forward to the last, then stop.
    Forward,
    /// 3: back, past the first to the last.
    LoopBack,
    /// 5: back to the first, then stop.
    Back,
}

/// [`Playback::flags`] bit: loop rather than stop at the end.
const LOOPS: u32 = 1;

impl Mode {
    /// The mode `playback` gives: kind 1 still, 2 forward, 4 back, any
    /// other a loop; `LOOPS` makes 2 and 4 wrap.
    #[must_use]
    pub const fn of(playback: &Playback) -> Mode {
        let loops = playback.flags & LOOPS != 0;

        match (playback.kind, loops) {
            (1, _) => Mode::Still,
            (2, false) => Mode::Forward,
            (4, true) => Mode::LoopBack,
            (4, false) => Mode::Back,
            _ => Mode::Loop,
        }
    }

    /// The frame after `frame` of `count`; `None` where it stays.
    #[must_use]
    pub const fn step(self, frame: usize, count: usize) -> Option<usize> {
        match self {
            _ if count == 0 => None,
            Mode::Loop if frame + 1 >= count => Some(0),
            Mode::Loop => Some(frame + 1),
            Mode::Forward if frame + 1 < count => Some(frame + 1),
            Mode::LoopBack if frame == 0 => Some(count - 1),
            Mode::LoopBack | Mode::Back if frame > 0 => Some(frame - 1),
            _ => None,
        }
    }
}

/// An interface picture's frame and timing: a dialog control's image
/// widget.
///
/// **Binary**: `CYDialogItem` ticks it each pass:
/// past [`Mode::Still`], not stopped, and timer due — a frame
/// period since the last step, timed by `timeGetTime`, no catch-up — it
/// steps; a step that stays stops it until the next
/// [`Animator::play`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Animator {
    pub mode: Mode,
    /// Milliseconds a frame: [`Playback::value`].
    pub period: u32,
    pub frame: usize,
    /// When it last stepped, milliseconds.
    since: u64,
    stopped: bool,
}

impl Animator {
    /// Take `playback`'s mode and period, `None` keeping both, and go on
    /// from the frame shown. The hover callbacks clear the stop with it.
    pub const fn play(&mut self, playback: Option<Playback>) {
        if let Some(p) = playback {
            self.mode = Mode::of(&p);
            self.period = p.value;
        }

        self.stopped = false;
    }

    /// One pass at `now`, milliseconds, the drawn slot `count` frames.
    pub fn tick(&mut self, now: u64, count: usize) {
        let due = now.saturating_sub(self.since) >= u64::from(self.period);

        if self.mode != Mode::Still && !self.stopped && due {
            self.since = now;

            match self.mode.step(self.frame, count) {
                Some(next) => self.frame = next,
                None => self.stopped = true,
            }
        }
    }

    /// The frame drawn of `count`: one past the slot's end goes back to
    /// the first.
    pub const fn shown(&mut self, count: usize) -> usize {
        if self.frame >= count {
            self.frame = 0;
        }

        self.frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playback(kind: u32, flags: u32, value: u32) -> Playback {
        Playback {
            kind,
            flags,
            value,
            rest: [0; 2],
        }
    }

    /// `INkm` 1002's slots: 0 kind 4, 6 kind 2, neither looping; 1000's
    /// both loop.
    #[test]
    fn a_playback_gives_its_mode() {
        assert_eq!(Mode::of(&playback(4, 0, 40)), Mode::Back);
        assert_eq!(Mode::of(&playback(2, 0, 50)), Mode::Forward);
        assert_eq!(Mode::of(&playback(2, 1, 100)), Mode::Loop);
        assert_eq!(Mode::of(&playback(4, 1, 50)), Mode::LoopBack);
        assert_eq!(Mode::of(&playback(1, 0, 50)), Mode::Still);
        assert_eq!(Mode::of(&playback(0, 0, 50)), Mode::Loop);
    }

    #[test]
    fn a_mode_steps_and_stops_at_its_end() {
        assert_eq!(Mode::Forward.step(28, 30), Some(29));
        assert_eq!(Mode::Forward.step(29, 30), None);
        assert_eq!(Mode::Loop.step(29, 30), Some(0));
        assert_eq!(Mode::Back.step(1, 30), Some(0));
        assert_eq!(Mode::Back.step(0, 30), None);
        assert_eq!(Mode::LoopBack.step(0, 30), Some(29));
        assert_eq!(Mode::Still.step(3, 30), None);
        assert_eq!(Mode::Loop.step(0, 0), None);
    }

    /// Lit at 50 ms a frame, 30 frames: the last shows 1450 ms in; let
    /// go there at 40 ms, back to the first 1160 ms later.
    #[test]
    fn a_lit_picture_runs_to_its_end_and_back_by_the_clock() {
        let mut picture = Animator::default();

        picture.play(Some(playback(4, 0, 40)));
        picture.tick(0, 30);
        assert_eq!(picture.frame, 0, "at rest on the first frame");

        picture.play(Some(playback(2, 0, 50)));

        for now in (10..=1450).step_by(10) {
            picture.tick(now, 30);
        }

        assert_eq!(picture.frame, 29);

        picture.tick(5000, 30);
        assert_eq!(picture.frame, 29, "held");

        picture.play(Some(playback(4, 0, 40)));

        for now in (5010..=6170).step_by(10) {
            picture.tick(now, 30);
        }

        assert_eq!(picture.frame, 0);
    }

    #[test]
    fn a_step_waits_its_period_and_does_not_catch_up() {
        let mut picture = Animator::default();

        picture.play(Some(playback(2, 1, 100)));
        picture.tick(100, 58);
        picture.tick(150, 58);
        assert_eq!(picture.frame, 1);

        picture.tick(1000, 58);
        assert_eq!(picture.frame, 2, "one step a pass, however late");

        picture.play(None);
        assert_eq!((picture.mode, picture.period), (Mode::Loop, 100));
    }
}
