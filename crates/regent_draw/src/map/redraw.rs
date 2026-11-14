//! The map's ground redrawn as the view moves or the picture changes:
//! its patches, and the inset's.

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::ecs::component::Mutable;
use bevy::prelude::*;

use regent_core::content::terrain::Tileset;
use regent_core::content::terrain::grid::Terrain;
use regent_core::data::settings::RELIEF;
#[cfg(test)]
use regent_core::view::Colour;
use regent_core::view::Sheet;
#[cfg(test)]
use regent_core::view::Sprite as Frame;
use regent_core::view::map::cut::Cut;
use regent_core::view::raster::lit;

use super::cel::{Paint, Store};
use super::depth::GROUND_DEPTH;
use super::project::{Composed, Projection, visible_in, wanted_in};
use super::reel::Reel;
use super::things::{self, populate};
use super::{Maps, floor, patch};
use crate::data::Corpus;
use crate::plain::Plain;
use crate::smooth::{self, Sampling};
use crate::stage::View;
use crate::terrain::{CELL, Drawn, TileArt};
use crate::{frame, material, texture, wind};

/// The composed ground: its patches (`patch`).
#[derive(Component)]
pub struct GroundSheet;

/// Keep the ground up as the view moves (`patch`); compose the map and
/// put it up on change.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies, declared rather than fetched"
)]
pub fn redraw<P: Paint + Resource + Component<Mutability = Mutable>>(
    mut commands: Commands,
    mut maps: ResMut<Maps>,
    corpus: Res<Corpus>,
    ground: Res<TileArt>,
    mut view: ResMut<View>,
    mut shown: ResMut<crate::terrain::MapShown>,
    reel: Res<Reel>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut colours: ResMut<Assets<Plain>>,
    mut bodies: ResMut<Assets<material::Body>>,
    mut bands: ResMut<Assets<material::Band>>,
    existing: Query<Entity, With<Drawn>>,
    mut painter: ResMut<P>,
    trees: Res<wind::Trees>,
    (sampling, mut sheets): (Option<Res<Sampling>>, Option<ResMut<Assets<smooth::Sheet>>>),
) {
    let projection = maps.projection();

    // A new map or toggle rebuilds everything; camera movement only
    // the ground's patches. The scatter goes up once its sprite pages
    // are in, asked meanwhile: at once where they are.
    let whole = shown.mapped != Some(maps.revision);

    if whole {
        for entity in &existing {
            commands.entity(entity).despawn();
        }

        maps.patches = patch::Patches::new();
        maps.composed = None;
        shown.mapped = Some(maps.revision);
        shown.scattered = None;
    }

    // Patches unseen by change detection: what watches `Maps` (the
    // shadows' rows, `lie::rows`) reads the objects, and a pan would
    // wake it most frames.
    let kept = if maps.terrain().is_some() {
        patch::keep(
            &mut commands,
            maps.bypass_change_detection(),
            &ground,
            (&view, sampling.map_or(Sampling::Nearest, |s| *s)),
            (
                &mut images,
                &mut meshes,
                &mut colours,
                sheets.as_deref_mut(),
            ),
        )
    } else {
        patch::Kept::Same
    };
    let scatter = shown.scattered != Some(maps.revision);

    if !whole && !scatter && kept == patch::Kept::Same {
        return;
    }

    let Some(terrain) = maps.terrain() else {
        "no save game to draw".clone_into(&mut shown.status);
        return;
    };

    let patches = match maps.composed {
        Some(want) => format!(
            "ground {} patches at {} per cell",
            maps.patches.len(),
            want.resolution
        ),
        None => String::new(),
    };

    if !whole && !scatter {
        shown.status = format!("{}  {patches}", maps.summary());
        return;
    }

    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    let mut store = Store {
        images: &mut images,
        meshes: &mut meshes,
        colours: &mut colours,
        bodies: &mut bodies,
        bands: &mut bands,
        quad,
    };

    if whole {
        floor::lay(&mut commands, (&terrain, &ground), (&maps, &mut store));

        // The whole map, for fitting.
        view.content = projection.extent();
    }

    let scene = (&*maps, &*trees);
    let ready = things::ready(
        scene,
        &corpus,
        (&mut store, &mut *painter as &mut dyn Paint),
    );

    if ready {
        let drawn = (&mut store, &mut *painter as &mut dyn Paint);
        let up = populate(&mut commands, scene, &corpus, drawn, &reel, projection);

        maps.placed = up.placed;
        maps.reach = up.reach;
        shown.scattered = Some(maps.revision);
        shown.status = format!("{}  {patches}", maps.summary());
    } else {
        "scatter: sprite pages on their way".clone_into(&mut shown.status);
    }
}

/// The render layer of the inset's ground: its camera's alone.
pub const INSET_LAYER: usize = 4;

/// Under the main ground sheet: where both are composed, the main one
/// draws; the inset shows only past it.
const INSET_DEPTH: f32 = GROUND_DEPTH - 10.0;

/// Frames a replaced inset sheet or ground patch stays under its
/// successor: a new one draws two frames after it is spawned (a shot
/// after a view jump, `--look-from`), and the ground showed missing
/// those frames (**Inferred**: Bevy orders no 2D material's preparation
/// after its images', and an entity waits a frame more for its
/// material).
pub(super) const RETIRE: u32 = 3;

/// A second view's ground, for a camera away from the main view: a
/// game's tracking window. The front end writes `seen`, the world rect
/// it shows; [`redraw_inset`] composes the cells around it, as
/// [`redraw`] does the main view's, on [`INSET_LAYER`].
///
/// ```text
///  main sheet  layer 0, GROUND_DEPTH   both cameras
///  inset sheet INSET_LAYER, deeper     the inset camera; shows where
///                                      the main sheet does not reach
/// ```
#[derive(Resource, Default)]
pub struct Inset {
    pub seen: Option<Rect>,
    composed: Option<Composed>,
    /// The ground it was composed from: the source's generation and the
    /// picture's revision.
    from: Option<(Option<u32>, u64)>,
}

/// The inset's ground sheet.
#[derive(Component)]
pub struct InsetSheet;

/// An inset sheet replaced, under its successor: the frames it has left
/// (`RETIRE`).
#[derive(Component)]
pub struct Retiring(u32);

/// Where a replaced inset sheet lies: just under its successor.
pub(super) const RETIRED_DEPTH: f32 = INSET_DEPTH - 1.0;

/// Compose the inset's ground when its rect leaves what is composed, or
/// the ground changed.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies, declared rather than fetched"
)]
pub fn redraw_inset(
    mut commands: Commands,
    maps: Res<Maps>,
    ground: Res<TileArt>,
    mut inset: ResMut<Inset>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut colours: ResMut<Assets<Plain>>,
    mut sheets: Query<(Entity, &mut Transform, Option<&mut Retiring>), With<InsetSheet>>,
) {
    // Replaced sheets let go once their successor has had time to show.
    for (entity, _, retiring) in &mut sheets {
        match retiring {
            Some(retiring) if retiring.0 <= 1 => commands.entity(entity).despawn(),
            Some(mut retiring) => retiring.0 -= 1,
            None => {}
        }
    }

    let projection = maps.projection();
    let side = maps.terrain().map_or(1, |t| t.side());
    let from = maps.current().map(|l| (l.source.generation, maps.revision));
    // By the composed ground's rise: measured only to compose.
    let held = match (inset.composed, inset.seen) {
        (Some(have), Some(seen)) => {
            have.holds(visible_in(Some(seen), projection, side, 0.0, have.rise))
                && inset.from == from
        }
        (_, None) => true,
        (None, Some(_)) => false,
    };

    if held {
        return;
    }

    let rise = maps.rise();

    // The sheet up stays under the new one a few frames, so the window
    // has ground while the new one's texture arrives.
    for (entity, mut transform, retiring) in &mut sheets {
        if retiring.is_none() {
            transform.translation.z = RETIRED_DEPTH;
            commands.entity(entity).insert(Retiring(RETIRE));
        }
    }

    let (Some(terrain), Some(tileset)) = (maps.terrain(), ground.tileset.as_ref()) else {
        return;
    };
    let region = wanted_in(inset.seen, projection, side, rise);
    let sheet = art(&terrain, tileset, &ground, region);

    inset.composed = Some(region);
    inset.from = from;

    let Some(image) = texture::sheet(&sheet) else {
        return;
    };
    let mesh = relief(&terrain, region, projection, RELIEF);
    let colour = Plain::ground(images.add(image));

    commands.spawn((
        InsetSheet,
        grounded(&mesh),
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(colours.add(colour)),
        Transform::from_xyz(0.0, 0.0, INSET_DEPTH),
        bevy::camera::visibility::RenderLayers::layer(INSET_LAYER),
    ));
}

/// The ground, composited from the tiles the save names.
///
/// Blend, case and variant all come from the file, so a wrong reading looks
/// wrong. `region` and resolution come from the camera. Each tile's RGBA
/// is made once
/// ([`TileArt::tile_rgba`]); a coarser pitch point-samples it ([`put`]).
pub(super) fn art(
    terrain: &Terrain<'_>,
    tileset: &Tileset<'static>,
    ground: &TileArt,
    region: Composed,
) -> Sheet {
    let span = region.span();
    let size = region.resolution as usize;
    let mut sheet = Sheet::blank(span.x as usize * size, span.y as usize * size);

    for (dx, dy) in (0..span.y).flat_map(|dy| (0..span.x).map(move |dx| (dx, dy))) {
        let tile = terrain
            .cell(region.from.x + dx, region.from.y + dy)
            .and_then(|cell| tileset.picture(cell))
            .and_then(|source| {
                ground
                    .tile_rgba(&source)
                    .map(|(f, rgba)| (f.width, f.height, rgba))
            });

        if let Some(tile) = tile {
            put(
                &mut sheet,
                tile,
                (dx as usize * size, dy as usize * size),
                size,
            );
        }
    }

    lit::ground(
        &mut sheet,
        terrain,
        region.from.to_array(),
        region.to.to_array(),
        region.resolution,
    );

    sheet
}

/// A tile's RGBA, `width` by `height`, into `sheet` at `at`, `size`
/// texels a side: as it is at full pitch, clipped to the cell
/// (`atlas::tile`); coarser, point-sampled: averaging blurs blend
/// boundaries and corrupts control indices.
fn put(
    sheet: &mut Sheet,
    (width, height, rgba): (u16, u16, &[u8]),
    at: (usize, usize),
    size: usize,
) {
    const CHANNELS: usize = 4;

    let (width, height) = (usize::from(width), usize::from(height));
    let stride = sheet.width * CHANNELS;

    if size == usize::from(CELL.0) {
        let (cols, rows) = (width.min(size), height.min(size));

        for row in 0..rows {
            let from = row * width * CHANNELS;
            let to = (at.1 + row) * stride + at.0 * CHANNELS;

            sheet.rgba[to..to + cols * CHANNELS]
                .copy_from_slice(&rgba[from..from + cols * CHANNELS]);
        }
    } else {
        for row in 0..size {
            let y = row * height / size;

            for col in 0..size {
                let from = (y * width + col * width / size) * CHANNELS;
                let to = (at.1 + row) * stride + (at.0 + col) * CHANNELS;

                sheet.rgba[to..to + CHANNELS].copy_from_slice(&rgba[from..from + CHANNELS]);
            }
        }
    }
}

/// The composed ground as a mesh, each vertex lifted by its height.
///
/// Shared corners cannot part, so a gap means a gap in the data. UVs are
/// linear, so zero relief reproduces the flat sprite. Each cell is cut
/// as the engine cuts it ([`Cut`]), keeping one diagonal as ridge or
/// valley. Up to 263,169 vertices (512-cell map).
#[allow(
    clippy::cast_precision_loss,
    reason = "a map is at most 512 cells; f32 is exact well past that"
)]
pub(super) fn relief(
    terrain: &Terrain<'_>,
    region: Composed,
    projection: Projection,
    scale: f32,
) -> bevy::render::mesh::Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};

    let span = region.span();
    let (across, down) = (span.x + 1, span.y + 1);
    let rise = scale * projection.pitch;

    let mut positions = Vec::with_capacity((across * down) as usize);
    let mut uvs = Vec::with_capacity((across * down) as usize);

    for dy in 0..down {
        for dx in 0..across {
            let (cx, cy) = (region.from.x + dx, region.from.y + dy);
            let height = terrain.vertex(cx, cy).map_or(0.0, |v| v.height);
            let at = projection.at(Vec2::new(cx as f32, cy as f32));

            // +y is up, so positive lifts high ground (the reading). Negative allowed
            // as a control. z: the metric point's depth (`crate::frame`).
            let y = at.y + rise * height;

            positions.push([at.x, y, frame::ground_z(y, rise * height)]);
            uvs.push([dx as f32 / span.x as f32, dy as f32 / span.y as f32]);
        }
    }

    let mut indices = Vec::with_capacity((span.x * span.y * 6) as usize);

    for dy in 0..span.y {
        for dx in 0..span.x {
            let tl = dy * across + dx;
            let (tr, bl, br) = (tl + 1, tl + across, tl + across + 1);
            let (cx, cy) = (region.from.x + dx, region.from.y + dy);

            indices.extend_from_slice(&Cut::of(cx, cy).indices([tl, tr, bl, br]));
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));

    mesh
}

/// A ground mesh's bounds for Bevy's culling: its screen footprint at its
/// rung, as 2D culled it. Its vertices' metric depths lie far off the
/// camera's depth range; it draws at its rung (`card.wesl`).
pub(crate) fn grounded(mesh: &bevy::render::mesh::Mesh) -> (Aabb, NoAutoAabb) {
    use bevy::render::mesh::{Mesh, VertexAttributeValues};

    let flat = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(positions)) => positions
            .iter()
            .map(|p| Vec2::new(p[0], p[1]))
            .fold(None, |seen: Option<Rect>, p| {
                Some(seen.map_or(Rect::from_corners(p, p), |r| r.union_point(p)))
            }),
        _ => None,
    }
    .unwrap_or_default();

    (
        Aabb::from_min_max(flat.min.extend(0.0), flat.max.extend(0.0)),
        NoAutoAabb,
    )
}

/// A tile at a coarser pitch, point-sampled: averaging blurs blend
/// boundaries and corrupts control indices. What [`put`] samples, as a
/// frame: the tests' reference.
#[cfg(test)]
pub(super) fn shrink(frame: &Frame, to: u32) -> Frame {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a resolution is at most 32"
    )]
    let side = to as u16;
    let take = |i: u16, of: u16| usize::from(i) * usize::from(of) / usize::from(side);

    let pick: Vec<usize> = (0..side)
        .flat_map(|y| (0..side).map(move |x| (x, y)))
        .map(|(x, y)| take(y, frame.height) * usize::from(frame.width) + take(x, frame.width))
        .collect();

    Frame {
        width: side,
        height: side,
        covered: pick.iter().map(|&i| frame.covered[i]).collect(),
        colour: match &frame.colour {
            Colour::Direct { words, packing } => Colour::Direct {
                words: pick.iter().map(|&i| words[i]).collect(),
                packing: *packing,
            },
            Colour::Indexed { indices, palette } => Colour::Indexed {
                indices: pick.iter().map(|&i| indices[i]).collect(),
                palette: palette.clone(),
            },
        },
        ..frame.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::fixtures::{art_of, looking, sloped, tiny};
    use crate::map::project::{Rise, visible};

    /// So a non-zero relief shows the setting, not a method change.
    #[test]
    #[allow(clippy::float_cmp, reason = "the same arithmetic both ways")]
    fn at_no_relief_the_ground_mesh_lies_exactly_where_the_flat_sheet_did() {
        let (vertices, cells) = tiny([3.0; 9]);
        let terrain = Terrain::typed(&vertices, &cells).unwrap();
        let projection = art_of(2);
        let region = Composed {
            from: UVec2::ZERO,
            to: UVec2::splat(2),
            resolution: 32,
            rise: Rise::default(),
        };

        let mesh = relief(&terrain, region, projection, 0.0);
        let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(at)) =
            mesh.attribute(bevy::render::mesh::Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("the mesh has no positions");
        };

        assert_eq!(at.len(), 9, "a two-cell map has three vertices a side");

        // The sheet corners the sprite path would use.
        let top_left = projection.at(Vec2::ZERO);
        let bottom_right = projection.at(Vec2::splat(2.0));

        assert_eq!([at[0][0], at[0][1]], [top_left.x, top_left.y]);
        assert_eq!([at[8][0], at[8][1]], [bottom_right.x, bottom_right.y]);
    }

    #[test]
    fn relief_lifts_each_vertex_by_its_own_height_alone() {
        let mut heights = [0.0; 9];
        heights[4] = 2.0;

        let (vertices, cells) = tiny(heights);
        let terrain = Terrain::typed(&vertices, &cells).unwrap();
        let projection = art_of(2);
        let region = Composed {
            from: UVec2::ZERO,
            to: UVec2::splat(2),
            resolution: 32,
            rise: Rise::default(),
        };

        let scale = 0.25;
        let mesh = relief(&terrain, region, projection, scale);
        let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(at)) =
            mesh.attribute(bevy::render::mesh::Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("the mesh has no positions");
        };

        let flat = projection.at(Vec2::ONE);

        // Up the screen: high ground drawn high.
        assert!((at[4][1] - (flat.y + scale * projection.pitch * 2.0)).abs() < 1e-3);
        assert!(at[4][1] > flat.y);

        // Neighbours stay put: per-vertex lift, tilted quads.
        for i in [0, 1, 2, 3, 5, 6, 7, 8] {
            let x = i % 3;
            let y = i / 3;

            #[allow(clippy::cast_precision_loss, reason = "three by three")]
            let want = projection.at(Vec2::new(x as f32, y as f32));

            assert!((at[i][1] - want.y).abs() < 1e-3, "vertex {i} moved");
        }
    }

    /// Only a saddle separates the cuts: the kept diagonal is the ridge. Read
    /// from emitted triangles on two cells of differing parity.
    #[test]
    fn a_ground_cell_is_cut_on_the_diagonal_its_parity_picks() {
        // High on one diagonal, low on the other: every cell a saddle.
        let (vertices, cells) = tiny([1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0]);
        let terrain = Terrain::typed(&vertices, &cells).unwrap();
        let region = Composed {
            from: UVec2::ZERO,
            to: UVec2::splat(2),
            resolution: 32,
            rise: Rise::default(),
        };

        let mesh = relief(&terrain, region, art_of(2), 1.0);
        let Some(bevy::render::mesh::Indices::U32(indices)) = mesh.indices() else {
            panic!("the mesh has no indices");
        };

        // Cell (0, 0) even, (1, 0) odd: first six indices on the main diagonal,
        // next six on the anti-diagonal.
        let cut = |tris: &[u32]| -> Vec<u32> {
            let (first, second) = (&tris[..3], &tris[3..]);
            let mut shared: Vec<u32> = first
                .iter()
                .filter(|v| second.contains(v))
                .copied()
                .collect();
            shared.sort_unstable();

            shared
        };

        // On a 3-wide vertex grid, cell (0, 0) is 0, 1, 3, 4 and (1, 0) is
        // 1, 2, 4, 5: 0-4 is the first's main diagonal, 2-4 the second's
        // anti-diagonal.
        assert_eq!(cut(&indices[..6]), [0, 4], "the even cell lost its saddle");
        assert_eq!(
            cut(&indices[6..12]),
            [2, 4],
            "the odd cell kept the wrong one"
        );

        assert_eq!(
            (Cut::of(0, 0), Cut::of(1, 0)),
            (Cut::Main, Cut::Anti),
            "the parity rule moved"
        );
    }

    /// Control indices must never be averaged.
    #[test]
    fn shrinking_a_frame_keeps_a_control_index_whole() {
        use regent_core::data::imag::Blit;
        use regent_core::data::payload::{Palette, Rgb};
        use regent_core::view::{Ink, PaletteSource};

        let frame = Frame {
            width: 2,
            height: 2,
            version: 3,
            anchor: (0, 0),
            covered: vec![true; 4],
            colour: Colour::Indexed {
                indices: vec![247, 1, 1, 1],
                palette: Box::new(Palette {
                    colours: [Rgb { r: 9, g: 9, b: 9 }; 256],
                }),
            },
            source: PaletteSource::Embedded,
            mode: regent_core::data::tile::Mode::Band,
            level: 0,
        };

        let small = shrink(&frame, 1);

        assert_eq!((small.width, small.height), (1, 1));
        assert!(
            matches!(small.ink(0, Blit::World), Ink::Shade(_)),
            "the band did not survive the shrink",
        );
    }

    /// The lowest point of a ground mesh, world units.
    fn lowest(mesh: &bevy::render::mesh::Mesh) -> f32 {
        match mesh.attribute(bevy::render::mesh::Mesh::ATTRIBUTE_POSITION) {
            Some(bevy::render::mesh::VertexAttributeValues::Float32x3(at)) => {
                at.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min)
            }
            _ => panic!("the mesh has no positions"),
        }
    }

    /// High ground lifts the rows past the screen's bottom into view. The
    /// least the ground may hold before a pan recomposes it must reach
    /// them, else the backdrop shows under its edge.
    #[test]
    fn the_ground_reaches_the_screens_bottom_over_high_ground() {
        let side = 128;
        let maps = sloped(side, |_, _| 8.0);
        let (terrain, projection) = (maps.terrain().expect("a terrain"), maps.projection());
        let view = looking(1.0, Vec2::ZERO);
        let bottom = view.seen().expect("a picture region").min.y;

        let rise = maps.rise();
        let (from, to) = visible(&view, projection, side, 0.0, rise);
        let held = Composed {
            from,
            to,
            resolution: 1,
            rise,
        };
        let mesh = relief(&terrain, held, projection, RELIEF);

        assert!(
            lowest(&mesh) <= bottom,
            "the ground ends at {} over a screen bottom of {bottom}",
            lowest(&mesh)
        );
    }

    /// Low ground, the other way: rows past the top drop into view.
    #[test]
    fn the_ground_reaches_the_screens_top_over_low_ground() {
        let side = 128;
        let maps = sloped(side, |_, _| -8.0);
        let (terrain, projection) = (maps.terrain().expect("a terrain"), maps.projection());
        let view = looking(1.0, Vec2::ZERO);
        let top = view.seen().expect("a picture region").max.y;

        let rise = maps.rise();
        let (from, to) = visible(&view, projection, side, 0.0, rise);
        let held = Composed {
            from,
            to,
            resolution: 1,
            rise,
        };
        let mesh = relief(&terrain, held, projection, RELIEF);
        let highest = match mesh.attribute(bevy::render::mesh::Mesh::ATTRIBUTE_POSITION) {
            Some(bevy::render::mesh::VertexAttributeValues::Float32x3(at)) => {
                at.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max)
            }
            _ => panic!("the mesh has no positions"),
        };

        assert!(
            highest >= top,
            "the ground ends at {highest} under a screen top of {top}"
        );
    }
}
