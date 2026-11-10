//! Who stands where for a walker's plan: the collision grid, the
//! blockers on it, and the planner's view of them (`Among`). **Binary**
//! unless marked.
//!
//! ```text
//!  a change to a piece ──> Pieces / Units mark it stale
//!  before a plan or PathCost: World::regrid
//!     each stale piece: World::stood anew ──> Board::regrid
//!  Among, PathCost read World::collision
//!     with MAJESTY_CHECK_GRID set: World::check_grid, the grid built
//!     whole from every piece must equal it
//! ```

use crate::sim::world::kind;
use crate::sim::world::piece_state;
use std::collections::BTreeSet;

use crate::base::digest::Set;
use crate::base::switch::Switch;

use super::super::board::Stood;
use super::*;

/// Environment variable that turns on `World::check_grid`, e.g.
/// `MAJESTY_CHECK_GRID=1 cargo nextest run`.
pub static CHECK_GRID: Switch = Switch::new("MAJESTY_CHECK_GRID");

/// The units about a walker in one frame: `OrderMovePath`'s view.
pub(super) struct Among<'w> {
    world: &'w World,
    id: u32,
    goal: Goal,
    avoid: Avoid,
    /// The world's, read on first use ([`World::check_grid`]): most
    /// frames a walker plans nothing.
    grid: OnceCell<&'w Grid>,
    me: Occupant,
    cells: &'w [Cell],
    edge: Edge,
    waiter: Waiter,
    /// Units path after wait sent off: their place is unknown.
    nudged: Vec<u32>,
}

impl Among<'_> {
    /// The units the plan sent off.
    pub(super) fn nudged(self) -> Vec<u32> {
        self.nudged
    }

    /// The collision grid, checked once per walker and frame if asked.
    fn grid(&self) -> &Grid {
        self.grid.get_or_init(|| {
            self.world.check_grid();
            self.world.collision()
        })
    }

    /// Nearest free cell from `from`, counting `how`.
    fn near(&self, from: Cell, how: Blocking) -> Option<Cell> {
        let side = self.world.board.side();
        let d2 = |c: Cell| (from.0 - c.0).pow(2) + (from.1 - c.1).pow(2);

        flood(side, from, from, &d2, &mut |c| {
            if self.grid().fits(self.cells, self.edge, c, &self.me, how) {
                Visit::Take
            } else {
                Visit::Expand
            }
        })
    }

    /// Whether the walker fits on `at` with anyone in the way.
    fn fits_any(&self, at: Cell) -> bool {
        self.grid()
            .fits(self.cells, self.edge, at, &self.me, Blocking::Any)
    }

    /// The goal's half cell from `here`: beside the target, anyone
    /// blocking; else the nearest free cell to the point, what stands
    /// still blocking.
    fn end(&self, here: Cell) -> Option<Cell> {
        let fits =
            |c: Cell, how: Blocking| self.grid().fits(self.cells, self.edge, c, &self.me, how);

        match self.goal {
            Goal::Beside(target, _) => self.world.beside(self.id, target, here, &fits),
            Goal::Point(p) => {
                let side = self.world.board.side();
                let at = (
                    (p.0 / crate::sim::units::PER_HALF).clamp(0, side.0),
                    (p.1 / crate::sim::units::PER_HALF).clamp(0, side.1),
                );

                self.near(at, Blocking::Still)
            }
        }
    }

    /// Nudge candidate: over the walker's pad rect on
    /// `at`, row by row, the first unit on the grid not the walker, with a
    /// movement class, not frozen (`GSUnit`, `APV:63`) and of the
    /// walker's owner (`GSUnit`: hostile when owners differ, never
    /// for `AVh1`).
    fn candidate(&self, at: Cell) -> Option<u32> {
        let rect = self.world.shape_rect(self.id)?;
        let standing = self.world.standing();
        let owner = |id: u32| self.world.any_piece(id).map(|p| (p.owner, p.code.0));
        let (mine, my_code) = owner(self.id)?;
        let friendly = |id: u32| {
            owner(id).is_some_and(|(o, code)| {
                o == mine || code == NEVER_HOSTILE || my_code == NEVER_HOSTILE
            })
        };
        let mover = |id: u32| {
            let piece = self.world.any_piece(id);
            let gait = piece
                .and_then(|p| self.world.descriptors.body(p.code))
                .is_some_and(|b| b.gait.is_some());
            let frozen = piece.is_some_and(|p| p.word(FROZEN) != 0);

            gait && !frozen
        };

        rect_cells(space::placed(rect, at))
            .into_iter()
            .find_map(|cell| {
                standing.iter().find_map(|(other, spot, cells)| {
                    let hit = *other != self.id
                        && cells.iter().any(|d| (spot.0 + d.0, spot.1 + d.1) == cell);

                    (hit && mover(*other) && friendly(*other)).then_some(*other)
                })
            })
    }
}

impl Around for Among<'_> {
    fn plan(&mut self, from: Cell, keep: usize, how: Blocking) -> Vec<Cell> {
        match (self.near(from, how), self.end(from)) {
            (Some(start), Some(goal)) => {
                let free = |c: Cell| self.fits_any(c);

                // A path following a unit ranks its aim by the pad-rect gap
                // to it (`OrderMovePath` hands the unit to
                // path search); a path to a point by the goal.
                let unit = match self.goal {
                    Goal::Beside(target, _) => self
                        .world
                        .shape_rect(self.id)
                        .zip(self.world.footprint(target)),
                    Goal::Point(_) => None,
                };
                let gap =
                    |c: Cell| unit.map_or(0, |(rect, to)| space::gap2(space::placed(rect, c), to));
                let rank = match unit {
                    Some(_) => Rank::Gap(&gap),
                    None => Rank::Goal,
                };
                let to = aim(self.world.board.side(), goal, start, &rank, &free).unwrap_or(NOWHERE);

                // An avoid mode's squares block
                // the plan's cells; the start's node overwrites its mark.
                let mut marked = self.avoid.marked(self.grid(), start, &self.me, how);

                marked.remove(&start);

                let nodes = |marked: &Set<Cell>| {
                    plan(start, to, keep, PATH_HORIZON, &mut |c| {
                        self.grid().fits(self.cells, self.edge, c, &self.me, how)
                            && (marked.is_empty()
                                || !self
                                    .cells
                                    .iter()
                                    .any(|d| marked.contains(&(c.0 + d.0, c.1 + d.1))))
                    })
                    .nodes
                };
                let first = nodes(&marked);

                // Under two nodes with a radius: again with none.
                if first.len() < 2 && self.avoid.replans() {
                    nodes(&Set::default())
                } else {
                    first
                }
            }
            _ => vec![],
        }
    }

    fn at_goal(&mut self, from: Cell) -> bool {
        self.near(from, Blocking::Any) == self.end(from)
    }

    fn fits(&mut self, at: Cell, how: Blocking) -> bool {
        self.grid().fits(self.cells, self.edge, at, &self.me, how)
    }

    fn wait(&mut self, at: Cell) -> bool {
        let mut ids = vec![self.id];
        let mut units = vec![Waiter {
            wants: Some(at),
            ..self.waiter
        }];

        // Who stands is the same for every blocker asked here: found once.
        let standing = self.world.standing();
        let reach = World::reach(&standing);

        for (other, _, _) in &standing {
            if *other != self.id {
                ids.push(*other);
                units.push(self.world.waiter(*other));
            }
        }

        let blocker = |i: usize, c: Cell| {
            self.world
                .blocker(&standing, reach, ids[i], c)
                .and_then(|b| ids.iter().position(|x| *x == b))
        };

        wait_for(0, &units, &blocker)
    }

    fn nudge(&mut self, ahead: &[Cell]) {
        // A unit counts as its handle while moving, 0 while still: the
        // nudged goes when it counts less than the walker. The walker's
        // own track is out of the world while it runs: its own waiter.
        let weight = |id: u32| {
            let moving = if id == self.id {
                self.waiter.moving
            } else {
                self.world.waiter(id).moving
            };

            if moving {
                self.world.any_piece(id).map_or(0, |p| p.handle)
            } else {
                0
            }
        };
        let found = ahead.iter().find_map(|c| self.candidate(*c));

        self.nudged
            .extend(found.filter(|n| weight(*n) < weight(self.id)));
    }

    fn beyond(&mut self) -> Option<(i32, i32)> {
        let side = self.world.board.side();
        let point = match self.goal {
            Goal::Point(p) | Goal::Beside(_, Some(p)) => Some(p),
            Goal::Beside(_, None) => None,
        };
        let on_map = |p: (i32, i32)| {
            let (x, y) = (
                p.0 / crate::sim::units::PER_HALF,
                p.1 / crate::sim::units::PER_HALF,
            );

            (0..side.0).contains(&x) && (0..side.1).contains(&y)
        };

        point.filter(|p| !on_map(*p))
    }
}

impl World {
    /// What piece `id` registers on the collision grid now:
    /// its pad cells about its half cell, if
    /// the world knows its place, it is not held, not `DontBlock`, and
    /// its descriptor has a body.
    pub(in crate::sim::world) fn stood(&self, id: u32) -> Option<Stood> {
        let p = self.pieces.any(id)?;
        let place = self.known(id)?;
        let body = self.descriptors.body(p.code)?;
        let on = id != super::super::NO_AGENT_ID
            && place.holder == 0
            && place.state & piece_state::DONT_BLOCK == 0;
        let at = place.standing;

        on.then(|| Stood {
            who: Occupant {
                handle: p.handle,
                info: body.layer,
                state: u32::from(place.state),
                vehicle: place.is_vehicle(),
                building: place.is_building() && !is_chest(p.code.0),
                chest: place.is_building() && is_chest(p.code.0),
                clearable: Self::clearable(p, place),
                tramples: false,
                dont_block: body.state & piece_state::DONT_BLOCK != 0,
            },
            cells: body
                .cells
                .iter()
                .map(|(x, y)| (at.0 + x, at.1 + y))
                .collect(),
        })
    }

    /// The collision grid brought up to date: built whole the first time
    /// (and after a restore), then each piece changed since unregistered
    /// and registered anew, as the engine does at each change.
    pub(in crate::sim::world) fn regrid(&mut self) {
        let mut stale = self.pieces.take_stale();

        stale.append(&mut self.units.take_stale());

        if self.board.grid().is_none() {
            let all = self
                .pieces
                .all()
                .filter_map(|(id, _)| self.stood(*id).map(|s| (*id, s)))
                .collect();

            self.board.grid_whole(all, self.descriptors.bodies());
        } else {
            for id in stale {
                let stood = self.stood(id);

                self.board.regrid(id, stood);
            }
        }
    }

    /// The collision grid, as [`World::regrid`] last left it: the grid
    /// the planner reads. With [`CHECK_GRID`] set, checked against one
    /// built whole from every piece, once per reader
    /// ([`World::check_grid`]).
    ///
    /// # Panics
    ///
    /// Before the first [`World::regrid`].
    pub(in crate::sim::world) fn collision(&self) -> &Grid {
        debug_assert!(
            !self.pieces.any_stale() && !self.units.any_stale(),
            "the grid read before World::regrid"
        );

        self.board.grid().expect("World::regrid before reading")
    }

    /// The collision grid built whole from every piece and the scatter:
    /// what the kept grid must equal.
    fn grid_rebuilt(&self) -> Grid {
        let mut grid = Grid::new(self.board.side().0);
        let pieces = self.pieces.all().filter_map(|(id, _)| self.stood(*id));

        for s in self
            .board
            .scattered(self.descriptors.bodies())
            .chain(pieces)
        {
            grid.stand(s.who, s.cells);
        }

        grid
    }

    /// With [`CHECK_GRID`] set, [`World::assert_grid`]. A debugging aid,
    /// for when a divergence points at the grid: each check rebuilds it
    /// whole, which costs more than the kept grid saves.
    pub(in crate::sim::world) fn check_grid(&self) {
        if CHECK_GRID.on() {
            self.assert_grid();
        }
    }

    /// Panics where the kept grid differs from one built whole: a change
    /// that did not mark its piece stale. Names the first differing half
    /// cell, each grid's occupants there, and the pieces that reach it, as
    /// kept or now.
    pub(in crate::sim::world) fn assert_grid(&self) {
        if let Some((cell, kept, now)) = self.collision().differs(&self.grid_rebuilt()) {
            let reaches = |s: Option<&Stood>| s.is_some_and(|s| s.cells.contains(&cell));
            let ids: BTreeSet<u32> = self
                .pieces
                .all()
                .map(|(id, _)| *id)
                .chain(self.board.stood_ids())
                .collect();
            let pieces: Vec<String> = ids
                .into_iter()
                .filter(|id| reaches(self.board.stood(*id)) || reaches(self.stood(*id).as_ref()))
                .map(|id| {
                    format!(
                        "#{id} {:?} kept {:?} now {:?} motion {:?}",
                        self.pieces.any(id).map(|p| p.code),
                        self.board.stood(id).map(|s| s.who),
                        self.stood(id).map(|s| s.who),
                        self.units.motion(id),
                    )
                })
                .collect();

            panic!(
                "collision grid at {cell:?}: kept {kept:?}, built whole {now:?}; pieces: {pieces:#?}"
            );
        }
    }

    /// `GSUnit`: scatter; a building ruined or agentless, as
    /// `DeleteAgent` leaves it; an emptied chest.
    pub(in crate::sim::world) fn clearable(
        piece: &super::super::Piece,
        place: &super::super::Place,
    ) -> bool {
        let get = |k: i32| piece.attributes.get(&k).copied().unwrap_or_default();
        let kind = match place.kind {
            kind::Kind::SCATTER => clearing::Kind::Scatter,
            kind::Kind::BUILDING => clearing::Kind::Building,
            _ => clearing::Kind::Other,
        };
        let occupant = clearing::Occupant {
            kind,
            code: piece.code,
            ruined: place.state & piece_state::RUINED != 0,
            agent: get(super::super::AGENT_ID.packed()) != 0,
            hit_points: get(super::super::combat::hit_points()),
        };

        occupant.cleared(clearing::Pass::Around, clearing::Time::Playing)
    }

    /// What walker `id`, weighed as `waiter`, finds about it this frame.
    pub(super) fn among(
        &self,
        id: u32,
        (goal, avoid): (Goal, Avoid),
        waiter: Waiter,
    ) -> Option<Among<'_>> {
        let piece = self.pieces.get(id)?;
        let body = self.descriptors.body(piece.code)?;
        let me = Occupant {
            handle: piece.handle,
            info: body.layer & LAYERS,
            state: 0,
            vehicle: true,
            building: false,
            chest: false,
            clearable: false,
            tramples: body.gait.is_some_and(|g| g.flags & TRAMPLES != 0),
            dont_block: false,
        };

        Some(Among {
            world: self,
            id,
            goal,
            avoid,
            grid: OnceCell::new(),
            me,
            cells: &body.cells,
            edge: body.edge,
            waiter,
            nudged: Vec::new(),
        })
    }

    /// Beside free cell: from the target's half cell,
    /// by pad-rect gap then distance from `here`, only cells beside it taken
    /// or passed; none free, the target's own half cell. **Game**, the
    /// dwarves game, 391,100 ms: skeleton 53 heads for a hemmed-in peasant.
    pub(super) fn beside(
        &self,
        id: u32,
        target: u32,
        here: Cell,
        fits: &dyn Fn(Cell, Blocking) -> bool,
    ) -> Option<Cell> {
        let rect = self.shape_rect(id)?;
        let to = self.footprint(target)?;
        let at = self.spot(target)?;
        let mask = self.body_of(target).and_then(|b| b.mask.as_ref());
        let near = |c: Cell| space::gap2(space::placed(rect, c), to);

        flood(self.board.side(), at, here, &near, &mut |c| match (
            space::adjacent(space::placed(rect, c), to, mask, at),
            fits(c, Blocking::Any),
        ) {
            (true, true) => Visit::Take,
            (true, false) => Visit::Expand,
            _ => Visit::Skip,
        })
        .or(Some(at))
    }

    /// Piece `id`'s pad rect about its own half cell.
    fn shape_rect(&self, id: u32) -> Option<HalfRect> {
        self.descriptors
            .shape(self.any_piece(id)?.code)
            .map(|s| s.rect)
    }

    /// Units adjacent: each at its
    /// agent position.
    pub(super) fn adjacent(&self, a: u32, b: u32) -> Option<bool> {
        let mask = self.body_of(b).and_then(|body| body.mask.as_ref());

        Some(space::adjacent(
            self.footprint(a)?,
            self.footprint(b)?,
            mask,
            self.spot(b)?,
        ))
    }

    /// The first unit in the way of `id`'s pad rect on `at`:
    /// the rect's rows, then
    /// columns, the first occupant on a shared layer, not ruined. **Game**, the
    /// no-budget defeat game, 287,250 ms: ratman 15 wants (22, 192), where
    /// standing ratman 16 reaches the rect's upper rows before walking
    /// champion 11 its lower ones; it waits for no one and plans round
    /// both, as the probe shows.
    ///
    /// `standing` is [`World::standing`]'s list, `reach` its
    /// [`World::reach`].
    pub(in crate::sim::world) fn blocker(
        &self,
        standing: &[(u32, Cell, &[Cell])],
        reach: i32,
        id: u32,
        at: Cell,
    ) -> Option<u32> {
        let rect = self.shape_rect(id)?;
        let side = self.board.side();
        let layer = |who: u32| self.body_of(who).map_or(0, |b| b.layer);
        let mine = layer(id);
        // blocker default with 0x400 alone: a ruin blocks no one.
        let ruined = |who: u32| {
            self.known(who)
                .is_some_and(|p| p.state & piece_state::RUINED != 0)
        };

        // Rows, then columns, of the pad rect clipped to the map; a cell's
        // occupants in turn. **Inferred**: a
        // cell's list in agent order.
        let placed = space::placed(rect, at);
        let on_map = |c: &Cell| c.0 >= 0 && c.1 >= 0 && c.0 < side.0 && c.1 < side.1;
        let inside = |c: &Cell| {
            (placed[0]..placed[2]).contains(&c.0) && (placed[1]..placed[3]).contains(&c.1)
        };

        // Within `reach` of the rect, by half cell alone: its cells need
        // not be read.
        let close = |spot: &Cell| {
            spot.0 + reach >= placed[0]
                && spot.0 - reach < placed[2]
                && spot.1 + reach >= placed[1]
                && spot.1 - reach < placed[3]
        };

        // Only those reaching a cell of the rect can be first, kept in
        // agent order: a few, where the list holds every piece.
        let near: Vec<&(u32, Cell, &[Cell])> = standing
            .iter()
            .filter(|(other, spot, cells)| {
                *other != id
                    && close(spot)
                    && cells.iter().any(|d| {
                        let c = (spot.0 + d.0, spot.1 + d.1);

                        on_map(&c) && inside(&c)
                    })
            })
            .collect();

        rect_cells(placed)
            .iter()
            .filter(|c| on_map(c))
            .find_map(|cell| {
                near.iter().find_map(|(other, spot, reach)| {
                    let hit = reach.iter().any(|d| (spot.0 + d.0, spot.1 + d.1) == *cell)
                        && layer(*other) & mine & LAYERS != 0
                        && !ruined(*other);

                    hit.then_some(*other)
                })
            })
    }

    /// How far any of `standing` reaches from its half cell, either way
    /// on either axis: (2, -1) reaches 2.
    pub(in crate::sim::world) fn reach(standing: &[(u32, Cell, &[Cell])]) -> i32 {
        standing
            .iter()
            .flat_map(|(_, _, cells)| cells.iter())
            .map(|d| d.0.abs().max(d.1.abs()))
            .max()
            .unwrap_or(0)
    }

    /// Every unheld piece on the grid, its half cell and cells.
    pub(in crate::sim::world) fn standing(&self) -> Vec<(u32, Cell, &[Cell])> {
        let mut out: Vec<(u32, Cell, &[Cell])> = self
            .pieces
            .iter()
            .filter_map(|(id, p)| {
                let place = self.known(*id)?;
                let body = self.descriptors.body(p.code)?;

                (place.holder == 0 && place.state & piece_state::DONT_BLOCK == 0).then_some((
                    *id,
                    place.standing,
                    body.cells.as_slice(),
                ))
            })
            .collect();

        out.sort_by_key(|(id, _, _)| *id);
        out
    }

    /// Piece `id` as a blocked walker weighs it: a walker's own, else
    /// standing, no faster than anyone.
    pub(super) fn waiter(&self, id: u32) -> Waiter {
        self.units.tracks(id).map_or(
            Waiter {
                wants: None,
                pace: Fixed::ZERO,
                moving: false,
            },
            |t| t.walker.waiter(),
        )
    }
}
