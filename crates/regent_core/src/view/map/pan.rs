//! Scrolling the main view: what the player sees of the arrow keys and
//! the window's edges. **Binary** unless marked.
//!
//! ```text
//!  arrow key   a jump of ScrollSpeed px on the next drawn frame, each
//!              key event: once, then at the keyboard's repeat rate
//!              (map keys, key window's WM_KEYDOWN)
//!  edge        each drawn frame in play, the cursor within EDGE px of a
//!              window edge: ScrollSpeed × RATE px a second toward it,
//!              ramping from a fifth to full over RAMP_MS
//!              (edge scroll, scroll ramp)
//! ```
//!
//! Not modelled: the original truncates each frame's step to whole
//! pixels, at least one (about 2 % slower at 60 frames a second).

use std::ops::RangeInclusive;

pub use super::map_keys::SCROLL_SPEED;

/// `ScrollSpeedLow` and `ScrollSpeedHigh`'s file values (scroll speed low,
/// scroll speed high). **Inferred**: the bounds of `Option4MenuController`'s
/// slider for `ScrollSpeed`.
pub const SPEEDS: RangeInclusive<i32> = 32..=64;

/// A cursor less than this far inside a window edge scrolls toward it
/// (a compare with 8, edge scroll).
pub const EDGE: i32 = 8;

/// Past the window's edge, a cursor scrolls only this near it
/// (a compare with -16, edge scroll), unless [`Reach::Any`]. The cursor
/// can leave only a window, not a full screen.
pub const OUTSIDE: i32 = 16;

/// The edge scrolls `ScrollSpeed` times this, pixels a second, at full
/// ramp (times 5, doubled twice, scroll ramp).
pub const RATE: f32 = 20.0;

/// How long the edge's ramp takes, ms.
pub const RAMP_MS: f32 = 500.0;

/// The ramp's share of full speed at its start; it rises
/// by [`RAMP_RISE`] over [`RAMP_MS`].
pub const RAMP_FLOOR: f32 = 0.2;
pub const RAMP_RISE: f32 = 0.8;

/// A frame this long after the last moves nothing: the original steps
/// one pixel then (a compare with `0x3e8`, scroll ramp), so a stall makes
/// no jump.
pub const STALL_MS: f32 = 1000.0;

/// How far past the window's edge a cursor still scrolls: the
/// `-edgescroll` switch with
/// Shift up is [`Reach::Any`]; else [`Reach::Near`]. Edge scrolling is
/// on either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// Up to [`OUTSIDE`] pixels past.
    Near,
    /// Any distance.
    Any,
}

/// The edge's push for a cursor `at` in a window of `size`, client
/// pixels: `∓speed` per axis toward the edge it is near, else 0.
/// [`Ramp::step`] makes it a move.
#[must_use]
pub const fn edge(at: (i32, i32), size: (i32, i32), reach: Reach, speed: i32) -> (i32, i32) {
    (
        along(at.0, size.0, reach, speed),
        along(at.1, size.1, reach, speed),
    )
}

/// One axis of [`edge`]: the near end first.
const fn along(at: i32, size: i32, reach: Reach, speed: i32) -> i32 {
    let any = matches!(reach, Reach::Any);

    if at < EDGE && (any || at >= -OUTSIDE) {
        -speed
    } else if at > size - EDGE && (any || at < size + OUTSIDE) {
        speed
    } else {
        0
    }
}

/// The edge's ramp: how long a push has lasted, ms.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ramp {
    pushed: f32,
}

impl Ramp {
    /// Push `(dx, dy)` held for a frame of `ms`, made a move in pixels:
    /// each part times [`RATE`], the seconds, and the ramp's share as the
    /// frame began. No push restarts the ramp.
    ///
    /// ```text
    ///  speed 32, 16 ms frames:  first 32 × 20 × 0.016 × 0.2 = 2.05 px
    ///                           from 500 ms on       × 1.0 = 10.24 px
    /// ```
    pub fn step(&mut self, ms: f32, (dx, dy): (i32, i32)) -> (f32, f32) {
        let share = RAMP_FLOOR + RAMP_RISE * self.pushed.min(RAMP_MS) / RAMP_MS;
        let stalled = ms >= STALL_MS;

        #[allow(clippy::cast_precision_loss, reason = "a push of pixels")]
        let moved = |push: i32| {
            if stalled {
                0.0
            } else {
                push as f32 * RATE * ms / 1000.0 * share
            }
        };

        self.pushed = if (dx, dy) == (0, 0) {
            0.0
        } else {
            self.pushed + ms
        };

        (moved(dx), moved(dy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: (i32, i32) = (800, 600);

    #[test]
    fn a_cursor_near_an_edge_scrolls_toward_it() {
        let edge = |at| edge(at, WINDOW, Reach::Near, SCROLL_SPEED);

        assert_eq!(edge((400, 300)), (0, 0));
        assert_eq!(edge((7, 300)), (-32, 0));
        assert_eq!(edge((8, 300)), (0, 0));
        assert_eq!(edge((793, 300)), (32, 0));
        assert_eq!(edge((792, 300)), (0, 0));
        assert_eq!(edge((0, 599)), (-32, 32));
    }

    #[test]
    fn past_the_window_only_the_switch_reaches_far() {
        let near = |at| edge(at, WINDOW, Reach::Near, SCROLL_SPEED);
        let any = |at| edge(at, WINDOW, Reach::Any, SCROLL_SPEED);

        assert_eq!(near((-16, 300)), (-32, 0));
        assert_eq!(near((-17, 300)), (0, 0));
        assert_eq!(near((815, 300)), (32, 0));
        assert_eq!(near((816, 300)), (0, 0));
        assert_eq!(any((-500, 300)), (-32, 0));
        assert_eq!(any((2000, 300)), (32, 0));
    }

    /// Within a float's rounding.
    fn near(got: (f32, f32), want: (f32, f32)) -> bool {
        (got.0 - want.0).abs() < 1e-4 && (got.1 - want.1).abs() < 1e-4
    }

    #[test]
    fn the_ramp_rises_from_a_fifth_over_half_a_second() {
        let mut ramp = Ramp::default();

        assert!(near(ramp.step(16.0, (32, -32)), (2.048, -2.048)));

        // 250 ms in: 0.2 + 0.8 × 250 / 500.
        ramp.step(234.0, (32, 0));
        assert!(near(ramp.step(16.0, (32, 0)), (10.24 * 0.6, 0.0)));

        ramp.step(300.0, (32, 0));
        assert!(near(ramp.step(16.0, (32, 0)), (10.24, 0.0)));
        assert!(near(ramp.step(16.0, (64, 0)), (20.48, 0.0)));
    }

    #[test]
    fn full_speed_is_twenty_times_the_scroll_speed_a_second() {
        let mut ramp = Ramp::default();

        ramp.step(RAMP_MS, (SCROLL_SPEED, 0));

        // Half a second: half of 640.
        assert!(near(ramp.step(500.0, (SCROLL_SPEED, 0)), (320.0, 0.0)));
    }

    #[test]
    fn no_push_restarts_the_ramp() {
        let mut ramp = Ramp::default();

        ramp.step(400.0, (32, 0));
        ramp.step(16.0, (0, 0));

        assert!(near(ramp.step(16.0, (32, 0)), (2.048, 0.0)));
    }

    #[test]
    fn a_stalled_frame_moves_nothing() {
        let mut ramp = Ramp::default();

        assert_eq!(ramp.step(STALL_MS, (64, 0)), (0.0, 0.0));
    }
}
