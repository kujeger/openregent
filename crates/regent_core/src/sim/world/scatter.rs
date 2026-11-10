//! The map's art beside its pieces: every scatter object and terrain
//! feature with no agent as drawn, and the set and slot each start
//! piece's record drew.
//!
//! ```text
//!  a save's GRND, AE:0 0 ──> Strewn: place, height, set, slot, frame
//!  generated scatter and chart features ──> the same, made as made
//!  BuildingRec's clearing ──> a gridded one gone (world::building_ground)
//!  building flatten's reseat ──> a gridded one's height
//!  a save's BLDG and VHCL ──> each record's set and slot, by handle
//! ```
//!
//! [`Board::scatter`](super::board::Board::scatter) holds what the
//! engine's rules read of the gridded ones; this holds what drawing them
//! needs, the off-grid (`DontBlock`) ones too. Each rule that changes one
//! applies to both.

use super::building_ground::Stand;
use super::new_map::OnGrid;
use crate::data::fourcc::FourCc;
use crate::sim::space;

/// The set an object draws and its slot and frame: a record's packed
/// set id and its drawing pair.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drawn {
    pub set: u32,
    pub drawing: (u16, u16),
}

/// One scatter object or terrain feature with no agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Strewn {
    pub code: FourCc,
    pub handle: u32,
    /// `None`: no owner.
    pub owner: Option<u32>,
    pub stand: Stand,
    /// Its half cell.
    pub standing: (i32, i32),
    pub drawn: Drawn,
    pub grid: OnGrid,
}

impl Strewn {
    /// Its half cell as [`Board::scatter`](super::board::Board::scatter) keys it: from where it stands,
    /// as a piece's place reads it.
    #[must_use]
    pub const fn half(&self) -> (i32, i32) {
        let (x, y) = self.stand.fixed;

        (space::half(x), space::half(y))
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Drawn {
        type Model = m::Drawn;

        fn save(&self) -> m::Drawn {
            m::Drawn {
                set: self.set,
                drawing: self.drawing,
            }
        }

        fn load(model: m::Drawn) -> Result<Drawn, Refused> {
            Ok(Drawn {
                set: model.set,
                drawing: model.drawing,
            })
        }
    }

    impl Saved for Strewn {
        type Model = m::Strewn;

        fn save(&self) -> m::Strewn {
            m::Strewn {
                code: self.code.save(),
                handle: self.handle,
                owner: self.owner,
                stand: self.stand.save(),
                standing: self.standing,
                drawn: self.drawn.save(),
                grid: self.grid.save(),
            }
        }

        fn load(model: m::Strewn) -> Result<Strewn, Refused> {
            Ok(Strewn {
                code: FourCc::load(model.code)?,
                handle: model.handle,
                owner: model.owner,
                stand: Stand::load(model.stand)?,
                standing: model.standing,
                drawn: Drawn::load(model.drawn)?,
                grid: OnGrid::load(model.grid)?,
            })
        }
    }
}
