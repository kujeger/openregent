//! Where a soft shadow lies (`[enhancements] shadows = "soft"`): a shadow
//! pixel falls on what stands on the ground it covers, not only on what
//! its caster is drawn over.
//!
//! The engine sorts a shadow pixel with its caster, at the caster's row.
//! The Palace's artists painted most of its shadow
//! in front of its anchor, so a unit standing in it is drawn after the
//! Palace and never darkened. Here each darkening pixel takes, as its
//! depth, the nearer of its caster's and that of the ground at the foot
//! of its run: the lowest shadow pixel below it, in its column, before
//! a hole. The foot, not the pixel's own ground: a unit is upright, and
//! the shadow over its head lies on ground behind it.
//!
//! ```text
//!   caster's row y ──┐                    a pixel's ground row:
//!        ┌────────┐  │  anchor             top + row / ROW
//!        │ palace │──┘                    its run's foot:
//!        └────────┘                         the lowest shadow pixel below,
//!     ░░░░░░░░░░░░░░    ← a unit here       no hole between
//!     ░░░░░░░░░░░░░░      takes it
//!                     ← one here does not
//!
//!  depth = max(caster's, Depth::at(foot's ground row))
//! ```
//!
//! The ground is taken as flat at the caster's height: the foot's row is
//! the caster's own plus its screen distance below the caster's ground
//! point over [`ROW`] (**Inferred**: the art paints shadows for flat
//! ground). The caster's depth is kept as a floor, so a shadow never
//! darkens less than the engine's does: shadows cast behind their caster
//! (trees, the Wizard Guild) look as before, and a set's later layer
//! still darkens its earlier ones, as 28,618 pixels of HD's art have it
//! (every unit's sets, frame 0, slot 0: `tests/lie.rs`). The cost: a layer's
//! shadow cast in front now darkens a later layer of the same set where
//! they overlap, 5,190 pixels in all, a few in each set.
//!
//! [`Depth`] is not linear in the row, so the shader looks it up in a
//! table of rows ([`ROWS`], [`PER_CELL`] samples a cell), and each band's
//! [`MeshTag`] holds the ground row at its quad's top, from the
//! [`Footing`] of its caster ([`tag`]).

use bevy::asset::{RenderAssetUsages, uuid_handle};
use bevy::image::ImageSampler;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use regent_core::data::settings::SQUASH;
use regent_core::view::raster::shade::{CONTROL_FIRST, TRANSPARENT};

use crate::cull::Parked;
use crate::map::Maps;
use crate::map::depth::{Depth, FURTHEST};
use crate::material::Band;
use crate::soft::{self, Ramps};
use crate::terrain::CELL;

/// Art pixels a cell down the screen: a tile's width, pitched
/// ([`SQUASH`]). Also the world units a cell on the art layer.
pub const ROW: f32 = CELL.1 as f32 * SQUASH;

/// Samples of [`ROWS`] a cell: a sample every 1.6 art pixels.
pub const PER_CELL: u32 = 16;

/// Samples of [`ROWS`] a texture row; the table wraps.
pub const WIDTH: u32 = 1024;

/// The depth a thing standing on each row draws at ([`Depth::at`]),
/// [`PER_CELL`] samples a cell from row 0, [`WIDTH`] a texture row, the
/// tail past the map's last row its last value.
pub const ROWS: Handle<Image> = uuid_handle!("bac8fe49-e5b7-4f42-b9e3-ea6894b5f888");

/// Where a caster stands: its row in cells, and the world height its
/// ground point is drawn at. On the entity a band is, or its parent.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Footing {
    pub row: f32,
    pub ground: f32,
}

/// The ground row at the top of a quad whose top edge is drawn at world
/// height `top`, its caster standing at `footing`.
#[must_use]
pub fn top_row(footing: Footing, top: f32) -> f32 {
    footing.row + (footing.ground - top) / ROW
}

/// A row as a [`MeshTag`]: its bits, where 0 is no footing; a row of
/// exactly 0 is written as −0.
#[must_use]
pub fn encode(row: f32) -> MeshTag {
    let row = if row == 0.0 { -0.0 } else { row };

    MeshTag::new(row.to_bits())
}

/// Whether index `index` darkens under `ramps`: a band index whose
/// factor is under one. Glows do not lie on the ground.
#[must_use]
pub fn darkens(ramps: Ramps, index: u8) -> bool {
    let factors = soft::factors(ramps);

    (CONTROL_FIRST..TRANSPARENT).contains(&index)
        && factors[usize::from(index - CONTROL_FIRST)] < soft::ONE
}

/// The row of the foot of darkening pixel `at`'s run, in a tile of
/// `width` holding `indices` ([`TRANSPARENT`] a hole): the lowest
/// darkening pixel below it in its column before a hole, as `band.wesl`
/// finds it.
#[must_use]
pub fn foot(indices: &[u8], width: usize, at: (usize, usize), ramps: Ramps) -> usize {
    let (x, y) = at;

    indices
        .iter()
        .skip(x)
        .step_by(width)
        .enumerate()
        .skip(y + 1)
        .take_while(|(_, i)| **i != TRANSPARENT)
        .filter(|(_, i)| darkens(ramps, **i))
        .last()
        .map_or(y, |(row, _)| row)
}

/// The depth a darkening pixel tests at, its caster at depth `caster`,
/// its quad's top on ground row `top` and its run's foot on tile row
/// `foot`: `band.wesl`'s `lying`, there to the table's resolution.
#[must_use]
pub fn lying(depth: &Depth, caster: f32, top: f32, foot: usize) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "a tile's row")]
    let row = top + (foot as f32 + 0.5) / ROW;

    caster.max(depth.at(row))
}

/// [`ROWS`]' samples over a map `side` cells down.
#[must_use]
pub fn samples(depth: &Depth, side: f32) -> Vec<f32> {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a map is at most 512 cells: a few thousand samples"
    )]
    let count = (side * PER_CELL as f32) as usize + 1;
    let wide = WIDTH as usize;
    let padded = count.div_ceil(wide) * wide;

    #[allow(clippy::cast_precision_loss, reason = "a few thousand samples")]
    let at = |i: usize| depth.at(i.min(count - 1) as f32 / PER_CELL as f32);

    (0..padded).map(at).collect()
}

/// [`samples`] as [`ROWS`]' texture.
fn table(samples: &[f32]) -> Image {
    let data = samples.iter().flat_map(|z| z.to_le_bytes()).collect();

    #[allow(
        clippy::cast_possible_truncation,
        reason = "a few thousand samples: a few texture rows"
    )]
    let rows = (samples.len() / WIDTH as usize).max(1) as u32;

    let mut image = Image::new(
        Extent3d {
            width: WIDTH,
            height: rows,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::R32Float,
        RenderAssetUsages::RENDER_WORLD,
    );

    image.sampler = ImageSampler::nearest();
    image
}

/// [`ROWS`] before a map: every row at the far end, so a shadow falls
/// as its caster's.
pub fn start(mut images: ResMut<Assets<Image>>) {
    let far = vec![FURTHEST; WIDTH as usize];

    // A Uuid handle takes any asset.
    let _ = images.insert(&ROWS, table(&far));
}

/// [`ROWS`] anew when the placed rows or the map's size change.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn rows(
    maps: Option<Res<Maps>>,
    mut images: ResMut<Assets<Image>>,
    mut drawn: Local<Vec<f32>>,
) {
    let Some(maps) = maps.filter(Res::is_changed) else {
        return;
    };

    #[allow(clippy::cast_precision_loss, reason = "a map is at most 512 cells")]
    let side = maps.terrain().map_or(0.0, |t| t.side() as f32);
    let samples = samples(&Depth::of(&maps), side);

    if *drawn != samples {
        let _ = images.insert(&ROWS, table(&samples));
        *drawn = samples;
    }
}

/// What [`tag`] reads of a band.
type Banded = (
    Entity,
    &'static Transform,
    Option<&'static Footing>,
    Option<&'static ChildOf>,
    Option<&'static MeshTag>,
);

/// Each band's [`MeshTag`]: the ground row at its quad's top, from its
/// own [`Footing`] or its parent's. A band with neither keeps none, and
/// falls as its caster's. A parked band
/// ([`crate::cull`]) keeps its tag until it comes out.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn tag(
    mut commands: Commands,
    bands: Query<Banded, (With<MeshMaterial3d<Band>>, Without<Parked>)>,
    parents: Query<(&Transform, &Footing)>,
) {
    for (entity, transform, own, parent, had) in &bands {
        let stood = match (own, parent) {
            (Some(footing), _) => Some((*footing, *transform)),
            (None, Some(parent)) => parents
                .get(parent.parent())
                .ok()
                .map(|(above, footing)| (*footing, above.mul_transform(*transform))),
            (None, None) => None,
        };

        if let Some((footing, put)) = stood {
            let top = put.translation.y + put.scale.y / 2.0;
            let tag = encode(top_row(footing, top));

            if had != Some(&tag) {
                commands.entity(entity).insert(tag);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A quad whose top is a cell's height above its caster's ground
    /// point starts a row behind the caster.
    #[test]
    #[allow(clippy::float_cmp, reason = "exact in binary")]
    fn a_quads_top_row_is_its_height_over_the_ground() {
        let footing = Footing {
            row: 10.0,
            ground: -100.0,
        };

        assert_eq!(top_row(footing, -100.0), 10.0);
        assert!((top_row(footing, -100.0 + ROW) - 9.0).abs() < 1e-5);
    }

    /// The table runs past the map to whole texture rows, its tail the
    /// last row's.
    #[test]
    #[allow(clippy::float_cmp, reason = "copied, not worked out")]
    fn the_table_fills_whole_rows() {
        let depth = Depth::new(vec![1.0, 2.0]);
        let samples = samples(&depth, 4.0);
        let last = 4 * PER_CELL as usize;

        assert_eq!(samples.len(), WIDTH as usize);
        assert_eq!(samples[last], depth.at(4.0));
        assert_eq!(samples[WIDTH as usize - 1], samples[last]);
        assert!(samples.windows(2).all(|w| w[0] <= w[1]));
    }

    /// A run goes on through colours and glows, and ends at a hole: its
    /// foot is the last darkening pixel before it.
    #[test]
    fn a_run_ends_at_a_hole() {
        let column = [247, 10, 248, 251, TRANSPARENT, 249];

        assert_eq!(foot(&column, 1, (0, 0), Ramps::Band), 2);
        assert_eq!(foot(&column, 1, (0, 5), Ramps::Band), 5);
        assert!(!darkens(Ramps::Band, 251));
    }

    /// A row of 0 is a footing, not none.
    #[test]
    #[allow(clippy::float_cmp, reason = "the same bits back")]
    fn row_zero_is_tagged() {
        assert_ne!(encode(0.0), MeshTag::new(0));
        assert_eq!(f32::from_bits(encode(2.5).value), 2.5);
    }
}
