//! The board: the map's physical state. Heights, cell paint, vertex
//! shade, the scatter on the collision grid, every agentless object as
//! drawn, the start pieces' saved sets, and a count of the changes.
//!
//! ```text
//!  a save's terrain, records ──> Board::of_terrain, lay, strew, keep_look
//!  generated ground, scatter ──> Board::generated, lay, raise
//!  a placement (world::building_ground) ──> level    heights, scatter reseated
//!                                  clear    scatter a ring reaches
//!                                  pave     Dirt under the pad
//!                                  relight  shade baked anew
//!      each change moves relaid: a drawing redraws when it moves
//!  drawing ──> World::board: heights, shades, paint, strewn, relaid
//!
//!  the collision grid, map
//!    built whole on first read (descriptors known), and after a restore
//!    scatter     raise stands it, clear takes it off
//!    a piece     World::regrid: its entry anew (Board::regrid)
//! ```
//!
//! The scatter and its stands move in step, and a rule that changes a
//! gridded object changes its drawn record too.
//!
//! The grid is the board's, not the pieces' store's, as the engine's is
//! its map's: the scatter on it is the board's own, and one owner
//! writes every cell. A piece's entry comes from the piece, its motion
//! and its descriptor, so the world works it out and hands it here.

use crate::sim::world::piece_state;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::base::digest::Map;

use super::bodies::{Bodies, Body};
use super::building_ground::{Lays, PAD_CLASS, Painted, Stand};
use super::new_map::OnGrid;
use super::scatter::{Drawn, Strewn};
use super::walking::HeightMap;
use crate::base::fixed::Fixed;
use crate::data::fourcc::FourCc;
use crate::data::rgs::class_of;
use crate::sim::movement::plan::{Grid, Occupant};
use crate::sim::paint::{self, Cell};
use crate::sim::placement::{self, Heights, Pad};
use crate::sim::units::{HALVES, PER_HALF};
use crate::sim::{light, movement};

/// The map's physical state.
#[derive(Clone, Debug, Default)]
pub struct Board {
    /// The half-cell grid's size.
    side: (i32, i32),
    /// The ground walkers stand on. This and the paint are shared with
    /// a copy of the world until a placement changes them.
    ground: Option<Arc<HeightMap>>,
    /// Each cell's paint: what a building's pad paints over.
    paint: Option<Arc<Painted>>,
    /// Each vertex's shade byte as the save had it; `None`
    /// once a placement relit the ground, or generated
    /// ([`Board::shades`]).
    shade: Option<Vec<u8>>,
    /// Ground scatter: no agent, but it blocks a placement. Code and half
    /// cell.
    scatter: Vec<(FourCc, (i32, i32))>,
    /// Where each scatter object stands, and how high, in step with it.
    stands: Vec<Stand>,
    /// Scatter and terrain features with no agent, as drawn
    /// ([`super::scatter`]).
    strewn: Vec<Strewn>,
    /// Each save piece record's set and slot, by handle.
    looks: BTreeMap<u32, Drawn>,
    /// Start scatter and terrain features whose slot plays frames: code,
    /// half cell, grid ([`World::start_frames`](super::World::start_frames)).
    scenery: Vec<(FourCc, (i32, i32), OnGrid)>,
    /// Changes to ground, paint, shade or scatter so far
    /// ([`Board::relaid`]).
    relaid: u32,
    /// Scatter a clearing in play deleted, by code and half cell: gone
    /// at the next drain ([`Board::clear_later`]).
    clearing: Vec<(FourCc, (i32, i32))>,
    /// The collision grid. Not saved: a restored board builds it anew.
    collision: Collision,
    /// The scatter a sight pass samples ([`Board::exposed`]). Not saved.
    exposed: Exposed,
}

/// A scatter object a sight pass samples: its code and half cell, where
/// it stands ([`Stand::fixed`]) and its sight.
pub(super) type Sampled = ((FourCc, (i32, i32)), (Fixed, Fixed), super::Sight);

/// [`Board::exposed`]'s list, as measured at a [`Board::relaid`]: kept
/// until the scatter or its stands change.
#[derive(Clone, Debug, Default)]
struct Exposed {
    at: Option<u32>,
    list: Vec<Sampled>,
}

/// The collision grid, map, and what each piece put on it. The
/// board owns it, as the engine's map does: scatter changes here, a
/// piece's entry by [`Board::regrid`] (`World::regrid`).
#[derive(Clone, Debug, Default)]
struct Collision {
    /// `None` until built whole: a board is made before the descriptors
    /// that measure what stands on it.
    grid: Option<Grid>,
    /// Each agent's entry as last registered, shared with a copy of the
    /// world.
    stood: Map<u32, Arc<Stood>>,
}

/// What one unit registers on the collision grid:
/// who, over which half cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stood {
    pub who: Occupant,
    pub cells: Vec<(i32, i32)>,
}

impl Stood {
    /// A scatter object `code` at half cell `at`, measured by `bodies`:
    /// clearable, on its body's layers.
    pub(super) fn scatter(code: FourCc, at: (i32, i32), bodies: &Bodies) -> Option<Stood> {
        bodies.get(&code).map(|body| Stood {
            who: Occupant {
                handle: 0,
                info: body.layer,
                state: 0,
                vehicle: false,
                building: false,
                chest: false,
                clearable: true,
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
}

impl Board {
    /// A generated map: `ground`'s heights, `cells` a row each; no shade
    /// saved, no objects yet.
    pub(super) fn generated(ground: &dyn Heights, cells: &[Cell]) -> Board {
        let side = ground.across().saturating_sub(1).cast_signed();

        Board {
            side: (side * HALVES, side * HALVES),
            ground: Some(Arc::new(HeightMap::of(ground))),
            paint: Some(Arc::new(Painted::new(side, cells.to_vec()))),
            ..Board::default()
        }
    }

    /// A scatter object on the collision grid: `code` at half cell `at`,
    /// standing at `stand`. For a board being made, its grid not built
    /// yet; in play [`Board::raise`] lays.
    pub(super) fn lay(&mut self, code: FourCc, at: (i32, i32), stand: Stand) {
        debug_assert!(
            self.collision.grid.is_none(),
            "lay before the grid is built"
        );

        // Measured anew when next read.
        self.ungrid();
        self.scatter.push((code, at));
        self.stands.push(stand);
        self.relaid += 1;
    }

    /// An agentless object's drawn record.
    pub(super) fn strew(&mut self, strewn: Strewn) {
        self.strewn.push(strewn);
        self.relaid += 1;
    }

    /// An agentless object made: laid on the grid where its record says
    /// so, at its half cell ([`Strewn::half`]), and strewn.
    pub(super) fn raise(&mut self, strewn: Strewn, bodies: &Bodies) {
        if strewn.grid == OnGrid::On {
            let at = strewn.half();

            if let (Some(grid), Some(s)) = (
                self.collision.grid.as_mut(),
                Stood::scatter(strewn.code, at, bodies),
            ) {
                grid.stand(s.who, s.cells);
            }

            self.scatter.push((strewn.code, at));
            self.stands.push(strewn.stand);
            self.relaid += 1;
        }

        self.strew(strewn);
    }

    /// The start's objects whose slot plays frames.
    pub(super) fn set_scenery(&mut self, scenery: Vec<(FourCc, (i32, i32), OnGrid)>) {
        self.scenery = scenery;
    }

    /// One more of [`Board::set_scenery`]'s.
    pub(super) fn stage(&mut self, code: FourCc, at: (i32, i32), grid: OnGrid) {
        self.scenery.push((code, at, grid));
    }

    /// Building flatten for pad `mask` at half cell `at`: phase 0
    /// and 1 level it ([`placement::flatten`]), then phase 1's reseat:
    /// each unit on the grid in [`placement::reseated`]
    /// that is not moving and not the building, by unit move to
    /// where it stands, takes the levelled ground's height. Of the
    /// board's, only scatter keeps one, measured by `bodies`; what stands
    /// outside keeps its own, though the slope clamp moved the ground
    /// under it.
    pub(super) fn level(&mut self, at: (i32, i32), mask: &Pad, bodies: &Bodies) {
        if let Some(ground) = self.ground.as_mut().map(Arc::make_mut) {
            let plateau = placement::flatten(ground, at, mask);

            ground.level(&plateau);
            self.reseat(at, mask, bodies);
        }

        self.relaid += 1;
    }

    /// [`Board::level`]'s reseat.
    fn reseat(&mut self, at: (i32, i32), mask: &Pad, bodies: &Bodies) {
        if let Some(ground) = self.ground.as_deref() {
            let [x0, y0, x1, y1] = placement::reseated(at, mask, ground.across());
            let inside = |(x, y): (i32, i32)| (x0..x1).contains(&x) && (y0..y1).contains(&y);

            let reseated = |code: &FourCc, spot: (i32, i32)| {
                bodies.get(code).is_some_and(|b| {
                    b.cells
                        .iter()
                        .any(|(x, y)| inside((spot.0 + x, spot.1 + y)))
                })
            };

            for ((code, spot), stand) in self.scatter.iter().zip(self.stands.iter_mut()) {
                if reseated(code, *spot) {
                    *stand = Stand::on(Some(ground), stand.fixed);
                }
            }

            // The drawn records of the same objects, by the same rule.
            for s in self.strewn.iter_mut().filter(|s| s.grid == OnGrid::On) {
                if reseated(&s.code, s.half()) {
                    s.stand = Stand::on(Some(ground), s.stand.fixed);
                }
            }
        }
    }

    /// The scatter a clearing deletes gone: what `stays` refuses, by code
    /// and half cell, with its stand, its drawn record and its place on
    /// the collision grid, measured by `bodies`. Off the grid nothing
    /// clears.
    pub(super) fn clear(&mut self, stays: impl Fn(&FourCc, (i32, i32)) -> bool, bodies: &Bodies) {
        let kept: Vec<bool> = self
            .scatter
            .iter()
            .map(|(code, spot)| stays(code, *spot))
            .collect();

        if let Some(grid) = self.collision.grid.as_mut() {
            let gone = self.scatter.iter().zip(&kept).filter(|(_, k)| !**k);

            for s in gone.filter_map(|((code, at), _)| Stood::scatter(*code, *at, bodies)) {
                grid.leave(&s.who, s.cells);
            }
        }

        let mut keep = kept.iter().copied();

        self.scatter.retain(|_| keep.next().unwrap_or(true));

        let mut keep = kept.iter().copied();

        self.stands.retain(|_| keep.next().unwrap_or(true));
        self.strewn
            .retain(|s| s.grid == OnGrid::Off || stays(&s.code, s.half()));
        self.relaid += 1;
    }

    /// [`Board::clear`] deferred: what `stays` refuses goes at the next
    /// [`Board::drain`]. A clearing in play deletes each scatter object
    /// by `GE_Level`, which queues it for `GS_Level` slot 34, after
    /// the next frame's orders (**Game**: the tournament probe's rat's
    /// nest placed at frame 4839; ranger 25 plans at 4840 around the
    /// `NGa1` it clears).
    pub(super) fn clear_later(&mut self, stays: impl Fn(&FourCc, (i32, i32)) -> bool) {
        let gone: Vec<(FourCc, (i32, i32))> = self
            .scatter
            .iter()
            .filter(|(code, spot)| !stays(code, *spot) && !self.clearing.contains(&(*code, *spot)))
            .copied()
            .collect();

        self.clearing.extend(gone);
    }

    /// The deletes [`Board::clear_later`] queued, done.
    pub(super) fn drain(&mut self, bodies: &Bodies) {
        let gone = std::mem::take(&mut self.clearing);

        if !gone.is_empty() {
            self.clear(|code, spot| !gone.contains(&(*code, spot)), bodies);
        }
    }

    /// `GE_Level` method `(GR00)`: Dirt over `body`'s pad at half cell `at`,
    /// cell by covered half cell. **Inferred**: `GR00`'s nibble is 0, as
    /// [`crate::data::rgs::class_of`] reads it; a tileset lacking it,
    /// which the engine leaves unpainted, is not modelled.
    #[allow(
        clippy::manual_midpoint,
        reason = "a half cell to its cell, truncated toward zero"
    )]
    pub(super) fn pave(&mut self, body: &Body, at: (i32, i32)) {
        let class = class_of(PAD_CLASS);

        if let (Lays::Dirt, Some(mask), Some(painted), Some(class)) = (
            body.lays,
            body.mask.as_ref(),
            self.paint.as_mut().map(Arc::make_mut),
            class,
        ) {
            for (x, y) in mask.halves() {
                paint::point(painted, ((at.0 + x) / 2, (at.1 + y) / 2), class, 0);
            }
        }

        self.relaid += 1;
    }

    /// The shade relit after a placement: `Map` over the whole map
    /// from now on. **Inferred**: the engine bakes a rect about the
    /// edit; the same bytes wherever its stale row does not reach
    /// ([`light`]).
    pub(super) fn relight(&mut self) {
        self.shade = None;
        self.relaid += 1;
    }

    /// The half-cell grid's size.
    #[must_use]
    pub const fn side(&self) -> (i32, i32) {
        self.side
    }

    /// The map's size in 32nds of a cell.
    #[must_use]
    pub const fn extent(&self) -> (i32, i32) {
        (self.side.0 * PER_HALF, self.side.1 * PER_HALF)
    }

    /// Whether half cell `at` is on the grid.
    #[must_use]
    pub fn holds(&self, (x, y): (i32, i32)) -> bool {
        (0..self.side.0).contains(&x) && (0..self.side.1).contains(&y)
    }

    /// The ground walkers stand on.
    #[must_use]
    pub fn ground(&self) -> Option<&HeightMap> {
        self.ground.as_deref()
    }

    /// The ground's heights, as placements left them.
    #[must_use]
    pub fn heights(&self) -> Option<&dyn Heights> {
        self.ground.as_deref().map(|g| g as &dyn Heights)
    }

    /// The ground's height at `(x, y)`, 16.16 cells, as
    /// [`movement::ground`] reads it.
    #[must_use]
    pub fn height_at(&self, x: Fixed, y: Fixed) -> Option<Fixed> {
        self.ground.as_deref().map(|g| movement::ground(g, x, y))
    }

    /// Each vertex's shade byte, row by row: the save's until a placement
    /// relights the ground, then baked from its heights
    /// ([`light::bake`]). `None` without ground.
    #[must_use]
    pub fn shades(&self) -> Option<Cow<'_, [u8]>> {
        match (&self.shade, self.ground.as_deref()) {
            (Some(saved), _) => Some(Cow::Borrowed(saved)),
            (None, Some(ground)) => Some(Cow::Owned(light::bake(ground))),
            (None, None) => None,
        }
    }

    /// Each cell's paint, as placements left it.
    #[must_use]
    pub fn painted(&self) -> Option<&Painted> {
        self.paint.as_deref()
    }

    /// The scatter on the collision grid: code and half cell.
    #[must_use]
    pub fn scatter(&self) -> &[(FourCc, (i32, i32))] {
        &self.scatter
    }

    /// Where each of [`Board::scatter`] stands, and how high.
    #[must_use]
    pub fn stands(&self) -> &[Stand] {
        &self.stands
    }

    /// Every scatter object and terrain feature with no agent, in the
    /// order made, less what placements cleared.
    #[must_use]
    pub fn strewn(&self) -> &[Strewn] {
        &self.strewn
    }

    /// The set and slot each piece record drew in the save the world was
    /// made from, by handle: what a building shows until it changes. None
    /// for a piece made since, or a world made with no save.
    #[must_use]
    pub const fn saved_looks(&self) -> &BTreeMap<u32, Drawn> {
        &self.looks
    }

    /// The start's objects whose slot plays frames.
    pub(super) fn scenery(&self) -> &[(FourCc, (i32, i32), OnGrid)] {
        &self.scenery
    }

    /// How many changes ground, paint, shade and scatter have taken so
    /// far: a drawing redraws when it moves.
    #[must_use]
    pub const fn relaid(&self) -> u32 {
        self.relaid
    }

    /// The collision grid as last brought up to date; `None` before it is
    /// built.
    pub(super) const fn grid(&self) -> Option<&Grid> {
        self.collision.grid.as_ref()
    }

    /// The collision grid built whole: the scatter, measured by `bodies`,
    /// and each of `pieces`' entries.
    pub(super) fn grid_whole(&mut self, pieces: Vec<(u32, Stood)>, bodies: &Bodies) {
        let mut grid = Grid::new(self.side.0);

        for s in self
            .scattered(bodies)
            .chain(pieces.iter().map(|(_, s)| s.clone()))
        {
            grid.stand(s.who, s.cells);
        }

        self.collision = Collision {
            grid: Some(grid),
            stood: pieces
                .into_iter()
                .map(|(id, s)| (id, Arc::new(s)))
                .collect(),
        };
    }

    /// The scatter's entries on the grid, measured by `bodies`.
    pub(super) fn scattered<'a>(&'a self, bodies: &'a Bodies) -> impl Iterator<Item = Stood> + 'a {
        self.scatter
            .iter()
            .filter_map(|(code, at)| Stood::scatter(*code, *at, bodies))
    }

    /// Agent `id`'s entry on the grid now `stood`, `None` for none:
    /// unregistered where it was, registered where it is. Nothing before
    /// the grid is built.
    pub(super) fn regrid(&mut self, id: u32, stood: Option<Stood>) {
        let Collision { grid, stood: all } = &mut self.collision;

        if let Some(grid) = grid
            && all.get(&id).map(|s| &**s) != stood.as_ref()
        {
            if let Some(old) = all.remove(&id) {
                grid.leave(&old.who, old.cells.iter().copied());
            }

            if let Some(new) = stood {
                grid.stand(new.who, new.cells.iter().copied());
                all.insert(id, Arc::new(new));
            }
        }
    }

    /// Agent `id`'s entry as last registered.
    pub(super) fn stood(&self, id: u32) -> Option<&Stood> {
        self.collision.stood.get(&id).map(|s| &**s)
    }

    /// Every agent with an entry registered, in no order.
    pub(super) fn stood_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.collision.stood.keys().copied()
    }

    /// The scatter objects `sight` names a sight for, in order, with
    /// where each stands: measured once per [`Board::relaid`], since a
    /// change to the scatter or its stands moves it.
    pub(super) fn exposed(&mut self, sight: impl Fn(FourCc) -> Option<super::Sight>) -> &[Sampled] {
        if self.exposed.at != Some(self.relaid) {
            self.exposed = Exposed {
                at: Some(self.relaid),
                list: self.measured(sight),
            };
        }

        &self.exposed.list
    }

    /// [`Board::exposed`] measured anew.
    pub(super) fn measured(&self, sight: impl Fn(FourCc) -> Option<super::Sight>) -> Vec<Sampled> {
        self.scatter
            .iter()
            .zip(&self.stands)
            .filter_map(|((code, spot), stand)| {
                sight(*code).map(|s| ((*code, *spot), stand.fixed, s))
            })
            .collect()
    }

    /// [`Board::exposed`]'s list dropped, to be measured anew: the
    /// descriptors changed.
    pub(super) fn forget_exposed(&mut self) {
        self.exposed = Exposed::default();
    }

    /// The collision grid dropped, to be built whole again: what stands
    /// on it is measured anew.
    pub(super) fn ungrid(&mut self) {
        self.collision = Collision::default();
    }
}

/// Boards tests make by hand.
#[cfg(test)]
impl Board {
    /// A grid `side` half cells a side, nothing on it.
    pub(super) fn sized(side: (i32, i32)) -> Board {
        Board {
            side,
            ..Board::default()
        }
    }

    pub(super) fn with_ground(self, ground: HeightMap) -> Board {
        Board {
            ground: Some(Arc::new(ground)),
            ..self
        }
    }

    pub(super) fn with_paint(self, paint: Painted) -> Board {
        Board {
            paint: Some(Arc::new(paint)),
            ..self
        }
    }

    pub(super) fn with_shade(self, shade: Vec<u8>) -> Board {
        Board {
            shade: Some(shade),
            ..self
        }
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Board {
        type Model = m::Board;

        fn save(&self) -> m::Board {
            let Board {
                side,
                ground,
                paint,
                shade,
                scatter,
                stands,
                strewn,
                looks,
                scenery,
                relaid,
                clearing,
                collision: _,
                exposed: _,
            } = self;

            m::Board {
                side: *side,
                ground: ground.save(),
                paint: paint.save(),
                shade: shade.clone(),
                scatter: scatter.save(),
                stands: stands.save(),
                strewn: strewn.save(),
                looks: looks.save(),
                scenery: scenery.save(),
                relaid: *relaid,
                clearing: clearing.save(),
            }
        }

        fn load(model: m::Board) -> Result<Board, Refused> {
            Ok(Board {
                side: model.side,
                ground: Option::load(model.ground)?,
                paint: Option::load(model.paint)?,
                shade: model.shade,
                scatter: Vec::load(model.scatter)?,
                stands: Vec::load(model.stands)?,
                strewn: Vec::load(model.strewn)?,
                looks: BTreeMap::load(model.looks)?,
                scenery: Vec::load(model.scenery)?,
                relaid: model.relaid,
                clearing: Vec::load(model.clearing)?,
                collision: Collision::default(),
                exposed: Exposed::default(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sampled scatter follows each change to it, and is measured
    /// anew once forgotten: a pond laid after the first measure is there
    /// at the next.
    #[test]
    fn the_sampled_scatter_follows_the_scatter() {
        let pond = FourCc(*b"TPo1");
        let tree = FourCc(*b"TTr1");
        let sight = |code: FourCc| (code == pond).then_some(super::super::Sight::Everyone);
        let mut board = Board::sized((64, 64));

        let at = |v: i32| (Fixed::from_bits(v), Fixed::from_bits(v));

        board.lay(pond, (4, 4), Stand::on(None, at(128)));
        board.lay(tree, (8, 8), Stand::on(None, at(256)));
        assert_eq!(board.exposed(sight).len(), 1);

        board.lay(pond, (12, 12), Stand::on(None, at(384)));
        let measured = board.measured(sight);

        assert_eq!(board.exposed(sight), measured);
        assert_eq!(measured.len(), 2);

        board.forget_exposed();
        assert_eq!(board.exposed(|_| None).len(), 0);
    }
}
