//! The map tests' shared fixtures.

use bevy::prelude::*;
use regent_core::content::terrain::grid::{Cell, Vertex};

use regent_core::base::fixed::Fixed;
use regent_core::data::fourcc::FourCc;
use regent_core::sim::units::{PER_HALF, from_pos32};

use super::load::Loaded;
use super::project::Projection;
use super::{Item, MapSource, Maps, Placed, SCATTER};
use crate::stage::View;
use crate::terrain::CELL;

/// A `Maps` holding nothing but a set of placed things.
pub(super) fn holding(placed: Vec<Placed>) -> Maps {
    let mut maps = Maps::with(Vec::new());
    maps.placed = placed;
    maps
}

pub(super) fn at(chunk: usize, index: usize, x: f32, y: f32) -> Placed {
    Placed {
        chunk,
        index,
        cell: Vec2::new(x, y),
        height: 0.0,
    }
}

/// A two-by-two map with vertex heights `heights`, in reading order over
/// the 3x3 grid.
pub(super) fn tiny(heights: [f32; 9]) -> (Vec<Vertex>, Vec<Cell>) {
    (heights.map(vertex).to_vec(), vec![cell(0); 4])
}

/// A cell of class pair `pair`, case 0, drawing variant 0 unturned.
const fn cell(pair: u8) -> Cell {
    Cell {
        pair,
        case: 0,
        variant: 0,
        rotation: 0,
    }
}

/// A vertex `height` up, lit 237, explored.
const fn vertex(height: f32) -> Vertex {
    Vertex {
        height,
        shade: 237,
        flags: 0x80,
    }
}

/// Maps holding one map: `side` cells a side, vertex heights from `f`.
pub(super) fn sloped(side: u32, f: impl Fn(u32, u32) -> f32) -> Maps {
    let across = side + 1;
    let vertices = (0..across * across)
        .map(|i| vertex(f(i % across, i / across)))
        .collect();
    let cells = vec![cell(0); (side * side) as usize];
    let source = MapSource::of_arrays(vertices, cells, Default::default());

    let mut maps = Maps::with(Vec::new());
    maps.loaded = vec![Loaded::drawn("sloped".to_owned(), source)];

    maps
}

/// A flat two-by-two grass map holding `scatter`.
pub(super) fn flat_with(scatter: Vec<Item>) -> MapSource {
    let grass = cell(0x11);
    let mut objects: [Vec<Item>; 4] = Default::default();

    objects[SCATTER] = scatter;

    MapSource::of_arrays(vec![vertex(0.0); 9], vec![grass; 4], objects)
}

/// A tree owned by player 7 at `at`, 32nds of a cell, drawing slot 0 of
/// set 0.
pub(super) fn tree(handle: u32, at: (i32, i32)) -> Item {
    Item {
        id: FourCc(*b"RG07"),
        handle,
        owner: Some(7),
        held: false,
        fixed: (from_pos32(at.0), from_pos32(at.1)),
        height: Fixed::ZERO,
        standing: (at.0 / PER_HALF, at.1 / PER_HALF),
        set: 0,
        drawing: (0, 0),
    }
}

/// A slope with saddles and a hill, cut both ways; row 0 level, as a
/// map's rim is: the pick sees nothing drawn above the flat map.
pub(super) fn hillside(x: u32, y: u32) -> f32 {
    let saddle = if (x + y).is_multiple_of(2) { 0.6 } else { 0.0 };
    let hill = if (x, y) == (4, 4) { 0.4 } else { 0.0 };

    #[allow(clippy::cast_precision_loss, reason = "a few cells")]
    let slope = 0.1 * x as f32 + 0.3 * y as f32;

    if y == 0 { 0.0 } else { slope + saddle + hill }
}

/// An art-layer projection for a map `side` cells across.
pub(super) fn art_of(side: u32) -> Projection {
    #[allow(clippy::cast_precision_loss, reason = "at most 512")]
    Projection {
        side: side as f32,
        pitch: f32::from(CELL.0),
    }
}

/// An 800 by 600 picture region at some magnification.
pub(super) fn looking(zoom: f32, pan: Vec2) -> View {
    View {
        rect: Some(Rect {
            min: Vec2::new(320.0, 40.0),
            max: Vec2::new(1120.0, 640.0),
        }),
        zoom,
        pan,
        ..default()
    }
}
