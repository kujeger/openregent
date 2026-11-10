//! HD's mods row: the strip under the main menu's buttons in a `UIData`
//! `AP29` (800 × 768), with CHANGE MODS and ACTIVATE MODS.
//!
//! ```text
//!  y 600  base AP29's frame ends; the row's controls lie below
//!  y 605  INx1 1018 ── 1019, tiled ───────────────────────── 1020
//!         1003 ┌ INx1 1008, tiled, x 19-781 ─────────────────┐ 1004
//!              │ (INx2 1011 [DEM2 1001 9000])  (INx2 1011 [9001]) │
//!  y 683  1005 ── INx1 1006, tiled ───────────────────────── 1007
//! ```
//!
//! **Corpus**: the twelve `UIData` files holding `AP29` hold the same
//! layout; the pictures are the base
//! dataset's `addinterface_se.cam`. The row's controls share the base
//! panel's coordinates: above y 600 the two layouts place the same
//! buttons, EXIT GAME a pixel right in `UIData`'s.

use super::{Layout, ShownControl};
use crate::data::fourcc::FourCc;

/// The row: controls of a `UIData` `AP29` starting at or below this y,
/// the base `AP29`'s height.
pub const TOP: i32 = 600;

/// CHANGE MODS, `STRT AP29` text 27: opens `APdc`.
pub const CHANGE_MODS: i32 = 9000;

/// ACTIVATE MODS or DEACTIVATE MODS, `STRT AP29` text 28: toggles the
/// mods, disabled without any.
pub const ACTIVATE_MODS: i32 = 9001;

/// Each button's surround, 281 × 31: a black well with scroll ends.
pub const SURROUND: (FourCc, i32) = (FourCc(*b"INx2"), 1011);

/// The strip behind the buttons, tiled. **Game**: drawn over the four
/// corner pieces it overlaps (`ui_main_menu_hd.png`), though the panel
/// lists it first; why is open.
pub const STRIP: (FourCc, i32) = (FourCc(*b"INx1"), 1008);

/// `layout`'s row controls, in draw order.
pub fn controls(layout: &Layout) -> impl Iterator<Item = &ShownControl> {
    layout
        .controls
        .iter()
        .filter(|c| c.rect.is_some_and(|r| r.y >= TOP))
}

/// The row's lowest edge in `layout`, if it has a row.
#[must_use]
pub fn bottom(layout: &Layout) -> Option<i32> {
    controls(layout)
        .filter_map(|c| c.rect)
        .map(|r| r.y + r.height)
        .max()
}
