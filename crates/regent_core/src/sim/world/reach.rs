//! Each bound piece's pad rect where the world knows it, kept for
//! `ListObjects`' range: a ranged search passes over a piece known out of
//! range without reading its record, unit or attributes.
//!
//! ```text
//!  a change to a piece ──> Pieces / Units mark it stale (range set)
//!  before ListObjects: World::rereach, each stale piece's reach anew
//!  ListObjects: a piece whose rect is known and out of range: a sure no,
//!     not walked; any other walked and tested whole (`World::passes`)
//!  with MAJESTY_CHECK_REACH set: the kept reaches equal ones made anew
//! ```
//!
//! A piece known out of range fails `ListObjects`' range test for sure,
//! and one failed test makes its verdict a sure no whatever the others
//! give; the list is sorted after the walk, so passing over it changes
//! nothing. A held piece stands where its holder does, which moves
//! without marking it: its reach is unknown, and it is walked.

use crate::base::digest::Map;
use crate::base::switch::Switch;
use crate::sim::space::{self, HalfRect};

use super::World;
use super::places::known_of;

/// Environment variable that turns on the check of the kept reaches
/// against ones made anew, e.g. `MAJESTY_CHECK_REACH=1`.
pub static CHECK_REACH: Switch = Switch::new("MAJESTY_CHECK_REACH");

/// Where a bound piece's pad rect is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reach {
    /// Its pad rect, known.
    Rect(HalfRect),
    /// Not known: it walks, has no shape, or stands where its holder does.
    Unknown,
}

/// Each bound piece's reach, by agent; none before the first
/// [`World::rereach`], or after a load.
#[derive(Clone, Debug, Default)]
pub(super) struct Reaches(Option<Map<u32, Reach>>);

impl Reaches {
    /// Whether made: [`World::rereach`] called since the last load.
    pub(super) const fn built(&self) -> bool {
        self.0.is_some()
    }

    /// Each bound piece and its reach, in no order.
    pub(super) fn iter(&self) -> impl Iterator<Item = (u32, Reach)> + '_ {
        self.0.iter().flatten().map(|(id, r)| (*id, *r))
    }
}

impl World {
    /// Bound piece `id`'s reach as the world has it now; none without a
    /// bound piece.
    fn reach_of(&self, id: u32) -> Option<Reach> {
        let piece = self.pieces.get(id)?;
        let shape = self.descriptors.shape(piece.code);
        let place = known_of(piece, self.units.motion(id));
        let rect = shape.zip(place).and_then(|(shape, place)| {
            let own = place.holder == 0 || place.state & super::Place::OWN_SPOT != 0;

            own.then(|| space::placed(shape.rect, place.standing))
        });

        Some(rect.map_or(Reach::Unknown, Reach::Rect))
    }

    /// Every bound piece's reach made anew.
    fn reaches_anew(&self) -> Map<u32, Reach> {
        self.pieces
            .keys()
            .filter_map(|id| self.reach_of(*id).map(|r| (*id, r)))
            .collect()
    }

    /// The kept reaches brought up to date: each piece marked since the
    /// last call anew, or every one on the first.
    pub(super) fn rereach(&mut self) {
        let mut stale = self.pieces.take_range();

        stale.append(&mut self.units.take_range());

        if self.reaches.0.is_none() {
            self.reaches.0 = Some(self.reaches_anew());
        } else {
            for id in stale {
                let reach = self.reach_of(id);

                if let Some(kept) = self.reaches.0.as_mut() {
                    match reach {
                        Some(r) => kept.insert(id, r),
                        None => kept.remove(&id),
                    };
                }
            }
        }

        if CHECK_REACH.on() {
            assert_eq!(
                self.reaches.0.as_ref(),
                Some(&self.reaches_anew()),
                "kept reaches differ from ones made anew: a change that did not mark its piece"
            );
        }
    }

    /// The kept reaches dropped: made whole at the next
    /// [`World::rereach`].
    pub(super) fn forget_reaches(&mut self) {
        self.reaches = Reaches::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::fourcc::FourCc;
    use crate::sim::world::kind;
    use crate::sim::world::places::Motion;
    use crate::sim::world::{Exposure, Piece, Place, Shape, Sight};

    const ID: u32 = 5;
    const CODE: FourCc = FourCc(*b"Atst");

    /// A world with one still piece of a one-cell shape at `at`.
    fn one_piece(at: (i32, i32)) -> World {
        let mut world = World::default();

        world.descriptors_mut().add_shape(
            CODE,
            Shape {
                rect: [0, 0, 1, 1],
                bounds: [0, 0, 32, 32],
                sight: Sight::Everyone,
                exposure: Exposure::Point,
            },
        );
        world.pieces.insert(
            ID,
            Piece {
                code: CODE,
                place: Some(Place {
                    kind: kind::Kind::VEHICLE,
                    fixed: Default::default(),
                    standing: at,
                    holder: 0,
                    state: 0,
                }),
                ..Piece::default()
            },
        );
        world.units.set_motion(ID, Motion::Still);
        world
    }

    /// The kept reach follows its piece: placed, moved, walking, held,
    /// gone; each time as one made anew.
    #[test]
    fn a_reach_follows_its_piece() {
        let mut world = one_piece((10, 10));
        let reach = |w: &World| w.reaches.iter().collect::<Vec<_>>();

        world.rereach();

        let first = reach(&world);

        assert!(matches!(first.as_slice(), [(ID, Reach::Rect(_))]));

        if let Some(place) = world.pieces.place_mut(ID) {
            place.standing = (40, 10);
        }
        world.rereach();

        assert_ne!(reach(&world), first);
        assert_eq!(world.reaches.0, Some(world.reaches_anew()));

        if let Some(place) = world.pieces.place_mut(ID) {
            place.holder = 99;
        }
        world.rereach();

        assert_eq!(reach(&world), vec![(ID, Reach::Unknown)]);

        world.units.set_motion(ID, Motion::Walking);
        world.rereach();

        assert_eq!(reach(&world), vec![(ID, Reach::Unknown)]);

        world.pieces.remove(ID);
        world.rereach();

        assert_eq!(reach(&world), vec![]);
    }
}
