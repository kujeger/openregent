//! `PathCost`: the planner run once, no walker moved. **Binary** unless
//! marked.
//!
//! ```text
//!  PathCost(from, to, mode, found&, unit)
//!    mode  "check_buildings" 0x100 (also any other text)
//!          "check_static_units" 0x200
//!          "check_enemy_units", "check_all_units" 0x400
//!    the unit by handle; none throws
//!  path cost search
//!    from, to: 32nds to half cells, toward zero
//!    path_search(unit, from, to, flags): the aim, a flood from `to`
//!      counting anyone; A* from `from`, at most PathHorizon pops
//!    found  the node it ends on is `to`
//!    cost   the squared steps back to the start: 1 a side, 2 a diagonal
//!  the result  cost as a float (shl 10); found& written after
//! ```

use super::{Answer, Found, World};
use crate::base::gfix::Gfix;
use crate::gpl::ops::{Accessor, OpError, Value};
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::movement::PATH_HORIZON;
use crate::sim::movement::plan::{Blocking, Cell, LAYERS, NOWHERE, Occupant, Rank, aim, plan};

/// `PathCost`'s modes and the path info each sets.
pub const MODES: [(&str, Blocking); 4] = [
    ("check_buildings", Blocking::Buildings),
    ("check_static_units", Blocking::Still),
    ("check_enemy_units", Blocking::Any),
    ("check_all_units", Blocking::Any),
];

/// A float's scale: the cost goes in shifted left 10.
const FLOAT_SHIFT: u32 = 10;

/// `PathCost`'s mode: its text's, folding case (`_stricmp`); else
/// `check_buildings`.
///
/// ```
/// use regent_core::sim::movement::plan::Blocking;
/// use regent_core::sim::world::cost::path_mode;
///
/// assert_eq!(path_mode("CHECK_ALL_UNITS"), Blocking::Any);
/// assert_eq!(path_mode("anything"), Blocking::Buildings);
/// ```
#[must_use]
pub fn path_mode(text: &str) -> Blocking {
    MODES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(text))
        .map_or(Blocking::Buildings, |(_, how)| *how)
}

/// 32nds of a cell to a half cell, toward zero.
const fn half(v: i32) -> i32 {
    v / 16
}

/// The planner's cost of a path: each step's squared length.
fn steps_cost(nodes: &[Cell]) -> i32 {
    nodes
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).pow(2) + (w[1].1 - w[0].1).pow(2))
        .sum()
}

impl World {
    /// `PathCost(from, to, mode, found, unit)`, gpl path cost: the cost of
    /// `unit`'s plan from `from` to `to`, a float, and whether it ends
    /// there, written to `found`. The arguments read first, through
    /// `gplcoord&`, `gplstring&`, `gplboolean&`; no unit throws. Known
    /// where every piece's place is.
    pub(super) fn path_cost(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let coord = |i: usize| match vm.object(vm.arg(args, i)?) {
            Value::Coord(x, y) => Ok((*x, *y)),
            _ => Err(Fault::from(OpError::BadConversion(Accessor::GplCoordRef))),
        };
        let from = coord(1)?;
        let to = coord(2)?;
        let how = match vm.object(vm.arg(args, 3)?) {
            Value::String(s) => path_mode(s),
            _ => Err(OpError::BadConversion(Accessor::GplStringRef))?,
        };
        let found_at = vm.arg(args, 4)?;

        if !matches!(vm.object(found_at), Value::Boolean(_)) {
            Err(OpError::BadConversion(Accessor::GplBooleanRef))?;
        }

        let id = super::agent_ref(vm, args, 5)?;
        let gone = || Fault::throw("PathCost() referred to a non-existent game-object");

        // The planner reads the grid up to date.
        self.regrid();

        let ran = match self.found(id) {
            Found::Gone => Err(gone())?,
            Found::Unsure => None,
            Found::Piece(_) => self.search(id, from, to, how),
        };

        match ran {
            Some((cost, found)) => {
                let ret = vm.arg(args, 0)?;

                crate::gpl::ops::assign(
                    vm.object_mut(ret),
                    &Value::Float(Gfix(cost << FLOAT_SHIFT)),
                )?;
                *vm.object_mut(found_at) = Value::Boolean(found);
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// Path search for piece `id` from `from` to `to`, 32nds,
    /// counting `how`: the cost and whether it ends at `to`. `None` where
    /// a place is not known.
    fn search(
        &self,
        id: u32,
        from: (i32, i32),
        to: (i32, i32),
        how: Blocking,
    ) -> Option<(i32, bool)> {
        let piece = self.pieces.get(id)?;
        let body = self.descriptors.body(piece.code)?;
        let known = self.frames == super::frame::Frames::Stepped && self.first_unplaced().is_none();
        let grid = known.then(|| {
            self.check_grid();
            self.collision()
        })?;

        // The flags carry no 0x1000: nothing is trampled.
        let me = Occupant {
            handle: piece.handle,
            info: body.layer & LAYERS,
            state: 0,
            vehicle: true,
            building: false,
            chest: false,
            clearable: false,
            tramples: false,
            dont_block: false,
        };
        let fits = |c: Cell, how: Blocking| grid.fits(&body.cells, body.edge, c, &me, how);
        let start = (half(from.0), half(from.1));
        let goal = (half(to.0), half(to.1));
        let target = aim(self.board.side(), goal, start, &Rank::Goal, &|c| {
            fits(c, Blocking::Any)
        })
        .unwrap_or(NOWHERE);
        let nodes = plan(start, target, usize::MAX, PATH_HORIZON, &mut |c| {
            fits(c, how)
        })
        .nodes;
        let found = nodes.last() == Some(&goal);

        Some((steps_cost(&nodes), found))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paths_cost_is_one_a_side_step_and_two_a_diagonal() {
        assert_eq!(steps_cost(&[(0, 0), (1, 1), (2, 1), (3, 1)]), 4);
        assert_eq!(steps_cost(&[(5, 5)]), 0);
    }

    #[test]
    fn a_coordinate_halves_toward_zero() {
        assert_eq!([half(31), half(32), half(-17)], [1, 2, -1]);
    }
}
