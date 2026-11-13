//! Each drawn body and band's row in one table on the GPU: its
//! [`MeshTag`] is the row's index, so every frame shares the unit quad and
//! instances batch by material.
//!
//! ```text
//!  entity: Framed (its frame's texels in its texture), Tag (a body's
//!          footing or slot, a band's ground row; `sun`, `lie`) and a
//!          tree's Lean (`wind`)
//!     ──write (Last): a Row the first time, rewritten when either changes──>
//!  Rows, on the CPU ──the texture rows touched this frame──> extract
//!     ──upload (render world): written into the texture in place──> TABLE
//!  TABLE  Rgba32Uint, WIDTH rows a texture row, a row two texels:
//!         x = tag   y = rectangle x | y << 16   z = width | height << 16
//!         then a tree's foot and bend, f32 bits (`wind::Gust`)
//!  body.wesl, band.wesl: the vertex stage reads its row by its MeshTag
//! ```
//!
//! A data texture, not a storage buffer: WebGL2 and GLES 3.0 read no
//! storage buffer in the vertex stage (wgpu's downlevel limits), and a
//! texture reads alike everywhere (**Inferred**: Intel UHD 600 on Vulkan
//! takes either). It is made once at its full size and written in place:
//! a material's bind group keeps the texture it was made with. Row 0 is all
//! zero: no tag, no rectangle. A row freed with its entity is given again.

use std::collections::BTreeSet;

use bevy::asset::{RenderAssetUsages, uuid_handle};
use bevy::image::ImageSampler;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use regent_core::view::pack::Rect;

use crate::material::{Band, Body};

/// The table.
pub const TABLE: Handle<Image> = uuid_handle!("6f0c1b6e-2a4f-4c47-9a35-1e9d6c0b5a71");

/// Rows a texture row of [`TABLE`]; `body.wesl` and `band.wesl` say
/// the same.
pub const WIDTH: u32 = 1024;

/// Texture rows of [`TABLE`]: 131,072 instances, 2 MB (**Inferred**:
/// past a late 512-cell game's 22,000 entities with room).
pub const HEIGHT: u32 = 128;

/// Words a texel: four `u32`.
const TEXEL_WORDS: usize = 4;

/// Texels a row.
const TEXELS: u32 = 2;

/// Words a row.
const WORDS: usize = TEXEL_WORDS * TEXELS as usize;

/// Bytes a word.
const WORD_BYTES: usize = 4;

/// Bytes a row.
const ROW_BYTES: usize = WORDS * WORD_BYTES;

/// Bits of a rectangle's x or width in a word; y or height above them.
const HALF: u32 = 16;

/// A frame's texels in its texture: on a sheet page, its rectangle; on
/// its own texture, the whole of it.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Framed(pub Rect);

/// What the shaders read beside the frame: a body's footing or slot
/// (`sun::stand`, `relief::model_tag`), a band's ground row
/// (`lie::encode`); 0 none.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tag(pub u32);

/// A tree's lean ([`crate::wind`]): its foot's height on the unit quad,
/// and the share of the lean a unit of height above it.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Lean {
    pub foot: f32,
    pub bend: f32,
}

/// An entity's row in [`TABLE`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row(pub u32);

/// The table on the CPU, the rows free to give again, and the texture
/// rows changed since the last upload.
#[derive(Resource, Debug)]
pub struct Rows {
    words: Vec<[u32; WORDS]>,
    free: Vec<u32>,
    touched: BTreeSet<u32>,
}

impl Default for Rows {
    /// Row 0 taken: the one an untagged instance reads.
    fn default() -> Self {
        Rows {
            words: vec![[0; WORDS]],
            free: Vec::new(),
            touched: BTreeSet::new(),
        }
    }
}

/// `tag`, `rect` and `lean` as a row.
#[must_use]
pub const fn row(tag: Tag, rect: Rect, lean: Lean) -> [u32; WORDS] {
    [
        tag.0,
        rect.x | rect.y << HALF,
        rect.w | rect.h << HALF,
        0,
        lean.foot.to_bits(),
        lean.bend.to_bits(),
        0,
        0,
    ]
}

/// The rectangle a row holds.
#[must_use]
pub const fn rect_of(row: [u32; WORDS]) -> Rect {
    let low = (1 << HALF) - 1;

    Rect {
        x: row[1] & low,
        y: row[1] >> HALF,
        w: row[2] & low,
        h: row[2] >> HALF,
    }
}

impl Rows {
    /// A row for an instance: a freed one, else a new one; row 0 (none)
    /// once the table is full.
    pub fn take(&mut self) -> u32 {
        let full = self.words.len() >= (WIDTH * HEIGHT) as usize;

        match (self.free.pop(), full) {
            (Some(at), _) => at,
            (None, false) => {
                self.words.push([0; WORDS]);

                u32::try_from(self.words.len() - 1).unwrap_or(0)
            }
            (None, true) => 0,
        }
    }

    /// Row `at` free to give again, cleared.
    pub fn give(&mut self, at: u32) {
        if at != 0 && (at as usize) < self.words.len() {
            self.set(at, [0; WORDS]);
            self.free.push(at);
        }
    }

    /// Row `at` set to `words`, its texture row noted if it changed.
    pub fn set(&mut self, at: u32, words: [u32; WORDS]) {
        if at != 0
            && let Some(was) = self.words.get_mut(at as usize)
            && *was != words
        {
            *was = words;
            self.touched.insert(at / WIDTH);
        }
    }

    /// Texture row `y`'s bytes, [`WIDTH`] rows of [`ROW_BYTES`].
    fn bytes(&self, y: u32) -> Vec<u8> {
        let first = (y * WIDTH) as usize;
        let mut bytes = vec![0u8; WIDTH as usize * ROW_BYTES];

        for (at, words) in bytes
            .as_chunks_mut::<ROW_BYTES>()
            .0
            .iter_mut()
            .zip(self.words.iter().skip(first))
        {
            for (word, value) in at.as_chunks_mut::<WORD_BYTES>().0.iter_mut().zip(words) {
                word.copy_from_slice(&value.to_le_bytes());
            }
        }

        bytes
    }
}

/// The table at its full size, all zero: written in place after.
#[must_use]
pub fn blank() -> Image {
    let mut image = Image::new(
        Extent3d {
            width: WIDTH * TEXELS,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; (WIDTH * HEIGHT) as usize * ROW_BYTES],
        TextureFormat::Rgba32Uint,
        RenderAssetUsages::RENDER_WORLD,
    );

    image.sampler = ImageSampler::nearest();
    image
}

/// The texture rows to write this frame, each its index and bytes.
#[derive(Resource, Default, Debug)]
pub struct Pending(Vec<(u32, Vec<u8>)>);

/// What [`write`] reads of an instance.
type Instance = (
    Entity,
    Option<&'static Framed>,
    (Option<&'static Tag>, Option<&'static Lean>),
    Option<&'static Row>,
);

/// Each body and band whose frame or tag changed, its row written; a
/// new one given a row and its [`MeshTag`]; the texture rows touched
/// handed to the render world.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
pub fn write(
    mut commands: Commands,
    (mut rows, mut pending): (ResMut<Rows>, ResMut<Pending>),
    changed: Query<
        Instance,
        (
            Or<(With<MeshMaterial3d<Body>>, With<MeshMaterial3d<Band>>)>,
            Or<(Changed<Framed>, Changed<Tag>, Changed<Lean>, Without<Row>)>,
        ),
    >,
) {
    for (entity, framed, (tag, lean), row) in &changed {
        let at = row.map_or_else(
            || {
                let at = rows.take();

                commands.entity(entity).insert((Row(at), MeshTag::new(at)));
                at
            },
            |row| row.0,
        );
        let words = self::row(
            tag.copied().unwrap_or_default(),
            framed.map(|f| f.0).unwrap_or_default(),
            lean.copied().unwrap_or_default(),
        );

        rows.set(at, words);
    }

    let touched = std::mem::take(&mut rows.touched);

    pending.0 = touched.into_iter().map(|y| (y, rows.bytes(y))).collect();
}

/// A row given back as its entity loses it (or is despawned).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy observer takes its parameters by value"
)]
pub fn freed(gone: On<Remove<Row>>, rows: Query<&Row>, mut table: ResMut<Rows>) {
    if let Ok(row) = rows.get(gone.entity) {
        table.give(row.0);
    }
}

/// The texture rows waiting for the table's texture, in the render world.
#[derive(Resource, Default)]
struct Queued(Vec<(u32, Vec<u8>)>);

/// This frame's touched rows, queued for [`upload`].
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn extract(pending: Extract<Res<Pending>>, mut queued: ResMut<Queued>) {
    queued.0.extend(pending.0.iter().cloned());
}

/// The queued rows written into the table's texture, once it is made.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn upload(
    images: Res<RenderAssets<GpuImage>>,
    queue: Res<RenderQueue>,
    mut queued: ResMut<Queued>,
) {
    if let Some(table) = images.get(&TABLE) {
        for (y, bytes) in queued.0.drain(..) {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &table.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(WIDTH * u32::try_from(ROW_BYTES).unwrap_or(0)),
                    rows_per_image: None,
                },
                Extent3d {
                    width: WIDTH * TEXELS,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

/// The table, its rows and their upload.
pub fn install(app: &mut App) {
    app.init_resource::<Rows>()
        .init_resource::<Pending>()
        .add_observer(freed)
        .add_systems(Startup, |mut images: ResMut<Assets<Image>>| {
            let _ = images.insert(&TABLE, blank());
        })
        .add_systems(Last, write);

    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .init_resource::<Queued>()
            .add_systems(ExtractSchedule, extract)
            .add_systems(Render, upload.in_set(RenderSystems::PrepareResources));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row holds a tag and rectangle and gives the rectangle back.
    #[test]
    fn a_row_round_trips_its_rectangle() {
        let rect = Rect {
            x: 4095,
            y: 17,
            w: 750,
            h: 502,
        };
        let lean = Lean {
            foot: -0.25,
            bend: 0.5,
        };
        let words = row(Tag(0x7F80_0003), rect, lean);

        assert_eq!(words[0], 0x7F80_0003);
        assert_eq!(rect_of(words), rect);
        assert_eq!(f32::from_bits(words[4]), lean.foot);
    }

    /// A freed row is given again, cleared; row 0 is never given.
    #[test]
    fn a_freed_row_is_given_again() {
        let mut rows = Rows::default();
        let first = rows.take();
        let second = rows.take();

        rows.set(first, [1; WORDS]);
        rows.give(first);

        assert_ne!(first, 0);
        assert_eq!(rows.take(), first);
        assert_eq!(rows.words[first as usize], [0; WORDS]);
        assert_ne!(second, first);
    }

    /// A changed row touches its texture row, whose bytes hold it.
    #[test]
    fn a_changed_row_touches_its_texture_row() {
        let mut rows = Rows::default();

        for _ in 0..=WIDTH {
            rows.take();
        }

        rows.touched.clear();
        rows.set(WIDTH, [7, 0, 0, 0, 0, 0, 0, 0]);
        rows.set(WIDTH, [7, 0, 0, 0, 0, 0, 0, 0]);

        assert_eq!(rows.touched, BTreeSet::from([1]));
        assert_eq!(rows.bytes(1)[0], 7);
        assert_eq!(rows.bytes(1).len(), WIDTH as usize * ROW_BYTES);
    }
}
