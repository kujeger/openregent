//! Where the interface goes in the window: the left column, the top and
//! bottom bars, and the map in what is left.
//!
//! The column is HD's left 206 pixels of `APMK` down to its first mini
//! view (`ui_full.png`, 848 high): minimap, selection panel, second panel,
//! mini view. It keeps that shape: its height is the window's, its width
//! follows. The bars are as tall as HD's, at the column's scale; a window
//! made only wider stretches the bars' blank middles and nothing else.
//! The spell bar, open, lies over the map's bottom ([`spell_grid`]).
//!
//! ```text
//!  ┌────────┬──────────────────────────────────┐ ─┐
//!  │        │ top bar   (TOP_BAR × scale)      │  │
//!  │ column ├──────────────────────────────────┤  │
//!  │ COLUMN │                                  │  │ window height
//!  │ × scale│ map                              │  │ = COLUMN.y × scale
//!  │        ├──────────────────────────────────┤  │
//!  │        │ spell bar: row 1 …               │  │
//!  │        │            row 0                 │  │
//!  │        ├──────────────────────────────────┤  │
//!  │        │ bottom bar (BOTTOM_BAR × scale)  │  │
//!  └────────┴──────────────────────────────────┘ ─┘
//! ```

use bevy_egui::egui::{Pos2, Rect, Vec2, pos2, vec2};

/// The column's size in HD's pixels: `APMK`'s left edge pieces end at
/// 206, its first mini view's button bar at 847.
pub const COLUMN: Vec2 = vec2(206.0, 848.0);

/// The top bar's height in HD's pixels: `APMK`'s map control 2003 starts
/// at y 29.
pub const TOP_BAR: f32 = 29.0;

/// The bottom bar's height in HD's pixels: 1200 less control 2003's
/// bottom, 1165, at 1920 × 1200.
pub const BOTTOM_BAR: f32 = 35.0;

/// HD's screen the bars' pieces are placed on: their x from the left
/// edge, or from this width's right edge.
pub const SCREEN_WIDTH: f32 = 1920.0;

/// The spell bar's height with one row, HD pixels: `AP92`'s.
pub const SPELL_BAR: f32 = 34.0;

/// A spell button's side, HD pixels: the base `AP92`'s 25 × 25.
pub const SPELL_CELL: f32 = 25.0;

/// The spell bar's margin left and above a row, HD pixels: `AP92`'s
/// buttons start at (4, 4).
const SPELL_MARGIN: f32 = 4.0;

/// The space between two temples' spells, HD pixels. **Inferred**: the
/// base `AP92`'s buttons are 25 apart within a temple, about 30 across.
const TEMPLE_GAP: f32 = 5.0;

/// One spell row to the next, HD pixels.
const SPELL_ROW: f32 = SPELL_CELL + SPELL_MARGIN;

/// The interface's scale on a `window`-sized window, points an HD pixel:
/// the window's height over the column's. The one rule for the game, the
/// menus, the quest map and egui's pages (a departure: the original draws
/// its menus at their own size).
///
/// ```text
///  1280 × 720: 0.849   1920 × 1200: 1.415   2560 × 1440: 1.698
/// ```
#[must_use]
pub fn scale(window: Vec2) -> f32 {
    (window.y / COLUMN.y).max(0.0)
}

/// The interface's rectangles in the window's points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Points an HD pixel.
    pub scale: f32,
    pub column: Rect,
    pub top: Rect,
    pub bottom: Rect,
    /// The spell bar, between the map and the bottom bar; none at first.
    pub spells: Rect,
    pub map: Rect,
}

impl Frame {
    /// The frame of a `window`-sized window.
    #[must_use]
    pub fn of(window: Vec2) -> Frame {
        let scale = scale(window);
        let width = (COLUMN.x * scale).min(window.x);
        let (top, bottom) = (TOP_BAR * scale, BOTTOM_BAR * scale);
        let floor = window.y - bottom;

        Frame {
            scale,
            column: Rect::from_min_size(Pos2::ZERO, vec2(width, window.y)),
            top: Rect::from_min_max(pos2(width, 0.0), pos2(window.x, top)),
            bottom: Rect::from_min_max(pos2(width, floor), window.to_pos2()),
            spells: Rect::from_min_max(pos2(width, floor), pos2(window.x, floor)),
            map: Rect::from_min_max(pos2(width, top), pos2(window.x, floor)),
        }
    }

    /// The frame with a spell bar `height` HD pixels high over the map's
    /// bottom, at most the whole map. The map keeps its size.
    #[must_use]
    pub fn with_spells(self, height: f32) -> Frame {
        let top = (self.map.max.y - height * self.scale).max(self.map.min.y);

        Frame {
            spells: Rect::from_min_max(pos2(self.map.min.x, top), self.map.max),
            ..self
        }
    }

    /// An HD column point on the window.
    #[must_use]
    pub fn in_column(&self, at: Pos2) -> Pos2 {
        self.column.min + at.to_vec2() * self.scale
    }

    /// A top or bottom bar piece at HD screen x `x` on the window: from
    /// the column's edge when `anchor` is the left, else from the right.
    #[must_use]
    pub fn bar_x(&self, x: f32, anchor: Anchor) -> f32 {
        match anchor {
            Anchor::Left => self.column.max.x + (x - COLUMN.x) * self.scale,
            Anchor::Right => self.top.max.x - (SCREEN_WIDTH - x) * self.scale,
        }
    }
}

/// The spell buttons' places on the spell bar, HD pixels from its top
/// left, and its rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grid {
    pub rows: usize,
    pub at: Vec<Pos2>,
}

impl Grid {
    /// The bar's height, HD pixels: `AP92`'s for one row, a row more each.
    #[must_use]
    pub fn height(&self) -> f32 {
        #[allow(clippy::cast_precision_loss, reason = "a few rows")]
        let more = self.rows.saturating_sub(1) as f32;

        match self.rows {
            0 => 0.0,
            _ => SPELL_BAR + more * SPELL_ROW,
        }
    }

    /// Button `i`'s cell, where it is drawn and pressed.
    #[must_use]
    pub fn cell(&self, i: usize) -> Option<Rect> {
        self.at
            .get(i)
            .map(|at| Rect::from_min_size(*at, Vec2::splat(SPELL_CELL)))
    }

    /// The button under HD point `p` on the bar.
    #[cfg(test)]
    fn hit(&self, p: Pos2) -> Option<usize> {
        (0..self.at.len()).find(|&i| self.cell(i).is_some_and(|c| c.contains(p)))
    }
}

/// Spell buttons in `groups`' order, each a temple's, laid out at full
/// size on a bar `width` window points wide at `scale`: left to right, a
/// gap between temples, a new row where the next does not fit; a temple
/// that fits a row whole is not split. Row 0 is the lowest; more rows
/// stack upward.
///
/// ```text
///  4 ┌──┬──┬──┐ 5 ┌──┬──┐       a row: 4 + 25 a button, 5 between
///    └──┴──┴──┘   └──┴──┘       temples, 4 short of the right edge
/// ```
#[must_use]
pub fn spell_grid<T: PartialEq>(groups: &[T], width: f32, scale: f32) -> Grid {
    let right = width / scale.max(f32::EPSILON) - SPELL_MARGIN;

    // Each button's column and row, rows counted from the first.
    let mut x = SPELL_MARGIN;
    let mut row = 0;
    let placed: Vec<(f32, usize)> = groups
        .iter()
        .enumerate()
        .map(|(i, group)| {
            let first = i.checked_sub(1).is_none_or(|p| groups[p] != *group);
            let gap = if first && i > 0 { TEMPLE_GAP } else { 0.0 };

            // A temple's first button asks room for the whole temple.
            #[allow(clippy::cast_precision_loss, reason = "a few buttons")]
            let whole = groups[i..].iter().take_while(|g| *g == group).count() as f32 * SPELL_CELL;
            let room = if first && SPELL_MARGIN + whole <= right {
                whole
            } else {
                SPELL_CELL
            };

            if x > SPELL_MARGIN && x + gap + room > right {
                row += 1;
                x = SPELL_MARGIN;
            } else if x > SPELL_MARGIN {
                x += gap;
            }

            let at = (x, row);

            x += SPELL_CELL;
            at
        })
        .collect();
    let rows = placed.last().map_or(0, |(_, r)| r + 1);

    #[allow(clippy::cast_precision_loss, reason = "a few rows")]
    let at = placed
        .into_iter()
        .map(|(x, r)| pos2(x, SPELL_MARGIN + (rows - 1 - r) as f32 * SPELL_ROW))
        .collect();

    Grid { rows, at }
}

/// Which edge a bar piece keeps its distance from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Left,
    Right,
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "whole numbers, exact in f32")]
mod tests {
    use super::*;

    #[test]
    fn at_the_columns_own_height_everything_is_hds_size() {
        let frame = Frame::of(vec2(1680.0, 848.0));

        assert_eq!(frame.scale, 1.0);
        assert_eq!(frame.column.width(), 206.0);
        assert_eq!(
            frame.map,
            Rect::from_min_max(pos2(206.0, 29.0), pos2(1680.0, 813.0))
        );
    }

    #[test]
    fn a_taller_window_widens_the_column_and_keeps_its_shape() {
        let low = Frame::of(vec2(1920.0, 848.0));
        let tall = Frame::of(vec2(1920.0, 1696.0));

        assert_eq!(tall.scale, 2.0);
        assert_eq!(tall.column.width(), 412.0);
        assert_eq!(tall.top.height(), 2.0 * low.top.height());
        assert_eq!(
            tall.column.width() / tall.column.height(),
            low.column.width() / low.column.height()
        );
    }

    #[test]
    fn a_wider_window_only_widens_the_bars_and_the_map() {
        let narrow = Frame::of(vec2(1280.0, 1000.0));
        let wide = Frame::of(vec2(2560.0, 1000.0));

        assert_eq!(narrow.column, wide.column);
        assert_eq!(narrow.top.height(), wide.top.height());
        assert_eq!(wide.top.width() - narrow.top.width(), 1280.0);
        assert_eq!(wide.map.width() - narrow.map.width(), 1280.0);
        assert_eq!(narrow.map.height(), wide.map.height());
    }

    #[test]
    fn bar_pieces_keep_their_distance_from_their_edge() {
        let frame = Frame::of(vec2(1000.0, 424.0));

        // HD's gold box at x 206 starts at the column's edge; its day
        // box, 1807, ends 1920 − 1919 = 1 pixel short of the right.
        assert_eq!(frame.bar_x(206.0, Anchor::Left), frame.column.max.x);
        assert_eq!(frame.bar_x(1919.0, Anchor::Right), 999.5);
    }

    #[test]
    fn the_spell_bar_lies_over_the_maps_bottom() {
        let frame = Frame::of(vec2(1680.0, 848.0));
        let open = frame.with_spells(SPELL_BAR);

        assert_eq!(frame.spells.height(), 0.0);
        assert_eq!(
            open.spells,
            Rect::from_min_max(pos2(206.0, 779.0), pos2(1680.0, 813.0))
        );
        assert_eq!(open.map, frame.map);
        assert_eq!(open.bottom, frame.bottom);
        assert_eq!(frame.with_spells(10_000.0).spells.min.y, frame.map.min.y);
    }

    /// The base bar's 22 in seven temples, the expansion's 28 in eight.
    const BASE: [u8; 22] = [
        0, 0, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 6, 6,
    ];

    fn expansion() -> Vec<u8> {
        BASE.iter().copied().chain([7; 6]).collect()
    }

    #[test]
    fn one_row_fits_a_wide_bar_at_full_size_with_temple_gaps() {
        let grid = spell_grid(&expansion(), 1474.0, 1.0);

        assert_eq!(grid.rows, 1);
        assert_eq!(grid.height(), SPELL_BAR);
        assert_eq!(grid.at[0], pos2(4.0, 4.0));
        assert_eq!(grid.at[5], pos2(129.0, 4.0));
        // A new temple: 5 more.
        assert_eq!(grid.at[6], pos2(159.0, 4.0));
        assert_eq!(grid.at[27], pos2(4.0 + 28.0 * 25.0 + 7.0 * 5.0 - 25.0, 4.0));
    }

    #[test]
    fn a_narrow_bar_wraps_into_rows_stacked_upward() {
        // 300 HD pixels: room for 11 buttons and their gaps.
        let grid = spell_grid(&expansion(), 600.0, 2.0);

        assert_eq!(grid.rows, 3);
        assert_eq!(grid.height(), SPELL_BAR + 2.0 * 29.0);
        assert!(grid.at.iter().all(|p| p.x + 25.0 <= 296.0));

        // The first row lowest, the last highest.
        assert_eq!(grid.at[0], pos2(4.0, 4.0 + 2.0 * 29.0));
        assert_eq!(grid.at[27].y, 4.0);

        // A row starts at the margin with a temple's first spell: Dauros's
        // and Lunord's.
        let starts: Vec<usize> = (1..28).filter(|&i| grid.at[i].x == 4.0).collect();

        assert_eq!(starts, [9, 20]);
    }

    #[test]
    fn every_scale_and_width_gives_each_button_its_own_cell() {
        for (width, scale) in [(1474.0, 1.0), (837.0, 0.9), (400.0, 1.5), (60.0, 1.0)] {
            let grid = spell_grid(&expansion(), width, scale);

            for (i, at) in grid.at.iter().enumerate() {
                let centre = *at + Vec2::splat(12.5);

                assert_eq!(grid.hit(centre), Some(i), "{width} {scale}");
            }
        }
    }

    #[test]
    fn no_buttons_take_no_room() {
        let grid = spell_grid::<u8>(&[], 1000.0, 1.0);

        assert_eq!(grid.rows, 0);
        assert_eq!(grid.height(), 0.0);
        assert_eq!(grid.hit(pos2(10.0, 10.0)), None);
    }

    #[test]
    fn a_window_narrower_than_the_column_gives_the_map_nothing() {
        let frame = Frame::of(vec2(100.0, 848.0));

        assert_eq!(frame.column.width(), 100.0);
        assert_eq!(frame.map.width(), 0.0);
    }
}
