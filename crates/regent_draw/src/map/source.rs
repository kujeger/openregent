//! What a map is drawn from: its ground and its objects, plain data.
//!
//! ```text
//!  arrays, records ──> Source::of_arrays ┤
//!  a live World ──> Source::of_world ────┴──> Source ──> Maps: ground,
//!   (its Board: heights, shade, paint,                   sprites, picks
//!    pictures, scatter; its explored
//!    flags; no pieces)
//! ```
//!
//! A world drawn draws its pieces itself: its source holds the ground
//! and the scatter alone.

use bevy::math::Vec2;
use regent_core::content::terrain::grid::{Cell, Terrain, Vertex};

use regent_core::base::fixed::{self, Fixed};
use regent_core::data::fourcc::FourCc;
use regent_core::sim::movement::{DIRECTIONS, action};
use regent_core::sim::world::{World, scatter::Strewn};

use super::SCATTER;

/// A map's ground and objects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapSource {
    /// The map; `None` without one.
    ground: Option<Ground>,
    /// Each object chunk's records, in [`CHUNKS`] order: [`SCATTER`]'s
    /// are the scatter.
    pub objects: [Vec<Item>; 4],
    /// The world's ground generation this shows
    /// ([`Board::relaid`](regent_core::sim::world::board::Board::relaid));
    /// `None` if not taken from one: a world's next relay redraws it.
    pub generation: Option<u32>,
}

/// A map's vertices and cells, read ([`Terrain::typed`]).
#[derive(Clone, Debug, PartialEq)]
struct Ground {
    vertices: Vec<Vertex>,
    cells: Vec<Cell>,
}

/// One object record: what drawing and picking read of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: FourCc,
    pub handle: u32,
    /// `None`: no owner.
    pub owner: Option<u32>,
    /// Inside another: its place is stale.
    pub held: bool,
    /// 16.16 cells.
    pub fixed: (Fixed, Fixed),
    /// 16.16 cells.
    pub height: Fixed,
    /// Its half cell.
    pub standing: (i32, i32),
    /// The packed set id it draws.
    pub set: u32,
    /// Slot, or a mobile's direction, and frame.
    pub drawing: (u16, u16),
}

impl Item {
    /// A world's scatter object or terrain feature.
    #[must_use]
    pub const fn of_strewn(strewn: &Strewn) -> Item {
        Item {
            id: strewn.code,
            handle: strewn.handle,
            owner: strewn.owner,
            held: false,
            fixed: strewn.stand.fixed,
            height: strewn.stand.height,
            standing: strewn.standing,
            set: strewn.drawn.set,
            drawing: strewn.drawn.drawing,
        }
    }

    /// Where it stands, in cells.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a coordinate is at most 512 cells; f32 is exact well past that"
    )]
    #[must_use]
    pub fn cell(&self) -> Vec2 {
        let at = |v: Fixed| fixed::to_f64(v) as f32;

        Vec2::new(at(self.fixed.0), at(self.fixed.1))
    }

    /// Which slot of a facing set of `slots` its direction lands on.
    #[must_use]
    pub fn facing(&self, slots: u16) -> u16 {
        let turn = i32::from(self.drawing.0).min(DIRECTIONS - 1);
        let slot = action::slot(turn, usize::from(slots));

        u16::try_from(slot).unwrap_or(0)
    }
}

impl MapSource {
    /// A map from its arrays and records alone: no save, no world.
    #[must_use]
    pub const fn of_arrays(
        vertices: Vec<Vertex>,
        cells: Vec<Cell>,
        objects: [Vec<Item>; 4],
    ) -> MapSource {
        MapSource {
            ground: Some(Ground { vertices, cells }),
            objects,
            generation: None,
        }
    }

    /// The map, over its arrays.
    #[must_use]
    pub fn terrain(&self) -> Option<Terrain<'_>> {
        let ground = self.ground.as_ref()?;

        Terrain::typed(&ground.vertices, &ground.cells).ok()
    }

    /// `world`'s map at its board's generation: heights, shade
    /// ([`Board::shades`](regent_core::sim::world::board::Board::shades)) and
    /// explored flags; each cell's paint and picture, `dress`'s where the
    /// board knows none; its scatter and terrain features. No pieces.
    /// `None` without a map.
    #[must_use]
    pub fn of_world(world: &World, dress: &dyn Fn(Cell, (u32, u32)) -> Cell) -> Option<MapSource> {
        let board = world.board();
        let (heights, painted, shades) = (board.heights()?, board.painted()?, board.shades()?);
        let flags = world.explored().flags();
        let across = heights.across();
        let side = across.saturating_sub(1);

        let vertices: Vec<Vertex> = shades
            .iter()
            .zip(flags)
            .enumerate()
            .map(|(i, (shade, flags))| {
                let i = u32::try_from(i).unwrap_or(u32::MAX);
                let height = heights
                    .height(i % across, i / across)
                    .unwrap_or(Fixed::ZERO);

                Vertex {
                    height: fixed::to_f32(height),
                    shade: *shade,
                    flags,
                }
            })
            .collect();
        let cells: Vec<Cell> = painted
            .cells()
            .iter()
            .zip(painted.pictures())
            .enumerate()
            .map(|(i, (cell, picture))| {
                let i = u32::try_from(i).unwrap_or(u32::MAX);
                let bare = Cell {
                    pair: cell.pair,
                    case: cell.case,
                    variant: 0,
                    rotation: 0,
                };

                match picture {
                    Some(p) => Cell {
                        variant: p.variant,
                        rotation: p.rotation,
                        ..bare
                    },
                    None => dress(bare, (i % side, i / side)),
                }
            })
            .collect();
        let mut objects: [Vec<Item>; 4] = Default::default();

        objects[SCATTER] = board.strewn().iter().map(Item::of_strewn).collect();

        Some(MapSource {
            ground: Some(Ground { vertices, cells }),
            objects,
            generation: Some(board.relaid()),
        })
    }
}
