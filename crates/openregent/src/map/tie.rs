//! An order for drawings at one depth: pieces, effects and missiles.
//!
//! ```text
//!  pieces, missiles, effects ──> Transform z: their row's depth (Depth::at)
//!  untie                     ──> z moved nearer by depth::TIE a thing
//!                                where things tie, in Who order
//! ```
//!
//! Depth comes from the row a thing stands on, so two things on one row
//! tie, and the `GreaterEqual` depth test leaves them to the draw order,
//! which follows Bevy's entities and pipelines and changes from run to
//! run. The engine's draw list puts a tie after the parts listed before
//! it (`world::effectors`); the order it lists units in is not modelled.
//! **Inferred**: as by id, a lower one behind. **Game** (the user's
//! recollection): missiles draw over units. Effects between the two:
//! **Inferred**.

use bevy::prelude::*;

use regent_draw::map::depth;

/// What a drawing is, in the order things at one depth draw: behind first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Class {
    Piece,
    Effect,
    Missile,
}

/// Which drawing a thing is, ordering it among those at its depth: by
/// [`Class`], then by its agent id (a piece) or unit handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Who(pub(crate) Class, pub(crate) u32);

/// A drawing [`untie`] orders: who it is, and the depth its maker last
/// gave it (`base`) with the one `untie` wrote for it (`z`).
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct Tied {
    who: Who,
    base: f32,
    z: f32,
}

impl Tied {
    /// `who`, not yet ordered.
    pub(crate) const fn new(who: Who) -> Tied {
        Tied {
            who,
            base: f32::NAN,
            z: f32::NAN,
        }
    }

    /// The z to write for depth `base`: the one [`untie`] gave while the
    /// depth stays, so a still thing's transform is not written twice a frame.
    pub(crate) const fn keep(&self, base: f32) -> f32 {
        if base.to_bits() == self.base.to_bits() {
            self.z
        } else {
            base
        }
    }

    /// The depth its maker gave it: the one kept while `untie`'s still stands.
    const fn base(&self, z: f32) -> f32 {
        if z.to_bits() == self.z.to_bits() {
            self.base
        } else {
            z
        }
    }
}

/// Each shown drawing's depth moved where it ties ([`depth::untie`]).
/// A hidden one keeps its maker's depth and takes no place in a run.
pub(crate) fn untie(mut drawn: Query<(&mut Transform, &mut Tied, &Visibility)>) {
    let shown = |seen: &Visibility| *seen != Visibility::Hidden;
    let at: Vec<(f32, Who)> = drawn
        .iter()
        .filter(|(.., seen)| shown(seen))
        .map(|(put, tied, _)| (tied.base(put.translation.z), tied.who))
        .collect();
    let mut untied = depth::untie(&at).into_iter();

    for (mut put, mut tied, seen) in &mut drawn {
        let base = tied.base(put.translation.z);
        let z = if shown(seen) {
            untied.next().unwrap_or(base)
        } else {
            base
        };

        *tied = Tied { base, z, ..*tied };

        if put.translation.z.to_bits() != z.to_bits() {
            put.translation.z = z;
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    /// Run [`untie`] once over `world`.
    fn untied(world: &mut World) {
        world.run_system_once(untie).expect("untie runs");
    }

    /// The depth `e` draws at.
    fn z(world: &World, e: Entity) -> f32 {
        world.get::<Transform>(e).expect("drawn").translation.z
    }

    /// A drawing `who` at depth `z`.
    fn drawn(world: &mut World, who: Who, z: f32) -> Entity {
        world
            .spawn((
                Transform::from_xyz(0.0, 0.0, z),
                Tied::new(who),
                Visibility::Inherited,
            ))
            .id()
    }

    /// Two pieces and a missile on one row get one depth: untied, the
    /// lower id behind, the missile in front; a thing on another row
    /// keeps its own.
    #[test]
    fn things_on_one_row_draw_in_a_fixed_order() {
        let mut world = World::new();
        let row = -500.0;
        let later = drawn(&mut world, Who(Class::Piece, 9), row);
        let missile = drawn(&mut world, Who(Class::Missile, 1), row);
        let earlier = drawn(&mut world, Who(Class::Piece, 4), row);
        let apart = drawn(&mut world, Who(Class::Piece, 2), row + 1.0);

        untied(&mut world);

        let order = [earlier, later, missile].map(|e| z(&world, e));

        assert!(order.is_sorted_by(|a, b| a < b), "{order:?}");
        assert!(order[2] - order[0] < 0.01, "{order:?}");
        assert_eq!(z(&world, apart), row + 1.0);
    }

    /// A maker writing the same depth again keeps the untied one; a new
    /// depth is ordered afresh.
    #[test]
    fn an_untied_depth_stays_while_its_row_does() {
        let mut world = World::new();
        let row = -500.0;
        let first = drawn(&mut world, Who(Class::Piece, 1), row);
        let second = drawn(&mut world, Who(Class::Piece, 2), row);

        untied(&mut world);
        let was = z(&world, second);
        let tied = *world.get::<Tied>(second).expect("tied");

        assert!(was > row);
        assert_eq!(tied.keep(row), was);

        untied(&mut world);
        assert_eq!(z(&world, second), was);

        world
            .get_mut::<Transform>(first)
            .expect("drawn")
            .translation
            .z = row - 1.0;
        untied(&mut world);
        assert_eq!(z(&world, second), row);
    }
}
