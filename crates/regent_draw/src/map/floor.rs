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
//!
//! The ground over it draws first, in the opaque phase
//! ([`Alpha::MaskFirst`]), so the floor is shaded only where no ground
//! covers it.

use bevy::image::ImageSampler;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use regent_core::content::terrain::grid::Terrain;
use regent_core::data::settings::RELIEF;

use super::Maps;
use super::cel::Store;
use super::project::{Composed, Rise};
use super::redraw::{RETIRED_DEPTH, art, grounded, relief};
use crate::plain::{Alpha, Plain};
use crate::terrain::{CELL, Drawn, TileArt};
use crate::texture;

/// The floor's entity.
#[derive(Component)]
pub struct Floor;

/// Texels the floor's sheet spans a side at most: 16 MB of texture.
const FLOOR_MAX: u32 = 2048;

/// Cells a side of a piece of the floor's mesh ([`pieces`]).
const PIECE: u32 = 64;

/// Indices a cell takes in [`relief`]'s mesh: two triangles.
const CELL_INDICES: usize = 6;

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
        // In the mask phase, after the patches': under them it fails the
        // early depth test.
        let colour = store.colours.add(Plain {
            alpha: Alpha::Mask,
            ..Plain::ground(store.images.add(image))
        });

        for piece in pieces(&mesh, UVec2::splat(side)) {
            commands.spawn((
                Drawn,
                Floor,
                grounded(&piece),
                Mesh3d(store.meshes.add(piece)),
                MeshMaterial3d(colour.clone()),
                Transform::from_xyz(0.0, 0.0, FLOOR_DEPTH),
            ));
        }
    }
}

/// `whole`, the floor's mesh over `span` cells ([`relief`]), cut into
/// pieces of [`PIECE`] cells a side, each culled on its own: a view
/// shades the vertices of those it meets, not the map's 263,169. A
/// piece's vertices and UVs are `whole`'s, so its pixels are too.
///
/// ```text
///  whole  (span + 1)² vertices, 6 indices a cell, row by row
///  piece  its cells' vertices, renumbered; their indices in order
/// ```
fn pieces(whole: &Mesh, span: UVec2) -> Vec<Mesh> {
    let (
        Some(VertexAttributeValues::Float32x3(positions)),
        Some(VertexAttributeValues::Float32x2(uvs)),
        Some(Indices::U32(indices)),
    ) = (
        whole.attribute(Mesh::ATTRIBUTE_POSITION),
        whole.attribute(Mesh::ATTRIBUTE_UV_0),
        whole.indices(),
    )
    else {
        return vec![whole.clone()];
    };

    let across = span.x + 1;
    let starts = |n: u32| (0..n).step_by(PIECE as usize);

    starts(span.y)
        .flat_map(|y| starts(span.x).map(move |x| UVec2::new(x, y)))
        .map(|from| {
            let to = (from + UVec2::splat(PIECE)).min(span);
            let wide = to.x - from.x + 1;
            let corners =
                (from.y..=to.y).flat_map(|y| (from.x..=to.x).map(move |x| y * across + x));
            let local = |v: u32| (v / across - from.y) * wide + (v % across - from.x);
            let cells = (from.y..to.y).flat_map(|y| (from.x..to.x).map(move |x| y * span.x + x));
            let cut: Vec<u32> = cells
                .flat_map(|cell| &indices[cell as usize * CELL_INDICES..][..CELL_INDICES])
                .map(|v| local(*v))
                .collect();

            let mut piece = Mesh::new(PrimitiveTopology::TriangleList, whole.asset_usage);

            piece.insert_attribute(
                Mesh::ATTRIBUTE_POSITION,
                corners
                    .clone()
                    .map(|v| positions[v as usize])
                    .collect::<Vec<_>>(),
            );
            piece.insert_attribute(
                Mesh::ATTRIBUTE_UV_0,
                corners.map(|v| uvs[v as usize]).collect::<Vec<_>>(),
            );
            piece.insert_indices(Indices::U32(cut));
            piece
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid mesh laid out as [`relief`]'s, `span` cells: each vertex's
    /// position its own, each cell cut on one diagonal.
    #[allow(clippy::cast_precision_loss, reason = "a small grid")]
    fn grid(span: UVec2) -> Mesh {
        let across = span.x + 1;
        let corners = (0..=span.y).flat_map(|y| (0..=span.x).map(move |x| (x, y)));
        let positions: Vec<[f32; 3]> = corners
            .clone()
            .map(|(x, y)| [x as f32, y as f32 * 2.0, 0.5])
            .collect();
        let uvs: Vec<[f32; 2]> = corners
            .map(|(x, y)| [x as f32 / span.x as f32, y as f32 / span.y as f32])
            .collect();
        let indices = (0..span.y)
            .flat_map(|y| (0..span.x).map(move |x| y * across + x))
            .flat_map(|tl| {
                [
                    tl,
                    tl + 1,
                    tl + across,
                    tl + 1,
                    tl + across + 1,
                    tl + across,
                ]
            })
            .collect();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }

    /// Each triangle's corners, position and UV, as bits.
    fn triangles(mesh: &Mesh) -> Vec<[([u32; 3], [u32; 2]); 3]> {
        let (
            Some(VertexAttributeValues::Float32x3(positions)),
            Some(VertexAttributeValues::Float32x2(uvs)),
            Some(Indices::U32(indices)),
        ) = (
            mesh.attribute(Mesh::ATTRIBUTE_POSITION),
            mesh.attribute(Mesh::ATTRIBUTE_UV_0),
            mesh.indices(),
        )
        else {
            panic!("a grid mesh");
        };
        let corner = |v: u32| {
            (
                positions[v as usize].map(f32::to_bits),
                uvs[v as usize].map(f32::to_bits),
            )
        };

        indices
            .chunks_exact(3)
            .map(|t| [corner(t[0]), corner(t[1]), corner(t[2])])
            .collect()
    }

    /// The pieces hold the whole floor's triangles, each corner's
    /// position and UV the same bits: their pixels are the whole's.
    #[test]
    fn the_floors_pieces_draw_its_triangles_exactly() {
        let span = UVec2::new(PIECE * 2 + 5, PIECE + 3);
        let whole = grid(span);
        let pieces = pieces(&whole, span);
        let mut cut: Vec<_> = pieces.iter().flat_map(triangles).collect();
        let mut kept = triangles(&whole);

        cut.sort_unstable();
        kept.sort_unstable();

        assert_eq!(pieces.len(), 3 * 2);
        assert_eq!(cut, kept);
    }
}
