//! Where a unit may stand: the half cells standing pieces and scatter take
//! on the collision grid, place search over them, and a spawner's
//! exit. **Binary** unless marked; the search and the footprint test are
//! [`crate::sim::placement`]'s.

use crate::sim::world::piece_state;
use std::collections::BTreeSet;

use super::{NO_AGENT_ID, Place, World, bodies::Body};
use crate::base::digest::{Map, Set};
use crate::base::switch::Switch;
use crate::sim::movement::plan::{Grid, Occupant};
use crate::sim::placement::Edge;

/// Environment variable that turns on `World::check_occupied`, e.g.
/// `MAJESTY_CHECK_OCCUPIED=1 cargo nextest run`.
pub static CHECK_OCCUPIED: Switch = Switch::new("MAJESTY_CHECK_OCCUPIED");

/// What a place search counts in the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blockers {
    /// Everything on a shared layer: flags 0, a mobile.
    All,
    /// Not what a building clears: flag `0x1000` lets an occupant whose
    /// method says so through; `GSUnit`
    /// says so for scatter, unit kind 2, a ruined or
    /// agentless building and an emptied chest (`BuildingRec`,
    /// the build preview).
    Standing,
    /// Everything but agent `n`'s own piece: a dying unit's search.
    Others(u32),
    /// Every occupant of the grid but agent `n`'s own piece, whatever its
    /// layer: a dying unit's search, place test not self,
    /// the same test as [`Blockers::Anyone`]'s. **Game**,
    /// the probed Barren Waste: a Rust Spitter dying at 216,450 ms lies a
    /// half cell east of a swamp walkover, no `BlockGround`.
    NotSelf(u32),
    /// Every occupant of the grid, whatever its layer: the test
    /// `VehicleRec` passes for a resource plant, `AVk1` to `AVk3`
    /// (place test avk: blocked by any occupant but itself).
    Anyone,
}

/// `Info` bits 4 and 5, `BlockGround` and `BlockFlying`: two pieces block
/// each other when they share one.
pub const LAYERS: u32 = 0x30;

/// The half cells a place search finds taken ([`World::occupied`]).
///
/// ```text
///  Kept   the collision grid's occupants as `who` counts them on `layer`,
///         plus `changed`: each piece changed since the grid was last
///         brought up to date off as registered, on as it stands now
///  Whole  every piece and scatter object's cells: before the grid is built
/// ```
pub(super) enum Taken<'w> {
    Kept {
        grid: &'w Grid,
        layer: u32,
        who: Blockers,
        changed: Map<(i32, i32), i32>,
    },
    Whole(Set<(i32, i32)>),
}

impl Taken<'_> {
    /// Whether half cell `c` is taken.
    fn contains(&self, c: (i32, i32)) -> bool {
        match self {
            Taken::Kept {
                grid,
                layer,
                who,
                changed,
            } => {
                let here = grid.at(c).filter(|o| counted(o, *layer, *who)).count();
                let here = i32::try_from(here).unwrap_or(i32::MAX);

                here + changed.get(&c).copied().unwrap_or(0) > 0
            }
            Taken::Whole(set) => set.contains(&c),
        }
    }
}

/// Whether a grid occupant is in the way of a search on `layer` by
/// `who`, as [`World::occupied_whole`] counts its piece or scatter
/// object: not with `DontBlock`; on a shared layer, or any layer for
/// [`Blockers::Anyone`] and [`Blockers::NotSelf`]; for
/// [`Blockers::Standing`] nothing clearable (scatter is).
fn counted(o: &Occupant, layer: u32, who: Blockers) -> bool {
    let any = matches!(who, Blockers::Anyone | Blockers::NotSelf(_));
    let cleared = who == Blockers::Standing && o.clearable;

    !o.dont_block && (any || o.info & layer != 0) && !cleared
}

impl World {
    /// Whether any piece the world cannot place might stand in the way.
    pub(super) fn unplaced(&self) -> bool {
        self.first_unplaced().is_some()
    }

    /// The half cells every standing piece and scatter object covers on
    /// `layer`, as [`World::occupied_whole`] counts them: read off the
    /// collision grid once it is built, not built anew each search.
    fn occupied(&self, layer: u32, who: Blockers) -> Taken<'_> {
        let taken = match self.board.grid() {
            Some(grid) => Taken::Kept {
                grid,
                layer,
                who,
                changed: self.changed(layer, who),
            },
            None => Taken::Whole(self.occupied_whole(layer, who)),
        };

        self.check_occupied(&taken, layer, who);
        taken
    }

    /// What the grid lacks for a search on `layer` by `who`, by half
    /// cell: each piece changed since [`World::regrid`] off where it was
    /// registered and on where it stands now; the searcher's own piece
    /// off.
    fn changed(&self, layer: u32, who: Blockers) -> Map<(i32, i32), i32> {
        let own = match who {
            Blockers::Others(id) | Blockers::NotSelf(id) => Some(id),
            Blockers::All | Blockers::Standing | Blockers::Anyone => None,
        };
        let ids: BTreeSet<u32> = self
            .pieces
            .stale()
            .iter()
            .chain(self.units.stale())
            .copied()
            .chain(own)
            .collect();
        let mut changed = Map::default();
        let mut add = |o: &Occupant, cells: &[(i32, i32)], by: i32| {
            if counted(o, layer, who) {
                for c in cells {
                    *changed.entry(*c).or_insert(0) += by;
                }
            }
        };

        for id in ids {
            if let Some(was) = self.board.stood(id) {
                add(&was.who, &was.cells, -1);
            }

            if Some(id) != own
                && let Some(now) = self.stood(id)
            {
                add(&now.who, &now.cells, 1);
            }
        }

        changed
    }

    /// With [`CHECK_OCCUPIED`] set, a kept answer checked against
    /// [`World::occupied_whole`] over the whole map. A debugging aid:
    /// each check builds the set whole.
    ///
    /// # Panics
    ///
    /// Where the two differ, naming the half cell.
    fn check_occupied(&self, taken: &Taken<'_>, layer: u32, who: Blockers) {
        if CHECK_OCCUPIED.on()
            && let Taken::Kept { grid, .. } = taken
        {
            let whole = self.occupied_whole(layer, who);
            let side = grid.side;
            let differs = (0..side)
                .flat_map(|y| (0..side).map(move |x| (x, y)))
                .find(|c| taken.contains(*c) != whole.contains(c));

            assert!(
                differs.is_none(),
                "occupied at {differs:?} on layer {layer:#x} by {who:?}: kept {}, built whole {}",
                differs.is_some_and(|c| taken.contains(c)),
                differs.is_some_and(|c| whole.contains(&c)),
            );
        }
    }

    /// The half cells every standing piece and scatter object covers on
    /// `layer`: unit grid register registers an unheld unit over its mask or
    /// rect. [`Blockers::Standing`] leaves out what a building clears.
    fn occupied_whole(&self, layer: u32, who: Blockers) -> Set<(i32, i32)> {
        let scatter = match who {
            Blockers::All | Blockers::Others(_) | Blockers::NotSelf(_) | Blockers::Anyone => {
                self.board.scatter()
            }
            Blockers::Standing => &[],
        };
        let pieces = self
            .pieces
            .all()
            .filter(|(id, _)| {
                **id != NO_AGENT_ID
                    && who != Blockers::Others(**id)
                    && who != Blockers::NotSelf(**id)
            })
            .filter_map(|(id, p)| {
                let place = self.known(*id)?;

                let cleared = who == Blockers::Standing && Self::clearable(p, place);
                // `DontBlock` keeps a unit off the grid.
                let gridded = place.state & piece_state::DONT_BLOCK == 0;

                (place.holder == 0 && !cleared && gridded).then_some((p.code, place.standing))
            });

        pieces
            .chain(scatter.iter().copied())
            .filter_map(|(code, at)| {
                let body = self.descriptors.body(code)?;
                // `Info` `DontBlock` keeps it off the grid as made
                // A cast Gate blocks no dying unit.
                let gridded = body.state & piece_state::DONT_BLOCK == 0;
                let any = matches!(who, Blockers::Anyone | Blockers::NotSelf(_));

                (gridded && (any || body.layer & layer != 0)).then(|| {
                    body.cells
                        .iter()
                        .map(move |(x, y)| (at.0 + x, at.1 + y))
                        .collect::<Vec<_>>()
                })
            })
            .flatten()
            .collect()
    }

    /// Place search for `body` from `at`, 32nds of a cell, with
    /// flags 0: the nearest half cell where no cell of its footprint is
    /// taken on its layer, in 32nds.
    pub(super) fn place(&self, body: &Body, at: (i32, i32)) -> Option<(i32, i32)> {
        self.place_among(body, at, Blockers::All)
    }

    /// [`World::place`], blocked by `who`.
    pub(super) fn place_among(
        &self,
        body: &Body,
        at: (i32, i32),
        who: Blockers,
    ) -> Option<(i32, i32)> {
        self.free_for(body, (&body.cells, body.edge), at, who)
    }

    /// `BuildingRec`'s search with a margin, place flags without bit
    /// 1: footprint blocked tests `GSUnit`'s
    /// ring for the margin, the half cells its placement clears, not its
    /// pad; what a building clears lets it through (flag `0x1000`).
    ///
    /// The ring is a mask, rasterised like a pad, so it may take the map's
    /// last row and column whatever the pad's own edge test
    /// (mask blocked, as the preview's [`World::crowder`]).
    /// **Game**: The Barren Waste's chests 15, 24 and 31 stand in half
    /// cell 246 and 248 of 256, their rings' last half cells on the map's
    /// last.
    pub(super) fn place_ringed(&self, body: &Body, at: (i32, i32)) -> Option<(i32, i32)> {
        let ring: Option<Vec<(i32, i32)>> = body.ring.as_ref().map(|r| r.halves().collect());

        match ring {
            Some(ring) => self.free_for(body, (&ring, Edge::Mask), at, Blockers::Standing),
            None => self.free_for(body, (&body.cells, body.edge), at, Blockers::Standing),
        }
    }

    /// The nearest half cell from `at` where none of `cells` about it is
    /// taken on `body`'s layer by `who`, in 32nds.
    fn free_for(
        &self,
        body: &Body,
        footprint: (&[(i32, i32)], Edge),
        at: (i32, i32),
        who: Blockers,
    ) -> Option<(i32, i32)> {
        let taken = self.occupied(body.layer, who);

        crate::sim::placement::search(footprint, self.board.side(), at, |c| taken.contains(c))
    }

    /// Footprint blocked for `cells` about a half cell, blocked by
    /// `who` on `body`'s layer.
    pub(super) fn blocked<'s>(
        &'s self,
        body: &Body,
        footprint: (&'s [(i32, i32)], Edge),
        who: Blockers,
    ) -> impl Fn((i32, i32)) -> bool + 's {
        let taken = self.occupied(body.layer, who);

        crate::sim::placement::blocked(footprint, self.board.side(), move |c| taken.contains(c))
    }

    /// `GenUnit`: where a unit spawned by `id` appears, 32nds of a
    /// cell. A held spawner's is its holder's. The point is the slot's its
    /// art draws: a new game piece's saved direction,
    /// else 0: a `Directionless` piece made in play draws its slot on the
    /// world RNG, not modelled.
    pub(super) fn exit(&self, id: u32) -> Option<(i32, i32)> {
        let place = self.known(id)?;

        if place.holder != 0 && place.state & Place::OWN_SPOT == 0 {
            self.exit(self.holder(place.holder)?)
        } else {
            let body = self.descriptors.body(self.any_piece(id)?.code)?;
            let (x, y) = self.position(id)?;
            let direction = self.units.direction(id).unwrap_or_default();
            let slot = body.facing.slot(direction, body.exits.len());
            let exit = body.exits.get(slot).copied().unwrap_or_default();

            Some((x + exit.0, y + exit.1))
        }
    }
}
