//! Which of a set's frames a unit part draws, and where the draw list
//! sorts it. Engine draw rules, kept in `data` because
//! [`crate::content::imagery`] draws with them.

use super::{PASS, Set};

/// Where [`Set::sort_nudge`] sits in [`Set::control`]: set `+0x3c`, the
/// loaded set's.
const SORT_NUDGE_AT: usize = 0x38;

/// [`Set::layer_flags`] bit 0: the set's parts sort by their own y, nudged
/// by [`Set::sort_nudge`].
pub const SORTS_OWN: u32 = 1;

/// A unit draw's pass: which of a slot's frames a part draws, and at
/// what height.
///
/// **Binary**: `UnitIMG` makes a part per layer
/// flag, bit 8 a first-pass part, bit 9 a second, layer by layer, into
/// part flags bits 8 to 11. `GSUnit` (`GSUnit`, as
/// `IsoGenUnit`) draws a frame where its [`PASS`] bit is the
/// part's pass; a second-pass part stands at ground height under
/// the unit's unit pos, a first at the unit's own place. The
/// second part draws with the first, in part order: the draw list holds
/// it apart only as any higher part.
///
/// **Corpus**: the second pass is a flier's shadow, tiles of mode 1 or
/// 7: arrows, bolts, the wizard blasts, the dragons and Vendral.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pass {
    /// Frames without [`PASS`], at the unit's own height.
    First,
    /// Frames with [`PASS`], on the ground under the unit.
    Second,
}

impl Pass {
    /// Both, in the order a layer makes their parts.
    pub const ALL: [Pass; 2] = [Pass::First, Pass::Second];

    /// The layer flag making a part of this pass: bit 8 or 9.
    #[must_use]
    pub const fn part_flag(self) -> u32 {
        match self {
            Pass::First => 1 << 8,
            Pass::Second => 1 << 9,
        }
    }

    /// The pass a frame reference draws in.
    #[must_use]
    pub const fn of(reference: u32) -> Pass {
        if reference & PASS == 0 {
            Pass::First
        } else {
            Pass::Second
        }
    }

    /// Where a part of this pass stands: `own`, the unit's place, or
    /// `ground`, the ground's under it.
    #[must_use]
    pub fn height<T>(self, own: T, ground: T) -> T {
        match self {
            Pass::First => own,
            Pass::Second => ground,
        }
    }
}

impl Set<'_> {
    /// The passes this layer makes parts for, in order ([`Pass`]).
    pub fn passes(&self) -> impl Iterator<Item = Pass> + '_ {
        Pass::ALL
            .into_iter()
            .filter(|p| self.layer_flags() & p.part_flag() != 0)
    }

    /// Screen pixels a part of this set moves down the draw list, nearer
    /// the viewer: set `+0x3c` where [`SORTS_OWN`] is set, else 0.
    /// `IsoDrawList` adds it over the camera's
    /// `pixel_scale_sin` to the part's unit pos y. **Corpus**: the
    /// attack flag's 2,000, the explore flag's 1,500, a ruin's -1,000.
    pub const fn sort_nudge(&self) -> i32 {
        if self.layer_flags() & SORTS_OWN == 0 {
            0
        } else {
            i32::from_le_bytes(self.bytes(SORT_NUDGE_AT))
        }
    }
}
