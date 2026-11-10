//! The ground a building takes as it is placed: `BuildingRec`
//! after its place search. **Binary** unless marked.
//!
//! ```text
//!  flatten               phase 0, 1   level the pad    sim::placement::flatten
//!  move the unit                      stand on it
//!  GSUnit                             two clearing passes  sim::clearing
//!    pass 0, the pad:   ruins, agentless buildings, AVk1-3
//!    pass 1, the ring:  scatter, the same buildings, spent chests
//!  GE_Level (GR00)                    Dirt over the pad, Info bit 8 only
//!    per covered half cell, j outer, i inner,
//!    cell (x / 2, y / 2) truncated, painted    then cascade;
//!    a painted cell's picture               a paint RNG draw, unknown
//!  Map      (Inferred)                the shade relit   sim::light
//! ```
//!
//! One path for chart, `SpawnUnit` and the player's buildings; nothing
//! restores the ground when the building goes.

use crate::base::digest::Set;
use crate::content::terrain::grid;

use super::{
    AGENT_ID, World,
    bodies::{Body, Class},
};
use crate::base::fixed::Fixed;
use crate::data::fourcc::FourCc;
use crate::sim::clearing::{Kind, Occupant, Pass, Time};
use crate::sim::paint::{self, Cell, Picture};
use crate::sim::placement::{Edge, Heights, Pad};
use crate::sim::{movement, space};

/// What `BuildingRec` passes `GE_Level`: the only `GR00`
/// immediate in the executable.
pub const PAD_CLASS: FourCc = FourCc(*b"GR00");

/// Whether a placement paints its pad: `GE_Level` tests `PRIM`
/// `Info` bit 8, `ModifyTerrainTextureOnPlacement`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lays {
    Dirt,
    #[default]
    Nothing,
}

/// Where a scatter object stands and how high, 16.16 cells: unit x,
/// The height is the ground's as the object is made, or
/// as a placement reseats it; nothing else moves it, and the engine draws
/// it there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stand {
    pub fixed: (Fixed, Fixed),
    pub height: Fixed,
}

impl Stand {
    /// Put at `fixed` on `ground`, as cell pos of pos32 puts a
    /// unit: 0 without ground.
    #[must_use]
    pub fn on(ground: Option<&dyn Heights>, fixed: (Fixed, Fixed)) -> Stand {
        Stand {
            fixed,
            height: ground.map_or(Fixed::ZERO, |g| movement::ground(g, fixed.0, fixed.1)),
        }
    }

    /// Half cell `at` in 16.16 cells.
    #[must_use]
    pub const fn of_half((x, y): (i32, i32)) -> (Fixed, Fixed) {
        (
            Fixed::from_bits(x << HALF_TO_FIXED),
            Fixed::from_bits(y << HALF_TO_FIXED),
        )
    }
}

/// Half cells to 16.16 cells: to 32nds, then to 16.16.
const HALF_TO_FIXED: u32 = space::HALF_SHIFT + crate::sim::units::POS32_SHIFT;

/// Each cell's paint, row by row: a map's `MCEL` pairs and cases, and
/// the picture each draws.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Painted {
    side: i32,
    cells: Vec<Cell>,
    /// In step with `cells`. `None` where generated or painted since the
    /// save: map paint cell draws a painted cell's variant from
    /// paint rng, which the clock seeds, so no seed gives it.
    pictures: Vec<Option<Picture>>,
}

impl Painted {
    /// A save's cells and pictures.
    #[must_use]
    pub fn of(terrain: &grid::Terrain<'_>) -> Painted {
        let cells = terrain
            .cells()
            .map(|c| Cell {
                pair: c.pair,
                case: c.case,
            })
            .collect();
        let pictures = terrain
            .cells()
            .map(|c| {
                Some(Picture {
                    variant: c.variant,
                    rotation: c.rotation,
                })
            })
            .collect();

        Painted {
            side: terrain.side().cast_signed(),
            cells,
            pictures,
        }
    }

    /// Generated cells, `side` a row: no picture known.
    #[must_use]
    pub fn new(side: i32, cells: Vec<Cell>) -> Painted {
        let pictures = vec![None; cells.len()];

        Painted {
            side,
            cells,
            pictures,
        }
    }

    /// Every cell, row by row.
    #[must_use]
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Every cell's picture, row by row: `None` where not known.
    #[must_use]
    pub fn pictures(&self) -> &[Option<Picture>] {
        &self.pictures
    }

    fn index(&self, (x, y): (i32, i32)) -> Option<usize> {
        ((0..self.side).contains(&x) && (0..self.side).contains(&y))
            .then(|| (y * self.side + x).cast_unsigned() as usize)
    }
}

impl paint::Paintable for Painted {
    fn cell(&self, at: (i32, i32)) -> Option<Cell> {
        self.index(at).and_then(|i| self.cells.get(i).copied())
    }

    /// Called only where pair or case moved ([`paint::point`]): the
    /// picture is the engine's draw, not known.
    fn set(&mut self, at: (i32, i32), cell: Cell) {
        if let Some(i) = self.index(at).filter(|i| *i < self.cells.len()) {
            self.cells[i] = cell;

            if let Some(picture) = self.pictures.get_mut(i) {
                *picture = None;
            }
        }
    }
}

impl World {
    /// `BuildingRec`'s tail for a building standing on half cell
    /// `at`: levelled, cleared, paved.
    ///
    /// A building with no polygon levels its rect half open, one half cell
    /// short of the Dirt it paints each way (building flatten's rect
    /// path, building flatten). **Game**: The Barren Waste's chests and banner
    /// each stand on 2 by 2 levelled vertices in its 0-day save.
    pub(super) fn take_ground(&mut self, body: &Body, at: (i32, i32), time: Time) {
        let flat = body.mask.as_ref().map(|m| match body.edge {
            Edge::Mask => m.clone(),
            Edge::Rect => m.half_open(),
        });

        if let Some(mask) = flat.as_ref() {
            self.board.level(at, mask, self.descriptors.bodies());
        }

        self.clear(body, at, time);
        self.board.pave(body, at);
        self.board.relight();
    }

    /// `GSUnit`'s two passes about half cell `at`. A deletion is
    /// deferred in the engine: in play the scatter
    /// goes at the next drain ([`super::board::Board::clear_later`]); while
    /// generating, and a piece, at once (**Inferred**: nothing reads the
    /// grid between).
    pub(super) fn clear(&mut self, body: &Body, at: (i32, i32), time: Time) {
        let about = |pad: Option<&Pad>| -> Set<(i32, i32)> {
            pad.map(|p| p.halves().map(|(x, y)| (at.0 + x, at.1 + y)).collect())
                .unwrap_or_default()
        };
        let passes = [
            (Pass::Under, about(body.mask.as_ref())),
            (Pass::Around, about(body.ring.as_ref())),
        ];
        let deletes = |occupant: &Occupant, cells: &[(i32, i32)], spot: (i32, i32)| {
            passes.iter().any(|(pass, mask)| {
                occupant.cleared(*pass, time)
                    && cells
                        .iter()
                        .any(|(x, y)| mask.contains(&(spot.0 + x, spot.1 + y)))
            })
        };
        let bodies = self.descriptors.bodies();

        // Agentless: a terrain feature, or a save's ruin.
        let stays = |code: &FourCc, spot: (i32, i32)| {
            bodies.get(code).is_none_or(|b| {
                let kind = match b.class {
                    Class::Building => Kind::Building,
                    _ => Kind::Scatter,
                };
                let occupant = Occupant {
                    kind,
                    code: *code,
                    ruined: false,
                    agent: false,
                    hit_points: 0,
                };

                !deletes(&occupant, &b.cells, spot)
            })
        };

        match time {
            Time::Playing => self.board.clear_later(stays),
            Time::Generating => self.board.clear(stays, self.descriptors.bodies()),
        }

        let mut gone: Vec<(u32, bool)> = self
            .pieces
            .iter()
            .filter_map(|(id, p)| {
                let place = self.known(*id).filter(|pl| pl.holder == 0)?;
                let b = bodies.get(&p.code)?;
                let get = |k: i32| p.attributes.get(&k).copied().unwrap_or_default();
                let occupant = Occupant {
                    kind: match b.class {
                        Class::Building => Kind::Building,
                        _ => Kind::Other,
                    },
                    code: p.code,
                    ruined: place.ruined(),
                    agent: get(AGENT_ID.packed()) != 0,
                    hit_points: get(super::combat::hit_points()),
                };

                deletes(&occupant, &b.cells, place.standing).then_some((*id, occupant.agent))
            })
            .collect();

        // By agent, not the pieces' hash order. **Inferred**: the
        // engine's walk is unit list 0's; two cleared at once show it.
        gone.sort_unstable();

        // One with an agent, an emptied chest, goes as `DeleteGamePiece`'s
        // do: `UnitDelete` after the orders.
        // **Game**: The Barren Waste's Rogues' Guild, placed by opened
        // chest 18, then its `UnitDelete`.
        for (id, agent) in gone {
            if agent {
                self.detach(id, None);

                self.dispatches.delete(id);
            } else {
                self.sweep(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::board::Board;
    use super::super::new_map::OnGrid;
    use super::super::places::Motion;
    use super::super::ruin::tests::{body, standing};
    use super::super::scatter::{Drawn, Strewn};
    use super::super::walking::HeightMap;
    use super::*;
    use crate::gpl::ops::Value;
    use crate::sim::light;
    use crate::sim::paint::Paintable as _;
    use crate::sim::world::kind;

    /// A full square of half cells about the unit's own.
    fn square(low: i32, size: u32) -> Pad {
        Pad {
            origin: (low, low),
            size: (size, size),
            covered: vec![true; (size * size) as usize],
        }
    }

    /// A building's body: a 2 × 2 half-cell pad, a 6 × 6 ring.
    fn hall(lays: Lays) -> Body {
        let mut b = body(Class::Building, 0x30, square(0, 2).halves().collect());

        b.mask = Some(square(0, 2));
        b.ring = Some(square(-2, 6));
        b.lays = lays;
        b
    }

    /// `side` cells of grass, class 1.
    fn lawn(side: i32) -> Painted {
        Painted::new(
            side,
            vec![Cell::uniform(1); (side * side).cast_unsigned() as usize],
        )
    }

    /// A world of `side` cells of grass ([`lawn`]).
    fn grass(side: i32) -> World {
        World {
            board: Board::sized((side * 2, side * 2)).with_paint(lawn(side)),
            ..World::default()
        }
    }

    /// 0.9 height units a cell: under the slope limit.
    const RISE: Fixed = Fixed::from_bits(58_982);

    /// Ground `0` vertices a side, rising [`RISE`] a cell east.
    struct Slope(u32);

    impl Heights for Slope {
        fn across(&self) -> u32 {
            self.0
        }

        fn height(&self, x: u32, y: u32) -> Option<Fixed> {
            (x < self.0 && y < self.0).then(|| x.cast_signed() * RISE)
        }
    }

    /// Building flatten phase 1 reseats what stands in the pad's
    /// cell rect grown by one: a tree there takes the new ground's
    /// height. One further down the slope, whose ground the slope clamp
    /// moved too, keeps its own.
    #[test]
    fn a_placement_reseats_the_scatter_beside_its_pad() {
        let tree = FourCc(*b"RG07");
        let mut world = World {
            board: Board::sized((128, 128))
                .with_paint(lawn(64))
                .with_ground(HeightMap::of(&Slope(65))),
            ..World::default()
        };
        let mut inn = hall(Lays::Nothing);

        inn.mask = Some(square(0, 4));
        inn.ring = Some(square(0, 4));
        world
            .descriptors_mut()
            .add_body(tree, body(Class::General, 0, vec![(0, 0)]));

        for half in [(58, 60), (48, 60)] {
            let stand = Stand::on(world.board.heights(), Stand::of_half(half));

            world
                .board
                .raise(strewn(tree, stand, OnGrid::On), world.descriptors.bodies());
        }

        // Off the grid: drawn, never reseated.
        let aside = Stand::on(world.board.heights(), Stand::of_half((60, 60)));

        world
            .board
            .raise(strewn(tree, aside, OnGrid::Off), world.descriptors.bodies());

        let before = world.board.stands().to_vec();

        world.take_ground(&inn, (60, 60), Time::Playing);

        let ground = |s: &Stand| world.board.height_at(s.fixed.0, s.fixed.1);
        let after = world.board.stands();
        let drawn: Vec<Stand> = world.board.strewn().iter().map(|s| s.stand).collect();

        assert_eq!(world.board.scatter().len(), 2);
        assert!(before.iter().all(|s| ground(s) != Some(s.height)));
        assert_eq!(Some(after[0].height), ground(&after[0]));
        assert_eq!(after[1], before[1]);
        assert_eq!(drawn, [after[0], after[1], aside]);
    }

    /// A drawn record of `code` at `stand`.
    fn strewn(code: FourCc, stand: Stand, grid: OnGrid) -> Strewn {
        Strewn {
            code,
            handle: 1,
            owner: None,
            stand,
            standing: (0, 0),
            drawn: Drawn::default(),
            grid,
        }
    }

    /// A placement's ring clears a tree on the grid, and its drawn
    /// record with it, at the next drain; one off the grid stays drawn.
    #[test]
    fn a_placement_clears_the_drawn_scatter_with_the_scatter() {
        let tree = FourCc(*b"RG07");
        let mut world = grass(32);
        let stand = Stand::on(None, Stand::of_half((21, 21)));

        world
            .descriptors_mut()
            .add_body(tree, body(Class::General, 0, vec![(0, 0)]));
        world
            .board
            .raise(strewn(tree, stand, OnGrid::On), world.descriptors.bodies());
        world
            .board
            .raise(strewn(tree, stand, OnGrid::Off), world.descriptors.bodies());
        world.take_ground(&hall(Lays::Nothing), (20, 20), Time::Playing);

        assert_eq!(world.board.scatter(), [(tree, (21, 21))]);

        world.take_deletes();

        assert_eq!(world.board.scatter(), []);
        assert_eq!(world.board.strewn(), [strewn(tree, stand, OnGrid::Off)]);
    }

    /// `GSUnit` clears a ruined building in pass 1: building over
    /// a ruin deletes it and its rubble order. A standing building with
    /// an agent stays; while charts generate, the ruin stays too.
    #[test]
    fn a_placement_deletes_the_ruin_its_ring_reaches() {
        let temple = *b"ABP1";
        let mut world = grass(32);
        let mut whole = standing(temple, 101, kind::Kind::BUILDING, (20, 23));

        world
            .descriptors_mut()
            .add_body(FourCc(temple), body(Class::Building, 0x30, vec![(0, 0)]));
        whole.attributes.insert(AGENT_ID.packed(), 13);
        world
            .pieces
            .insert(12, standing(temple, 100, kind::Kind::BUILDING, (23, 20)));
        world.pieces.insert(13, whole);
        world.units.set_motion(12, Motion::Still);
        world.units.set_motion(13, Motion::Still);
        world.collapse(12);
        assert!(world.rubble_end(12).is_some());

        let mut generating = world.clone();

        generating.take_ground(&hall(Lays::Dirt), (20, 20), Time::Generating);
        assert!(generating.pieces.contains(12));

        let relaid = world.board.relaid();

        world.take_ground(&hall(Lays::Dirt), (20, 20), Time::Playing);
        assert!(!world.pieces.contains(12));
        assert_eq!(world.rubble_end(12), None);
        assert!(world.pieces.contains(13));
        assert!(world.board.relaid() > relaid);
    }

    /// `GE_Level` paints `GR00` over the pad where `Info` bit 8
    /// says so: the cell under a 2 × 2 half-cell pad turns Dirt; a
    /// descriptor without the bit leaves the grass.
    #[test]
    fn a_placement_paves_its_pad_with_dirt() {
        let mut world = grass(32);
        let at = |w: &World| w.board.painted().and_then(|p| p.cell((10, 10)));

        world.take_ground(&hall(Lays::Nothing), (20, 20), Time::Playing);
        assert_eq!(at(&world), Some(Cell::uniform(1)));

        world.take_ground(&hall(Lays::Dirt), (20, 20), Time::Playing);
        assert_eq!(at(&world), Some(Cell::uniform(0)));
    }

    /// A saved cell keeps its picture until painted: the paved cell's,
    /// and those the cascade blended, are the engine's draw, unknown;
    /// one far off keeps the save's.
    #[test]
    fn a_painted_cell_loses_its_saved_picture() {
        let saved = Some(Picture {
            variant: 2,
            rotation: 1,
        });
        let mut paint = lawn(32);

        paint.pictures = vec![saved; paint.cells.len()];

        let mut world = World {
            board: Board::sized((64, 64)).with_paint(paint),
            ..World::default()
        };

        world.take_ground(&hall(Lays::Dirt), (20, 20), Time::Playing);

        let picture = |at: (usize, usize)| {
            world
                .board
                .painted()
                .and_then(|p| p.pictures().get(at.1 * 32 + at.0).copied())
                .flatten()
        };

        assert_eq!(picture((10, 10)), None);
        assert_eq!(picture((11, 10)), None);
        assert_eq!(picture((0, 0)), saved);
    }

    /// The save's shade holds until a placement relights the ground, then
    /// the heights' bake.
    #[test]
    fn a_placement_relights_the_saved_shade() {
        let mut world = World {
            board: Board::sized((4, 4))
                .with_paint(lawn(2))
                .with_ground(HeightMap::of(&Slope(3)))
                .with_shade(vec![7; 9]),
            ..World::default()
        };

        assert_eq!(world.board.shades().as_deref(), Some(&[7; 9][..]));

        // No pad: nothing levelled, the ground's own bake.
        world.take_ground(&body(Class::Building, 0, vec![]), (0, 0), Time::Playing);
        assert_eq!(
            world.board.shades().as_deref(),
            Some(light::bake(&Slope(3)).as_slice())
        );
    }

    /// Emptied chests in a placement's ring go in agent order, their
    /// `UnitDelete`s the last first, whatever order the pieces' hash
    /// map hands them in: each round a fresh world, a fresh hash.
    #[test]
    fn a_ring_deletes_its_chests_in_agent_order() {
        const ROUNDS: usize = 16;
        const CHEST: [u8; 4] = *b"BBt1";
        const CHESTS: [u32; 4] = [9, 3, 12, 6];

        for _ in 0..ROUNDS {
            let mut world = grass(64);

            world
                .descriptors_mut()
                .add_body(FourCc(CHEST), body(Class::General, 0, vec![(0, 0)]));

            for (id, x) in CHESTS.into_iter().zip(58..) {
                let mut chest = standing(CHEST, id, kind::Kind::BUILDING, (x, 60));

                chest.attributes.insert(AGENT_ID.packed(), 1);
                world.pieces.insert(id, chest);
                world.units.set_motion(id, Motion::Still);
            }

            world.clear(&hall(Lays::Nothing), (60, 60), Time::Playing);

            let deleted: Vec<Vec<Value>> = world
                .take_deletes()
                .into_iter()
                .map(|(_, args)| args)
                .collect();
            let expected: Vec<Vec<Value>> = [12, 9, 6, 3]
                .into_iter()
                .map(|id| vec![Value::Integer(id)])
                .collect();

            assert_eq!(deleted, expected);
        }
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Stand {
        type Model = m::Stand;

        fn save(&self) -> m::Stand {
            m::Stand {
                fixed: self.fixed.save(),
                height: self.height.save(),
            }
        }

        fn load(model: m::Stand) -> Result<Stand, Refused> {
            Ok(Stand {
                fixed: <(Fixed, Fixed)>::load(model.fixed)?,
                height: Fixed::load(model.height)?,
            })
        }
    }

    impl Saved for Painted {
        type Model = m::Painted;

        fn save(&self) -> m::Painted {
            m::Painted {
                side: self.side,
                cells: self.cells.iter().map(|c| (c.pair, c.case)).collect(),
                pictures: self
                    .pictures
                    .iter()
                    .map(|p| p.map(|p| (p.variant, p.rotation)))
                    .collect(),
            }
        }

        fn load(model: m::Painted) -> Result<Painted, Refused> {
            Ok(Painted {
                side: model.side,
                cells: model
                    .cells
                    .into_iter()
                    .map(|(pair, case)| Cell { pair, case })
                    .collect(),
                pictures: model
                    .pictures
                    .into_iter()
                    .map(|p| p.map(|(variant, rotation)| Picture { variant, rotation }))
                    .collect(),
            })
        }
    }
}
