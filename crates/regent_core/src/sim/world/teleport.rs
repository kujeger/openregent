//! `TeleportToPoint` and `TeleportToUnit`: a unit moved at once, as the
//! player's spells' scripts move it. **Binary** unless marked.
//!
//! ```text
//!  TeleportToPoint(unit, max, point)        held: nothing;
//!    the point, at most max 32nds away; on the board: the place search
//!    there, none found: nothing; GenUnit moves the unit
//!  TeleportToUnit(unit, max, target, gap)   held: nothing;
//!    the target's place (a building's entrance, GenUnit); farther
//!    than max: max along the way; else, the two apart, gap short of
//!    it (gap above max: nothing); the place search, the move
//! ```

use super::{Answer, Found, World, agent_ref, int_ref, placing::Blockers};
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};

impl World {
    /// `TeleportToPoint(unit, max, point)`.
    pub(super) fn teleport_point(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let max = int_ref(vm, args, 2)?;
        let to = match vm.object(vm.arg(args, 3)?) {
            Value::Coord(x, y) => (*x, *y),
            _ => Err(Fault::throw("TeleportToPoint() was not given a coordinate"))?,
        };

        match self.found(id) {
            Found::Gone => Err(Fault::throw("TeleportToPoint() unit is null"))?,
            Found::Unsure => Ok(Answer::Unknown),
            Found::Piece(_) => Ok(self.teleport(id, max, to, Teleport::Point)),
        }
    }

    /// `TeleportToUnit(unit, max, target, gap)`. **Inferred**: method,
    /// which the two units must share for no gap, is the owner.
    pub(super) fn teleport_unit(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let max = int_ref(vm, args, 2)?;
        let target = agent_ref(vm, args, 3)?;
        let gap = int_ref(vm, args, 4)?;

        match (self.found(id), self.found(target)) {
            (Found::Gone, _) => Err(Fault::throw("TeleportToUnit() unit is null"))?,
            (_, Found::Gone) => Err(Fault::throw("TeleportToUnit() target is null"))?,
            (Found::Piece(u), Found::Piece(t)) => {
                let apart = u.owner != t.owner;
                let building = self
                    .body_of(target)
                    .is_some_and(|b| b.class == super::bodies::Class::Building);
                let to = if building {
                    self.exit(target)
                } else {
                    self.position(target)
                };

                match to {
                    Some(to) => Ok(self.teleport(id, max, to, Teleport::Unit { apart, gap })),
                    None => Ok(Answer::Unknown),
                }
            }
            _ => Ok(Answer::Unknown),
        }
    }

    /// Both teleports past their arguments: unit `id` toward `to`, 32nds.
    fn teleport(&mut self, id: u32, max: i32, to: (i32, i32), how: Teleport) -> Answer {
        let held = self.known(id).is_some_and(|p| p.holder != 0);
        let from = self.position(id);

        match (held, from) {
            (true, _) => Answer::Given,
            (false, None) => Answer::Unknown,
            (false, Some(from)) => {
                let aim = toward(from, to, max, how);
                let on_board = aim.is_some_and(|(x, y)| {
                    let (hx, hy) = (
                        x.div_euclid(crate::sim::units::PER_HALF),
                        y.div_euclid(crate::sim::units::PER_HALF),
                    );

                    self.board.holds((hx, hy))
                });
                let body = self.body_of(id).cloned();
                // The place search on the board; off it, the point itself
                // (gpl teleport to point skips the search).
                let spot = match (aim, on_board, how) {
                    (Some(a), true, _) | (Some(a), _, Teleport::Unit { .. }) => body
                        .as_ref()
                        .and_then(|b| self.place_among(b, a, Blockers::Others(id))),
                    (Some(a), false, Teleport::Point) => Some(a),
                    (None, ..) => None,
                };

                if let Some(spot) = spot {
                    let now = self.now;

                    if let Some(t) = self.units.tracks_mut(id) {
                        t.clear(now);
                    }

                    if let Some(u) = self.units.get_mut(id) {
                        u.looping = false;
                    }
                    self.put(id, spot);
                }

                Answer::Given
            }
        }
    }
}

/// Which teleport, and what it keeps short of the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Teleport {
    Point,
    /// `apart`: the units' method differ, so it stops `gap` short.
    Unit {
        apart: bool,
        gap: i32,
    },
}

/// Where a teleport from `from` aims for `to`: at most `max` along the
/// way, by fixed sqrt's whole distance; a unit apart from its
/// target stops `gap` short, or not at all past `max`. 32nds.
const fn toward(from: (i32, i32), to: (i32, i32), max: i32, how: Teleport) -> Option<(i32, i32)> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let squared = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy));
    let far = crate::gpl::vm::native::fixed_sqrt(squared.cast_unsigned(), 0).cast_signed();

    match how {
        _ if max < far => Some((from.0 + max * dx / far, from.1 + max * dy / far)),
        Teleport::Unit { apart: true, gap } if gap > max => None,
        Teleport::Unit { apart: true, gap } if far > 0 => {
            Some((to.0 - gap * dx / far, to.1 - gap * dy / far))
        }
        _ => Some(to),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::kind;

    #[test]
    fn a_teleport_stops_at_its_reach() {
        assert_eq!(
            toward((0, 0), (300, 400), 100, Teleport::Point),
            Some((60, 80))
        );
        assert_eq!(
            toward((0, 0), (300, 400), 1000, Teleport::Point),
            Some((300, 400))
        );
    }

    #[test]
    fn a_teleport_to_another_players_unit_stops_short() {
        let apart = Teleport::Unit {
            apart: true,
            gap: 50,
        };

        assert_eq!(toward((0, 0), (300, 400), 1000, apart), Some((270, 360)));
        assert_eq!(
            toward(
                (0, 0),
                (300, 400),
                40,
                Teleport::Unit {
                    apart: true,
                    gap: 50
                }
            ),
            Some((24, 32))
        );
        assert_eq!(
            toward(
                (0, 0),
                (30, 40),
                50,
                Teleport::Unit {
                    apart: true,
                    gap: 60
                }
            ),
            None
        );
    }

    /// A teleport sets the place outright, so it counts as a jump.
    #[test]
    fn a_teleport_counts_as_a_jump() {
        let mut world = World::default();

        world.pieces.insert(
            1,
            super::super::ruin::tests::standing(*b"BVr1", 100, kind::Kind::VEHICLE, (10, 10)),
        );
        world
            .units
            .set_motion(1, super::super::places::Motion::Still);

        let jumps = |w: &World| w.shown_one(1).map(|s| (s.at, s.jumps));

        assert_eq!(jumps(&world), Some(((160, 160), 0)));
        world.teleport(1, 1000, (400, 400), Teleport::Point);
        assert_eq!(jumps(&world), Some(((400, 400), 1)));
    }
}
