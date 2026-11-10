//! A scroll bar's geometry, shared by the panels and the menus
//! (`CYDialogScrollbarItem`, `docs/formats/panels.md`, scroll bars).
//!
//! ```text
//!  ┌──┐ up arrow, set
//!  │  │ ┐
//!  ├──┤ │ track: the thumb's top runs
//!  │()│ │ (pos − min) × track / (max − min) down it
//!  │  │ ┘
//!  └──┘ down arrow, set + 1
//! ```

use bevy_egui::egui::{Pos2, Rect, Response, Vec2, pos2};

/// The down arrow's set and the thumb's, past the bar's own.
pub const DOWN: i32 = 1;
pub const THUMB: i32 = 2;

/// An arrow's slot while pressed: pink (**Inferred** from the art; 2 to 5
/// are greyed).
pub const PRESSED: usize = 1;

/// How far a text taller than its box scrolls: 0 when it fits, and then
/// its bar shows neither arrows nor thumb.
#[must_use]
pub fn most(tall: f32, shown: f32) -> f32 {
    (tall - shown).max(0.0)
}

/// A scroll bar: its rectangle and its arrows' and thumb's sizes, in the
/// caller's pixels.
#[derive(Clone, Copy, Debug)]
pub struct Bar {
    pub rect: Rect,
    pub arrow: Vec2,
    pub thumb: Vec2,
}

impl Bar {
    /// The up arrow, at the bar's top.
    #[must_use]
    pub fn up(&self) -> Rect {
        Rect::from_min_size(self.rect.min, self.arrow)
    }

    /// The down arrow, at the bar's bottom.
    #[must_use]
    pub fn down(&self) -> Rect {
        let top = self.rect.bottom() - self.arrow.y;

        Rect::from_min_size(pos2(self.rect.left(), top), self.arrow)
    }

    /// How far the thumb's top runs. **Inferred**: the bar less its
    /// arrows and the thumb.
    #[must_use]
    pub fn track(&self) -> f32 {
        self.rect.height() - 2.0 * self.arrow.y - self.thumb.y
    }

    /// The thumb, scrolled `by` of `most`: under the up arrow at the top
    /// (**Game**: `ui_full.png`), as far down the track as the text is
    /// scrolled; centred on the arrows (**Inferred**).
    #[must_use]
    pub fn thumb(&self, by: f32, most: f32) -> Rect {
        let share = if most > 0.0 {
            (by / most).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let left = self.rect.left() + (self.arrow.x - self.thumb.x) / 2.0;
        let top = self.rect.top() + self.arrow.y + share * self.track();

        Rect::from_min_size(pos2(left, top), self.thumb)
    }

    /// The scroll a thumb dragged `dy` moves, of `most`: as far through
    /// the text as through the track.
    #[must_use]
    pub fn dragged(&self, dy: f32, most: f32) -> f32 {
        dy * most / self.track().max(1.0)
    }
}

/// One pass of scroll bar `bar` over a text scrolled `by` of `most`, on
/// the caller's canvas `c`, `scale` window pixels a bar pixel: each
/// arrow made by `press` and drawn by `draw` (set past the bar's, slot),
/// [`PRESSED`] while held, a click a `step`; the thumb made by `grab`.
/// How far they move the text.
pub fn pass<C>(
    c: &mut C,
    (bar, scale): (&Bar, f32),
    (by, most, step): (f32, f32, f32),
    press: impl Fn(&mut C, Rect, i32) -> Response,
    grab: impl Fn(&mut C, Rect) -> Response,
    draw: impl Fn(&mut C, i32, usize, Pos2),
) -> f32 {
    let mut moved = 0.0;

    for (rect, past, sign) in [(bar.up(), 0, -1.0), (bar.down(), DOWN, 1.0)] {
        let response = press(c, rect, past);
        let slot = if response.is_pointer_button_down_on() {
            PRESSED
        } else {
            0
        };

        draw(c, past, slot, rect.min);

        if response.clicked() {
            moved += sign * step;
        }
    }

    // The thumb, dragged, moves the text as far through its own length
    // as it moves through the track.
    let at = bar.thumb(by, most);
    let response = grab(c, at);

    draw(c, THUMB, 0, at.min);

    if response.dragged() {
        moved += bar.dragged(response.drag_delta().y / scale, most);
    }

    moved
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_egui::egui::vec2;

    /// `AP03`'s bar 5001 at (174, 28), 25 × 186, `INDj`'s 17-tall arrows
    /// and 10-tall thumb.
    fn panel_bar() -> Bar {
        Bar {
            rect: Rect::from_min_size(pos2(174.0, 28.0), vec2(25.0, 186.0)),
            arrow: vec2(25.0, 17.0),
            thumb: vec2(25.0, 10.0),
        }
    }

    /// `APc9`'s bar 9101, 27 × 383 at (757, 120); `INDs`' 25 × 27 arrows
    /// and 23 × 27 thumb.
    fn briefing_bar() -> Bar {
        Bar {
            rect: Rect::from_min_size(pos2(757.0, 120.0), vec2(27.0, 383.0)),
            arrow: vec2(25.0, 27.0),
            thumb: vec2(23.0, 27.0),
        }
    }

    #[test]
    fn the_thumb_runs_between_the_arrows_as_far_as_the_text_is_scrolled() {
        let bar = panel_bar();

        assert!((bar.thumb(0.0, 90.0).top() - 45.0).abs() < 1e-3);
        assert!((bar.thumb(45.0, 90.0).top() - 116.0).abs() < 1e-3);
        assert!((bar.thumb(90.0, 90.0).bottom() - (214.0 - 17.0)).abs() < 1e-3);
    }

    #[test]
    fn the_briefing_thumb_is_centred_on_the_arrows() {
        let bar = briefing_bar();
        let top = bar.thumb(0.0, 100.0);
        let bottom = bar.thumb(100.0, 100.0);

        assert_eq!(top.min, pos2(758.0, 147.0));
        assert_eq!(top.size(), vec2(23.0, 27.0));
        assert!((bottom.max.y - bar.down().top()).abs() < 1e-3);
        assert_eq!(
            bar.down(),
            Rect::from_min_size(pos2(757.0, 476.0), vec2(25.0, 27.0))
        );
    }

    #[test]
    fn a_drag_through_the_whole_track_scrolls_the_whole_text() {
        let bar = briefing_bar();

        assert!((bar.dragged(bar.track(), 80.0) - 80.0).abs() < 1e-3);
        assert!((bar.dragged(bar.track() / 4.0, 80.0) - 20.0).abs() < 1e-3);
    }

    #[test]
    fn a_text_that_fits_does_not_scroll() {
        assert!(most(300.0, 382.0).abs() < f32::EPSILON);
        assert!((most(423.0, 382.0) - 41.0).abs() < 1e-3);
    }
}
