//! The art layer's whole map at a low resolution, under every patch: the
//! ground newly in view (a minimap jump, a pan past the patches' margin,
//! a zoom out's edges) shows it until its patches draw, a frame or two
//! after they are spawned (`map::RETIRE`), not the backdrop.
//!
//! ```text
//!  patches   their rows, GROUND_DEPTH up      the view's, full resolution
//!  leaving   UNDER those                      a zoom's or recompose's
//!  inset     INSET_DEPTH, its camera alone    the tracking window's
//!  floor     FLOOR_DEPTH, both cameras        the whole map, low
//!  backdrop
//! ```
//!
//! Composed with the map (a new map, layer or toggle), not at a
//! recompose: it shows only those frames, so a ground changed since
//! shows stale there.

use bevy::image::ImageSampler;
use bevy::prelude::*;
use regent_core::content::terrain::grid::Terrain;
use regent_core::data::settings::RELIEF;

use super::Maps;
use super::cel::Store;
use super::project::{Composed, Rise};
use super::redraw::{RETIRED_DEPTH, art, grounded, relief};
use crate::plain::Plain;
use crate::terrain::{CELL, Drawn, TileArt};
use crate::texture;

/// The floor's entity.
#[derive(Component)]
pub struct Floor;

/// Texels the floor's sheet spans a side at most: 16 MB of texture.
const FLOOR_MAX: u32 = 2048;

/// Under the inset's sheets, the replaced ones too: the tracking
/// window's camera sees the floor as well.
const FLOOR_DEPTH: f32 = RETIRED_DEPTH - 1.0;

/// Lay the floor of `terrain` on the art layer, under its patches: the
/// whole map at as many texels a cell as fit [`FLOOR_MAX`], up to a
/// tile's; read linear, as it is coarse.
pub(super) fn lay(
    commands: &mut Commands,
    (terrain, ground): (&Terrain<'_>, &TileArt),
    (maps, store): (&Maps, &mut Store<'_>),
) {
    let side = terrain.side();
    let region = Composed {
        from: UVec2::ZERO,
        to: UVec2::splat(side),
        resolution: (FLOOR_MAX / side.max(1)).clamp(1, u32::from(CELL.0)),
        rise: Rise::default(),
    };
    let sheet = (ground.tileset.as_ref()).map(|tileset| art(terrain, tileset, ground, region));

    if let Some(mut image) = sheet.as_ref().and_then(texture::sheet) {
        image.sampler = ImageSampler::linear();

        let mesh = relief(terrain, region, maps.projection(), RELIEF);
        let colour = Plain::ground(store.images.add(image));

        commands.spawn((
            Drawn,
            Floor,
            grounded(&mesh),
            Mesh3d(store.meshes.add(mesh)),
            MeshMaterial3d(store.colours.add(colour)),
            Transform::from_xyz(0.0, 0.0, FLOOR_DEPTH),
        ));
    }
}
