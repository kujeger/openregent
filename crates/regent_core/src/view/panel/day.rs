//! The top bar's day counter. **Binary** unless marked.
//!
//! `MainFrameController` refreshes it through the day bar's update:
//!
//! ```text
//!  7021  text  GMTX 24 "%d Day%s", GMTX 25 "s" unless 1
//!  2020  bar   seconds into the day, as 0 to 100
//! ```
//!
//! Both read the timer at the game object's, which
//! Game start bases on 0 and game frame step rebases on
//! Game clock each frame. **Inferred**: it is
//! the game clock, since no code moves its offset (no call of timer shift
//! found). **Corpus**: every save's default name, "N Days", is
//! [`Day::of`] its `GSGL` clock.

use crate::data::text::{Ref, Text, printf};

/// A day's length in game ms.
///
/// Day text and day percent divide the clock by it, as a
/// multiply by `0x45e7b273` and a shift of 46, and
/// day percent takes the remainder with it.
pub const DAY_MS: u32 = 60_000;

/// The day's text, `%d Day%s`.
pub const DAY_TEXT: Ref = Ref::gmtx(24);

/// The plural ending, `s`, for any day but 1.
pub const PLURAL: Ref = Ref::gmtx(25);

/// The text's control, `SMNU` key 6 on `APMK`.
pub const TEXT_CONTROL: i32 = 7021;

/// The bar's control, `SMNU` key 6 on `APMK`.
pub const BAR_CONTROL: i32 = 2020;

/// The bar's full mark: `APMK` gives control 2020 range 0 to 100.
pub const FULL: u32 = 100;

const MS_PER_SECOND: u32 = 1000;
const SECONDS_PER_DAY: u32 = DAY_MS / MS_PER_SECOND;

/// What the counter shows at a game clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Day {
    /// Whole days passed: 0 at the start.
    pub number: u32,
    /// Whole seconds into the day, out of [`FULL`]: 0 to 98, truncated.
    pub percent: u32,
}

impl Day {
    /// The counter at `clock` ms.
    #[must_use]
    pub const fn of(clock: u32) -> Self {
        let seconds = clock % DAY_MS / MS_PER_SECOND;

        Day {
            number: clock / DAY_MS,
            percent: seconds * FULL / SECONDS_PER_DAY,
        }
    }

    /// `20 Days`, `1 Day`: [`DAY_TEXT`] with the number and, unless it is
    /// 1, [`PLURAL`].
    #[must_use]
    pub fn text(&self, text: &Text) -> Option<String> {
        self.text_by(&|index| text.gmtx(index).map(std::borrow::Cow::into_owned))
    }

    /// [`Day::text`] with `gmtx` looking up a `GMTX` string by index.
    #[must_use]
    pub fn text_by(&self, gmtx: &dyn Fn(usize) -> Option<String>) -> Option<String> {
        let ending = if self.number == 1 {
            String::new()
        } else {
            gmtx(PLURAL.index)?
        };

        Some(printf(
            &gmtx(DAY_TEXT.index)?,
            &[self.number.into(), (&ending).into()],
        ))
    }

    /// How full the bar is, 0 to 1.
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "at most 100")]
    pub fn filled(&self) -> f32 {
        self.percent as f32 / FULL as f32
    }
}

/// How often `MainFrameController` refreshes the counter,
/// game ms: times 5000 in `MainFrameController`.
pub const REFRESH_MS: u32 = 5000;

/// How often it writes the frame rate instead, game ms: times 1000 at
/// `MainFrameController`.
pub const RATE_MS: u32 = 1000;

/// `FRAME IT`'s frame rate in the counter's place (frame rate on,
/// frame rate shown).
///
/// ```text
///  the cheat, anyone's    on; shown toggled
///  a click on 7021, on    shown toggled; hidden, text emptied MainFrameController
///  each REFRESH_MS        the day again                       MainFrameController
///  each RATE_MS, shown    "%.2f" of the rate, after the day   MainFrameController
/// ```
///
/// Every peer runs the cheat, whoever sent it: its case reads no player.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frames {
    on: bool,
    shown: bool,
    /// What control 7021 holds instead of the day, until the next
    /// refresh.
    text: Option<String>,
}

impl Frames {
    /// `FRAME IT` ran.
    pub const fn cheat(&mut self) {
        self.on = true;
        self.shown = !self.shown;
    }

    /// The counter clicked: nothing before the first cheat.
    pub fn click(&mut self) {
        if self.on {
            self.shown = !self.shown;

            if !self.shown {
                self.text = Some(String::new());
            }
        }
    }

    /// The clock moved from `from` to `to`, game ms, `rate` frames a
    /// second: each refresh passed puts the day back, each whole second
    /// writes the rate while shown.
    pub fn tick(&mut self, (from, to): (u32, u32), rate: f32) {
        let passed = |every: u32| to / every != from / every;

        if passed(REFRESH_MS) {
            self.text = None;
        }

        if self.on && self.shown && passed(RATE_MS) {
            self.text = Some(format!("{rate:.2}"));
        }
    }

    /// What the counter shows instead of the day.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

/// The frame rate game frame step measures: frames
/// counted and real ms summed, both read and restarted once over a
/// second.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Meter {
    frames: u32,
    ms: u32,
    rate: f32,
}

impl Meter {
    /// `frames` more run in `ms` real ms.
    pub fn step(&mut self, frames: u32, ms: u32) {
        self.frames = self.frames.saturating_add(frames);
        self.ms = self.ms.saturating_add(ms);

        if self.ms > RATE_MS {
            #[allow(clippy::cast_precision_loss, reason = "frames in a second or so")]
            let rate = self.frames as f32 * RATE_MS as f32 / self.ms as f32;

            *self = Meter {
                rate,
                ..Meter::default()
            };
        }
    }

    /// Frames a second, as last read; 0 before.
    #[must_use]
    pub const fn rate(&self) -> f32 {
        self.rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_starts_at_zero_and_turns_every_minute() {
        assert_eq!(
            Day::of(0),
            Day {
                number: 0,
                percent: 0
            }
        );
        assert_eq!(Day::of(59_999).number, 0);
        assert_eq!(
            Day::of(60_000),
            Day {
                number: 1,
                percent: 0
            }
        );
        assert_eq!(Day::of(62_100).number, 1);
    }

    #[test]
    fn the_bar_counts_whole_seconds() {
        assert_eq!(Day::of(999).percent, 0);
        assert_eq!(Day::of(30_000).percent, 50);
        assert_eq!(Day::of(59_999).percent, 98);
        assert!((Day::of(30_000).filled() - 0.5).abs() < 1e-6);
    }

    /// `FRAME IT` shows the rate at the next whole second; a second run
    /// leaves the last rate up until the refresh puts the day back.
    #[test]
    fn frame_it_writes_the_rate_each_second_until_run_again() {
        let mut frames = Frames::default();

        frames.cheat();
        frames.tick((900, 950), 20.0);
        assert_eq!(frames.text(), None);

        frames.tick((950, 1000), 19.5);
        assert_eq!(frames.text(), Some("19.50"));

        frames.cheat();
        frames.tick((1950, 2000), 18.0);
        assert_eq!(frames.text(), Some("19.50"));

        frames.tick((4950, 5000), 18.0);
        assert_eq!(frames.text(), None);
    }

    /// The refresh and the rate fall on one frame: the rate wins, as
    /// `MainFrameController` writes it after the day.
    #[test]
    fn the_rate_is_written_after_the_refresh() {
        let mut frames = Frames::default();

        frames.cheat();
        frames.tick((4950, 5000), 20.0);
        assert_eq!(frames.text(), Some("20.00"));
    }

    /// A click does nothing before the cheat; after, it hides the rate
    /// with an empty counter, and shows it again.
    #[test]
    fn a_click_toggles_the_rate_once_frame_it_ran() {
        let mut frames = Frames::default();

        frames.click();
        frames.tick((950, 1000), 20.0);
        assert_eq!(frames.text(), None);

        frames.cheat();
        frames.click();
        assert_eq!(frames.text(), Some(""));

        frames.click();
        frames.tick((1950, 2000), 20.0);
        assert_eq!(frames.text(), Some("20.00"));
    }

    /// The meter reads frames over ms once past a second, then restarts.
    #[test]
    fn the_meter_reads_once_past_a_second() {
        let mut meter = Meter::default();

        meter.step(20, 1000);
        assert!(meter.rate().abs() < f32::EPSILON);

        meter.step(1, 50);
        assert!((meter.rate() - 20.0).abs() < 1e-3);

        meter.step(10, 1001);
        assert!((meter.rate() - 10_000.0 / 1001.0).abs() < 1e-3);
    }
}
