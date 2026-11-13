//! The art layer's ground in patches: square pieces of what was one
//! composed sheet, each its own texture and mesh, kept up as the view
//! moves.
//!
//! The one sheet was composed again whole each time the view left it:
//! on a 512-cell map 70 to 170 ms on the main thread, every half second
//! of a pan, and at each zoom step. Patches compose only what comes into
//! reach.
//!
//! ```text
//!   ┌────┬────┬────┬────┬────┐  the wanted region (wanted): the screen
//!   │    │ ░░ │ ░░ │ ░░ │    │  and MARGIN, at its resolution, as the
//!   ├────┼────┼────┼────┼────┤  sheet was
//!   │ ░░ │ ▓▓ │ ▓▓ │ ░░ │    │  ▓ on screen and missing: composed now,
//!   ├────┼────┼────┼────┼────┤    in parallel
//!   │ ░░ │ ▓▓ │ ▓▓ │ ░░ │    │  ░ wanted and missing: AHEAD a frame,
//!   └────┴────┴────┴────┴────┘    nearest the screen first
//!   past KEEP of the screen's span: dropped
//! ```
//!
//! A patch is [`PATCH`] texels a side, aligned to the map: 16 cells at
//! full resolution, more as it coarsens. A zoom to another resolution
//! hides the patches up and shows that resolution's, kept from before
//! while in reach: a zoom back composes nothing. A recompose
//! ([`Maps::recompose`]) drops them all. Each cell composes alone, so a
//! patch's texels are the sheet's. Read sharp-bilinear
//! ([`Sampling::Sharp`]), a patch has an [`APRON`] of its neighbours'
//! texels, so the blend crosses its seams, and a [`smooth::Sheet`] for a
//! material; a change of sampling drops them all.
//!
//! A new patch draws frames after it is spawned (`RETIRE`): the patches
//! a zoom hides or a recompose drops stay drawn under the new ones until
//! then, or the ground flashes missing (a building placed).

use std::collections::BTreeMap;

use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::tasks::{ComputeTaskPool, TaskPool};
use regent_core::content::terrain::Tileset;
use regent_core::content::terrain::grid::Terrain;
use regent_core::data::settings::RELIEF;
use regent_core::view::Sheet;

use super::Maps;
use super::depth::GROUND_DEPTH;
use super::project::{Composed, MARGIN, Projection, visible, wanted};
use super::redraw::{GroundSheet, RETIRE, art, grounded, relief};
use crate::plain::{Plain, Shade};
use crate::smooth::{self, Sampling};
use crate::stage::View;
use crate::terrain::{Drawn, TileArt};
use crate::texture;

/// Texels a patch spans a side: about a megabyte of texture.
const PATCH: u32 = 512;

/// Texels of the neighbouring cells' art round a patch read
/// sharp-bilinear: the blend at its edge reads one. None for nearest,
/// which reads none: its texels and UVs stay the one sheet's.
const APRON: u32 = 1;

/// Bytes per texel: RGBA8.
const CHANNELS: usize = 4;

/// How far each patch row stands in front of the row above it, world
/// units: in the one sheet a later row drew over an earlier one where
/// relief folds the ground up over it. Rows only: a cell's triangles
/// stay in its column. At most 512 rows, so all stay well short of the
/// objects' band (`BEYOND`).
const STACK: f32 = 0.01;

/// How far past the screen, as a share of its span, a patch is kept:
/// twice what is composed ahead, so a pan back composes nothing again.
const KEEP: f32 = 2.0 * MARGIN;

/// How far under every row a leaving patch draws: past all rows' stack
/// (512 of [`STACK`]), so a new patch always covers it.
const UNDER: f32 = 6.0;

/// Patches composed ahead a frame, once all on screen are up: a pan
/// needs a few a second, and each costs the frame a few ms.
const AHEAD: usize = 2;

/// A patch's place on the grid: column and row.
type At = [u32; 2];

/// The patches up, by resolution and place: the wanted resolution's
/// shown, the others hidden until a zoom comes back to them.
#[derive(Debug, Default)]
pub(super) struct Patches {
    up: BTreeMap<u32, BTreeMap<At, Entity>>,
    /// The resolution shown.
    shown: Option<u32>,
    /// Patches taken down or hidden, still drawn under their successors
    /// a few frames ([`RETIRE`]).
    leaving: Vec<Leaving>,
    /// How those up are read: made for it, with or without an [`APRON`].
    sampling: Sampling,
}

/// A patch replaced, drawn under the new ones until their textures are
/// surely on the GPU, then let go.
#[derive(Debug)]
struct Leaving {
    entity: Entity,
    at: At,
    /// Drawn frames left.
    frames: u32,
    fate: Fate,
}

/// What becomes of a [`Leaving`] patch once its frames are up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    /// Despawned: a recompose's.
    Gone,
    /// Hidden at its own depth again, kept for a zoom back.
    Hidden,
}

impl Patches {
    pub(super) const fn new() -> Patches {
        Patches {
            up: BTreeMap::new(),
            shown: None,
            leaving: Vec::new(),
            sampling: Sampling::Nearest,
        }
    }

    /// How many are shown.
    pub(super) fn len(&self) -> usize {
        self.shown
            .and_then(|r| self.up.get(&r))
            .map_or(0, BTreeMap::len)
    }

    /// Every patch taken down: those drawn kept under the new ones a few
    /// frames, the hidden ones at once.
    fn clear(&mut self, commands: &mut Commands) {
        let shown = self.shown.and_then(|r| self.up.remove(&r));

        for leaving in &mut self.leaving {
            leaving.fate = Fate::Gone;
        }

        for (at, entity) in shown.into_iter().flatten() {
            self.leave(commands, entity, at, Fate::Gone);
        }

        let drawn: Vec<Entity> = self.leaving.iter().map(|l| l.entity).collect();

        for (_, entity) in std::mem::take(&mut self.up).into_values().flatten() {
            if !drawn.contains(&entity) {
                commands.entity(entity).despawn();
            }
        }

        self.shown = None;
    }

    /// Resolution `to` shown, the one before kept under it a few frames,
    /// then hidden.
    fn show(&mut self, commands: &mut Commands, to: u32) {
        let before = self.shown.and_then(|r| self.up.get(&r)).cloned();

        for (at, entity) in before.into_iter().flatten() {
            self.leave(commands, entity, at, Fate::Hidden);
        }

        // Shown again while still leaving: back at its own depth.
        let back = self.up.get(&to).cloned().unwrap_or_default();

        for (at, entity) in &back {
            if self.leaving.iter().any(|l| l.entity == *entity) {
                commands.entity(*entity).insert(placed(*at, Depth::Own));
            }

            commands.entity(*entity).insert(Visibility::Inherited);
        }

        self.leaving
            .retain(|l| !back.values().any(|e| *e == l.entity));
        self.shown = Some(to);
    }

    /// Patch `entity` at `at` drawn under every other a few frames, then
    /// `fate`'s.
    fn leave(&mut self, commands: &mut Commands, entity: Entity, at: At, fate: Fate) {
        commands.entity(entity).insert(placed(at, Depth::Under));
        self.leaving.push(Leaving {
            entity,
            at,
            frames: RETIRE,
            fate,
        });
    }

    /// A frame on: the leaving patches whose frames are up let go.
    fn tick(&mut self, commands: &mut Commands) {
        self.leaving.retain_mut(|leaving| {
            leaving.frames = leaving.frames.saturating_sub(1);

            let done = leaving.frames == 0;

            if done {
                let mut entity = commands.entity(leaving.entity);

                match leaving.fate {
                    Fate::Gone => entity.despawn(),
                    Fate::Hidden => {
                        entity.insert((Visibility::Hidden, placed(leaving.at, Depth::Own)));
                    }
                }
            }

            !done
        });
    }

    /// Patches of every resolution past `reach` (cells) taken down.
    fn drop_past(&mut self, commands: &mut Commands, reach: (UVec2, UVec2)) -> usize {
        let mut gone = 0;

        for (resolution, set) in &mut self.up {
            let cells = (PATCH / resolution).max(1);

            set.retain(|at, entity| {
                let kept = meets(*at, cells, reach);

                if !kept {
                    commands.entity(*entity).despawn();
                    gone += 1;
                }

                kept
            });
        }

        // A hidden-to-be patch dropped is no longer leaving.
        let up: Vec<Entity> = self
            .up
            .values()
            .flat_map(BTreeMap::values)
            .copied()
            .collect();

        self.leaving
            .retain(|l| l.fate == Fate::Gone || up.contains(&l.entity));
        gone
    }
}

/// Where a patch draws: at its row's depth, or under every row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Depth {
    Own,
    Under,
}

/// Patch `at`'s place: its row's depth ([`STACK`]), or [`UNDER`] that.
#[allow(clippy::cast_precision_loss, reason = "at most 512 rows")]
fn placed(at: At, depth: Depth) -> Transform {
    let row = GROUND_DEPTH + STACK * at[1] as f32;
    let z = match depth {
        Depth::Own => row,
        Depth::Under => row - UNDER,
    };

    Transform::from_xyz(0.0, 0.0, z)
}

/// What a frame's [`keep`] changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kept {
    Same,
    Changed,
}

/// The textures, meshes and materials a patch is made into; the sheets
/// only where [`smooth::Plugin`] is.
pub(super) type Stores<'a> = (
    &'a mut Assets<Image>,
    &'a mut Assets<Mesh>,
    &'a mut Assets<Plain>,
    Option<&'a mut Assets<smooth::Sheet>>,
);

/// The ground's patches kept up for `view`: those on screen made now,
/// the rest of the wanted region [`AHEAD`] a frame, those out of reach
/// dropped. A zoom to another resolution shows its patches, hiding the
/// others; a recompose, or another `sampling`, drops them all.
pub(super) fn keep(
    commands: &mut Commands,
    maps: &mut Maps,
    ground: &TileArt,
    (view, sampling): (&View, Sampling),
    stores: Stores<'_>,
) -> Kept {
    let mut patches = std::mem::take(&mut maps.patches);

    patches.tick(commands);

    if patches.sampling != sampling {
        maps.recompose();
        patches.sampling = sampling;
    }

    let (projection, side) = (maps.projection(), maps.terrain().map_or(1, |t| t.side()));

    // By the held rise, measured only to compose: a first compose or a
    // recompose measures it again, as the heights may have moved.
    let rise = if let Some(have) = maps.composed {
        have.rise
    } else {
        patches.clear(commands);
        maps.rise()
    };
    let want = wanted(view, projection, side, rise);
    let zoomed = patches.shown != Some(want.resolution);

    if zoomed {
        patches.show(commands, want.resolution);
    }

    let cells = (PATCH / want.resolution).max(1);
    let shown = visible(view, projection, side, 0.0, want.rise);
    let gone = patches.drop_past(commands, visible(view, projection, side, KEEP, want.rise));
    let up = patches.up.entry(want.resolution).or_default();

    // On screen and missing: all, now. Else a few of the wanted
    // region's, nearest the screen's middle first.
    let missing = |(from, to): (UVec2, UVec2)| -> Vec<At> {
        grid(from, to, cells)
            .filter(|at| !up.contains_key(at))
            .collect()
    };
    let mut todo = missing(shown);

    if todo.is_empty() {
        let middle = (shown.0 + shown.1) / (2 * cells);
        let away = |at: &At| {
            UVec2::from(*at)
                .as_ivec2()
                .distance_squared(middle.as_ivec2())
        };

        todo = missing((want.from, want.to));
        todo.sort_by_key(away);
        todo.truncate(AHEAD);
    }

    let made = match (maps.terrain(), ground.tileset.as_ref()) {
        (Some(terrain), Some(tileset)) => {
            let from = Source {
                terrain: &terrain,
                tileset,
                ground,
                projection,
            };

            ComputeTaskPool::get_or_init(TaskPool::default).scope(|scope| {
                for &at in &todo {
                    let from = &from;

                    scope.spawn(async move { from.patch(want, cells, at, sampling) });
                }
            })
        }
        _ => Vec::new(),
    };

    let (images, meshes, colours, mut sheets) = stores;

    for made in made {
        if let Some(mut image) = texture::sheet(&made.sheet) {
            let mut entity = commands.spawn((
                Drawn,
                GroundSheet,
                grounded(&made.mesh),
                Mesh3d(meshes.add(made.mesh)),
                placed(made.at, Depth::Own),
            ));

            // Sharp: one linear tap at a warped uv (`smooth::Sheet`).
            if let (Sampling::Sharp, Some(sheets)) = (sampling, sheets.as_deref_mut()) {
                image.sampler = ImageSampler::linear();
                entity.insert(MeshMaterial3d(sheets.add(smooth::Sheet {
                    picture: images.add(image),
                    shade: Shade::Bare,
                })));
            } else {
                let colour = Plain::ground(images.add(image));

                entity.insert(MeshMaterial3d(colours.add(colour)));
            }

            up.insert(made.at, entity.id());
        }
    }

    maps.patches = patches;
    maps.composed = Some(want);

    if todo.is_empty() && gone == 0 && !zoomed {
        Kept::Same
    } else {
        Kept::Changed
    }
}

/// The patches, `cells` a side, over cells `from` to `to` (exclusive).
fn grid(from: UVec2, to: UVec2, cells: u32) -> impl Iterator<Item = At> {
    let (low, high) = (from / cells, (to - UVec2::ONE) / cells);

    (low.y..=high.y).flat_map(move |y| (low.x..=high.x).map(move |x| [x, y]))
}

/// Whether patch `at`, `cells` a side, meets cells `from` to `to`.
fn meets(at: At, cells: u32, (from, to): (UVec2, UVec2)) -> bool {
    let start = UVec2::from(at) * cells;

    start.cmplt(to).all() && (start + UVec2::splat(cells)).cmpgt(from).all()
}

/// What a patch is composed from.
struct Source<'a> {
    terrain: &'a Terrain<'a>,
    tileset: &'a Tileset<'static>,
    ground: &'a TileArt,
    projection: Projection,
}

/// One patch composed: its texture and its mesh.
struct Made {
    at: At,
    sheet: Sheet,
    mesh: Mesh,
}

impl Source<'_> {
    /// Patch `at`, `cells` a side, at `want`'s resolution and rise: its
    /// cells composed, its mesh over the whole texture; read sharp, with
    /// an [`APRON`] round it ([`Source::aproned`]).
    fn patch(&self, want: Composed, cells: u32, at: At, sampling: Sampling) -> Made {
        let side = UVec2::splat(self.terrain.side());
        let start = UVec2::from(at) * cells;
        let region = Composed {
            from: start,
            to: (start + UVec2::splat(cells)).min(side),
            ..want
        };
        let mut mesh = relief(self.terrain, region, self.projection, RELIEF);

        let sheet = match sampling {
            Sampling::Nearest => {
                mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs(region.span()));
                art(self.terrain, self.tileset, self.ground, region)
            }
            Sampling::Sharp => {
                let (sheet, at) = self.aproned(region);

                mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, at);
                sheet
            }
        };

        Made { at, sheet, mesh }
    }

    /// `region` composed with a cell more each way where the map has one,
    /// cut to the region and [`APRON`]; and its mesh's UVs inside the
    /// apron.
    fn aproned(&self, region: Composed) -> (Sheet, Vec<[f32; 2]>) {
        let side = UVec2::splat(self.terrain.side());
        let wide = Composed {
            from: region.from.saturating_sub(UVec2::ONE),
            to: (region.to + UVec2::ONE).min(side),
            ..region
        };
        let sheet = art(self.terrain, self.tileset, self.ground, wide);

        // Texels of the wide sheet before and after the region: a cell's
        // or none; an apron's of them kept.
        let texels = region.resolution;
        let (lead, trail) = (
            (region.from - wide.from) * texels,
            (wide.to - region.to) * texels,
        );
        let (before, after) = (
            lead.min(UVec2::splat(APRON)),
            trail.min(UVec2::splat(APRON)),
        );
        let kept = region.span() * texels + before + after;

        (
            crop(&sheet, lead - before, kept),
            apron_uvs(region, before, kept),
        )
    }
}

/// `sheet`'s texels from `from`, `size` of them.
fn crop(sheet: &Sheet, from: UVec2, size: UVec2) -> Sheet {
    let (x, width) = (from.x as usize, size.x as usize);
    let mut rgba = Vec::with_capacity(width * size.y as usize * CHANNELS);

    for y in from.y as usize..(from.y + size.y) as usize {
        let start = (y * sheet.width + x) * CHANNELS;

        rgba.extend_from_slice(&sheet.rgba[start..start + width * CHANNELS]);
    }

    Sheet {
        width,
        height: size.y as usize,
        rgba,
    }
}

/// Each vertex of `region`'s mesh (row-major, as [`relief`] lays them)
/// on its texture, `size` texels with `before` of apron.
#[allow(
    clippy::cast_precision_loss,
    reason = "a patch is a few hundred texels a side"
)]
fn apron_uvs(region: Composed, before: UVec2, size: UVec2) -> Vec<[f32; 2]> {
    let span = region.span();
    let on = |cell: u32, apron: u32, of: u32| (apron + cell * region.resolution) as f32 / of as f32;

    (0..=span.y)
        .flat_map(|dy| (0..=span.x).map(move |dx| (dx, dy)))
        .map(|(dx, dy)| [on(dx, before.x, size.x), on(dy, before.y, size.y)])
        .collect()
}

/// Each vertex of a mesh `span` cells a side (row-major, as [`relief`]
/// lays them) on its texture: cell edges evenly from 0 to 1.
#[allow(clippy::cast_precision_loss, reason = "at most 512 cells a side")]
fn uvs(span: UVec2) -> Vec<[f32; 2]> {
    (0..=span.y)
        .flat_map(|dy| (0..=span.x).map(move |dx| (dx, dy)))
        .map(|(dx, dy)| [dx as f32 / span.x as f32, dy as f32 / span.y as f32])
        .collect()
}

#[cfg(test)]
mod tests {

    use super::super::project::Rise;

    use super::*;

    /// A patch's mesh spans its texture: first vertex at 0, last at 1.
    #[test]
    #[allow(clippy::float_cmp, reason = "exact quotients")]
    fn a_patch_mesh_spans_its_texture() {
        let at = uvs(UVec2::new(16, 8));

        assert_eq!(at.len(), 17 * 9);
        assert_eq!(at[1], [1.0 / 16.0, 0.0]);
        assert_eq!(at[at.len() - 1], [1.0, 1.0]);
    }

    /// Read sharp, a patch's mesh meets its texture inside the apron: its
    /// first vertex on the first texel past it, its last on the far apron.
    #[test]
    #[allow(clippy::float_cmp, reason = "exact quotients")]
    fn a_sharp_patch_mesh_maps_inside_its_apron() {
        let region = Composed {
            from: UVec2::new(16, 0),
            to: UVec2::new(32, 16),
            resolution: 32,
            rise: Rise::default(),
        };
        let at = apron_uvs(region, UVec2::new(1, 0), UVec2::new(514, 513));

        assert_eq!(at.len(), 17 * 17);
        assert_eq!(at[0], [1.0 / 514.0, 0.0]);
        assert_eq!(at[at.len() - 1], [513.0 / 514.0, 512.0 / 513.0]);
    }

    #[test]
    fn the_grid_covers_the_cells_and_meets_tells_its_patches() {
        let found: Vec<At> = grid(UVec2::new(15, 0), UVec2::new(17, 16), 16).collect();

        assert_eq!(found, vec![[0, 0], [1, 0]]);
        assert!(meets([1, 0], 16, (UVec2::new(31, 0), UVec2::new(40, 1))));
        assert!(!meets([1, 0], 16, (UVec2::new(32, 0), UVec2::new(40, 1))));
        assert!(!meets([1, 0], 16, (UVec2::ZERO, UVec2::new(16, 1))));
    }
}
