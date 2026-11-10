//! Each model's relief: where on its building a sprite pixel lies, and
//! whether the model's own body keeps the sun off it. With it a model
//! building takes the other models' shadows and never its own: its
//! sprite has its own shade painted in (`body.wesl`). Under
//! [`Own::Cast`] it takes its own too: a texel on the model is never
//! SHADED, and its lift only clears its own face (`bias`); one off it
//! is as by default, else its upright point (behind the model, for the
//! ground in its sprite) speckles with the model's own shadow.
//!
//! ```text
//!  grid     COLUMNS × ROWS texels a model, one a world unit: the sprite's
//!           own pixels at zoom 1, the anchor on the corner of texel
//!           (ANCHOR_COLUMN, ANCHOR_ROW), rows down
//!  texel    r: the model's depth at the texel's centre (world z, toward
//!           the viewer) over the upright plane through the anchor: the
//!           pixel is its footing's upright point (frame::point) moved
//!           that far. 0 where the model does not cover it: the pixel
//!           stands upright over its footing, as a building without a
//!           model does. SHADED where the model's own body lies between
//!           that point and the sun: no models' shadow there, the paint
//!           has its shade.
//!           g: how far toward the sun the point is moved before the
//!           shadow map is read: past its own model's faces within the
//!           map filter's reach (reach: a sized sun's penumbra search
//!           too), so they darken nothing (no acne, no fringe along its
//!           own shade); another building's lie farther
//!  layers   one a model, in Proxies' order (RELIEF)
//!  slots    one a casting building (STANDS): its anchor's drawn point,
//!           its footing's flat ground and its model's layer; its bodies
//!           carry the slot in their MeshTag (model_tag)
//! ```
//!
//! Both are worked out on the CPU: the view and the sun are orthographic
//! and a model is rigid, so a model's relief is the same wherever it
//! stands, and changes only with the sun.

use bevy::asset::{RenderAssetUsages, uuid_handle};
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

use super::Own;
use super::cast::Light;
use super::model::{Model, Proxies};
use crate::frame;

/// Every model's relief, a layer each.
pub const RELIEF: Handle<Image> = uuid_handle!("5d0f4a5e-7c55-4f53-8a5b-3f0d2b8a6c11");

/// Every casting building's slot: anchor, footing and layer.
pub const STANDS: Handle<Image> = uuid_handle!("a4c2e9b1-0f6d-4b8e-9d3a-6e1f7c2b5a90");

/// A relief layer's texels across.
pub const COLUMNS: u32 = 384;

/// A relief layer's texels down.
pub const ROWS: u32 = 384;

/// The column of the texel whose top-left corner is a model's anchor:
/// room either side (`every_model_fits_its_grid`).
pub const ANCHOR_COLUMN: i32 = 192;

/// That texel's row: room for the shipped models' tops above it and
/// their footprints below.
pub const ANCHOR_ROW: i32 = 224;

/// A texel the model's own body keeps the sun off.
pub const SHADED: f32 = 1.0e9;

/// Slots [`STANDS`] holds: casting buildings past it take no models'
/// shadows, as before.
pub const SLOTS: u32 = 1024;

/// A [`MeshTag`] naming a slot: the exponent bits of an f32's infinity,
/// which no footing's height has (`stand`), the slot in the rest.
const SLOT_TAG: u32 = 0x7F80_0000;

/// The sun's grid, world units a texel.
const SUN_STEP: f32 = 0.5;

/// How far about a point the shadow map's filter reads, world units:
/// its own model's faces within it, nearer the sun, would shade it
/// (Bevy's Gaussian filter: about two texels, under 1.5 units at zoom
/// 1, 3 at zoom 2). **Inferred**: by eye, from the filter's taps.
const FILTER_REACH: f32 = 4.0;

/// How far under its own model's surface, toward the sun, a point may
/// lie and still be lit, world units: a texel's centre on a face, the
/// sun's texel's face extended to it. **Inferred**: by eye, against
/// the models' faces.
const CLEAR: f32 = 0.5;

/// The most a lit face's depth across the light changes a world unit
/// across it, under [`Own::Cast`]: tan 76°, a face that near edge on to
/// the sun lifted no farther. **Inferred**: a bound, not measured.
const STEEPEST: f32 = 4.0;

/// The tag of a body on slot `slot`'s building.
#[must_use]
pub const fn model_tag(slot: u32) -> MeshTag {
    MeshTag::new(SLOT_TAG | slot)
}

/// A grid of square texels, rows down from its top-left corner.
struct Grid {
    corner: Vec2,
    step: f32,
    columns: usize,
    rows: usize,
}

impl Grid {
    /// The centre of texel (`column`, `row`).
    #[allow(clippy::cast_precision_loss, reason = "a few thousand texels")]
    fn centre(&self, column: usize, row: usize) -> Vec2 {
        self.corner + Vec2::new(column as f32 + 0.5, -(row as f32 + 0.5)) * self.step
    }

    /// The texel `p` lies in, if on the grid.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "checked on the grid first"
    )]
    fn texel(&self, p: Vec2) -> Option<usize> {
        let column = ((p.x - self.corner.x) / self.step).floor();
        let row = ((self.corner.y - p.y) / self.step).floor();
        let inside =
            (0.0..self.columns as f32).contains(&column) && (0.0..self.rows as f32).contains(&row);

        inside.then(|| row as usize * self.columns + column as usize)
    }

    /// Each texel whose centre lies in triangle `t`, with the centre's
    /// barycentric weights.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "clamped to the grid"
    )]
    fn cover(&self, t: [Vec2; 3], mut each: impl FnMut(usize, Vec3)) {
        let Some(weights) = Weights::of(t) else {
            return;
        };
        let min = t[0].min(t[1]).min(t[2]);
        let max = t[0].max(t[1]).max(t[2]);
        let span = |low: f32, high: f32, end: usize| {
            let first = low.floor().max(0.0) as usize;
            let last = (high.ceil().max(0.0) as usize).min(end);

            first..last
        };
        let columns = span(
            (min.x - self.corner.x) / self.step,
            (max.x - self.corner.x) / self.step,
            self.columns,
        );
        let rows = span(
            (self.corner.y - max.y) / self.step,
            (self.corner.y - min.y) / self.step,
            self.rows,
        );

        for row in rows {
            for column in columns.clone() {
                let w = weights.at(self.centre(column, row));

                if w.min_element() >= -1e-5 {
                    each(row * self.columns + column, w);
                }
            }
        }
    }
}

/// A 2D triangle's barycentric weights at any point, inside or out.
struct Weights {
    t: [Vec2; 3],
    area: f32,
}

impl Weights {
    /// `None` for a triangle seen edge on.
    fn of(t: [Vec2; 3]) -> Option<Weights> {
        let area = (t[1] - t[0]).perp_dot(t[2] - t[0]);

        (area.abs() > 1e-6).then_some(Weights { t, area })
    }

    fn at(&self, p: Vec2) -> Vec3 {
        let [a, b, c] = self.t;

        Vec3::new(
            (b - p).perp_dot(c - p),
            (c - p).perp_dot(a - p),
            (a - p).perp_dot(b - p),
        ) / self.area
    }
}

/// `model`'s faces in the world frame about its anchor ([`frame::ART`]).
fn turned(model: &Model) -> Vec<[Vec3; 3]> {
    model
        .triangles
        .iter()
        .map(|t| t.map(|v| frame::ART * v))
        .collect()
}

/// The relief grid of every model.
const fn view_grid() -> Grid {
    #[allow(clippy::cast_precision_loss, reason = "a few hundred")]
    let corner = Vec2::new(-ANCHOR_COLUMN as f32, ANCHOR_ROW as f32);

    Grid {
        corner,
        step: 1.0,
        columns: COLUMNS as usize,
        rows: ROWS as usize,
    }
}

/// Each texel's model depth and face: the nearest face to the viewer's
/// world z at its centre, or `None` off the model.
fn depths(faces: &[[Vec3; 3]], grid: &Grid) -> Vec<Option<(f32, usize)>> {
    let mut out = vec![None; grid.columns * grid.rows];

    for (k, face) in faces.iter().enumerate() {
        let drawn = face.map(Vec3::truncate);
        let z = Vec3::new(face[0].z, face[1].z, face[2].z);

        grid.cover(drawn, |i, w| {
            let here = w.dot(z);

            if out[i].is_none_or(|(was, _)| here > was) {
                out[i] = Some((here, k));
            }
        });
    }

    out
}

/// How far about a point the shadow map is read under `light`, world
/// units: the filter's `FILTER_REACH`, and a sized sun's penumbra
/// search past it ([`Light::search`]).
#[must_use]
pub fn reach(light: Light) -> f32 {
    FILTER_REACH + light.search()
}

/// The model as the sun sees it: the face nearest the sun over each
/// texel of a grid across the light, and the nearest the sun any face
/// comes within a reach ([`reach`]).
struct Sunward {
    /// World to light: across, across, toward the sun.
    basis: Mat3,
    grid: Grid,
    faces: Vec<[Vec3; 3]>,
    nearest: Vec<Option<usize>>,
    reach: Vec<f32>,
}

impl Sunward {
    fn of(faces: &[[Vec3; 3]], toward: Vec3, reach: f32) -> Sunward {
        let (across, up) = toward.any_orthonormal_pair();
        let basis = Mat3::from_cols(across, up, toward).transpose();
        let lit: Vec<[Vec3; 3]> = faces.iter().map(|f| f.map(|v| basis * v)).collect();
        let (min, max) = lit.iter().flatten().fold(
            (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
            |(min, max), v| (min.min(v.truncate()), max.max(v.truncate())),
        );

        // Room for the reach past the model on every side.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few dozen texels"
        )]
        let around = (reach / SUN_STEP).ceil() as usize + 1;
        #[allow(clippy::cast_precision_loss, reason = "a few texels")]
        let margin = around as f32 * SUN_STEP;

        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a model a few hundred units across"
        )]
        let count = |span: f32| (span / SUN_STEP).ceil().max(0.0) as usize + 2 * around;
        let grid = Grid {
            corner: Vec2::new(min.x - margin, max.y + margin),
            step: SUN_STEP,
            columns: count(max.x - min.x),
            rows: count(max.y - min.y),
        };
        let mut height = vec![f32::MIN; grid.columns * grid.rows];
        let mut nearest = vec![None; grid.columns * grid.rows];

        for (k, face) in lit.iter().enumerate() {
            let z = Vec3::new(face[0].z, face[1].z, face[2].z);

            grid.cover(face.map(Vec3::truncate), |i, w| {
                let here = w.dot(z);

                if here > height[i] {
                    height[i] = here;
                    nearest[i] = Some(k);
                }
            });
        }

        let reach = widened(&height, &grid, around);

        Sunward {
            basis,
            grid,
            faces: lit,
            nearest,
            reach,
        }
    }

    /// Whether the model's own body lies between world point `p` (about
    /// the anchor) and the sun: the face nearest the sun over it, its
    /// plane taken to `p`, more than [`CLEAR`] nearer the sun.
    fn shades(&self, p: Vec3) -> bool {
        let q = self.basis * p;
        let face = self
            .grid
            .texel(q.truncate())
            .and_then(|i| self.nearest[i])
            .map(|k| self.faces[k]);
        let top = face.and_then(|f| {
            let weights = Weights::of(f.map(Vec3::truncate))?;

            Some(
                weights
                    .at(q.truncate())
                    .dot(Vec3::new(f[0].z, f[1].z, f[2].z)),
            )
        });

        top.is_some_and(|top| top > q.z + CLEAR)
    }

    /// How far toward the sun `p` must move to be nearer it than every
    /// face of the model within the reach across the light.
    fn lift(&self, p: Vec3) -> f32 {
        let q = self.basis * p;
        let top = self
            .grid
            .texel(q.truncate())
            .map_or(f32::MIN, |i| self.reach[i]);

        (top - q.z).max(0.0)
    }
}

/// `height`, each texel the most of those within `around` texels of
/// it, a square about it: a max filter, rows then columns.
fn widened(height: &[f32], grid: &Grid, around: usize) -> Vec<f32> {
    let columns = grid.columns;
    let across: Vec<f32> = height
        .chunks(columns)
        .flat_map(|row| slide(row, around))
        .collect();
    let mut out = vec![f32::MIN; height.len()];

    for column in 0..columns {
        let line: Vec<f32> = across
            .iter()
            .skip(column)
            .step_by(columns)
            .copied()
            .collect();

        for (row, most) in slide(&line, around).into_iter().enumerate() {
            out[row * columns + column] = most;
        }
    }

    out
}

/// `values`, each the most of those within `around` of it, in time
/// linear in their count whatever `around` (van Herk, Gil and Werman):
/// blocks a window long, each value's most from its block's start
/// (`ahead`) and to its block's end (`behind`); a window spans two
/// blocks, or is one. Padded past both ends, so every window is whole.
fn slide(values: &[f32], around: usize) -> Vec<f32> {
    let window = 2 * around + 1;
    let pad = std::iter::repeat_n(f32::MIN, around);
    let padded: Vec<f32> = pad
        .clone()
        .chain(values.iter().copied())
        .chain(pad)
        .collect();
    let mut ahead = padded.clone();
    let mut behind = padded;

    for i in 1..ahead.len() {
        if i % window != 0 {
            ahead[i] = ahead[i].max(ahead[i - 1]);
        }
    }

    for i in (0..behind.len().saturating_sub(1)).rev() {
        if (i + 1) % window != 0 {
            behind[i] = behind[i].max(behind[i + 1]);
        }
    }

    (0..values.len())
        .map(|i| behind[i].max(ahead[i + 2 * around]))
        .collect()
}

/// How far toward the sun (`toward`) a point on face `face` moves so
/// the face does not shade itself through the map's filter: a
/// slope-scaled depth bias. Its depth across the light changes by
/// tan θ a unit across (θ the sun off its normal), so by
/// [`FILTER_REACH`] × tan θ within the filter, capped at [`STEEPEST`];
/// plus [`CLEAR`]. Not by a sized sun's search reach: that lifts past
/// the model's own nearby shade (an eave's on its wall). Its face
/// found in the search lies near, so it asks for the least filter,
/// which this covers (`shaders/lit.wesl`). None on a face turned from the sun: its model's own
/// body is between it and the sun. Moving along the light keeps the
/// point's texel in the map, so other casters' shadows stay put.
fn bias(face: [Vec3; 3], toward: Vec3) -> f32 {
    let normal = (face[1] - face[0])
        .cross(face[2] - face[0])
        .normalize_or_zero();
    // The side the viewer sees: toward +z.
    let normal = normal * normal.z.signum();
    let cos = normal.dot(toward);

    if cos > 0.0 {
        let tan = (1.0 - cos * cos).max(0.0).sqrt() / cos;

        CLEAR + FILTER_REACH * tan.min(STEEPEST)
    } else {
        0.0
    }
}

/// `model`'s relief layer under a sun `toward` (world frame, a unit
/// vector), the map read `reach` about a point ([`reach`]):
/// [`COLUMNS`] × [`ROWS`] texels, rows down, each its depth and its
/// lift: SHADED where its own body shades it, lifted past its own faces
/// within the reach; under [`Own::Cast`], on the model, never SHADED,
/// lifted off its own face alone (`bias`).
#[must_use]
pub fn relief(model: &Model, toward: Vec3, reach: f32, own: Own) -> Vec<[f32; 2]> {
    let faces = turned(model);
    let grid = view_grid();
    let sunward = Sunward::of(&faces, toward, reach);
    let tan = frame::ELEVATION.to_radians().tan();

    depths(&faces, &grid)
        .into_iter()
        .enumerate()
        .map(|(i, nearest)| {
            let centre = grid.centre(i % grid.columns, i / grid.columns);
            let upright = centre.y * tan;
            let delta = nearest.map_or(0.0, |(z, _)| z - upright);
            let point = centre.extend(upright + delta);

            match (own, nearest) {
                (Own::Cast, Some((_, k))) => [delta, bias(faces[k], toward)],
                _ if sunward.shades(point) => [SHADED, 0.0],
                _ => [delta, sunward.lift(point)],
            }
        })
        .collect()
}

/// `layers` as [`RELIEF`]'s texture: one array layer each; no layers, one
/// empty (a texture has at least one).
fn layered(layers: &[Vec<[f32; 2]>]) -> Image {
    let blank = vec![[0.0; 2]; (COLUMNS * ROWS) as usize];
    let data = if layers.is_empty() {
        std::slice::from_ref(&blank)
    } else {
        layers
    }
    .iter()
    .flatten()
    .flatten()
    .flat_map(|v| v.to_le_bytes())
    .collect();

    #[allow(clippy::cast_possible_truncation, reason = "a few models")]
    let count = layers.len().max(1) as u32;
    let mut image = Image::new(
        Extent3d {
            width: COLUMNS,
            height: ROWS,
            depth_or_array_layers: count,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rg32Float,
        RenderAssetUsages::RENDER_WORLD,
    );

    // An array even with one layer: the shader reads one.
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    image
}

/// One slot's building: its anchor's drawn point, its footing's flat
/// ground height and its model's layer, world units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stand {
    pub anchor: Vec2,
    pub flat: f32,
    pub layer: u32,
}

/// `stands` as [`STANDS`]' texture, [`SLOTS`] wide; past them dropped.
fn stood(stands: &[Stand]) -> Image {
    #[allow(clippy::cast_precision_loss, reason = "a few models")]
    let texel = |s: &Stand| [s.anchor.x, s.anchor.y, s.flat, s.layer as f32];
    let data = stands
        .iter()
        .map(texel)
        .chain(std::iter::repeat([0.0; 4]))
        .take(SLOTS as usize)
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect();

    Image::new(
        Extent3d {
            width: SLOTS,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba32Float,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Both textures at their full size, empty: the size a material binds
/// stays (`gpu_image` writes a same-sized one in place).
pub fn start(mut images: ResMut<Assets<Image>>, proxies: Option<Res<Proxies>>) {
    let count = proxies.map_or(0, |p| p.len());
    let empty = vec![vec![[0.0; 2]; (COLUMNS * ROWS) as usize]; count];

    // A Uuid handle takes any asset.
    let _ = images.insert(&RELIEF, layered(&empty));
    let _ = images.insert(&STANDS, stood(&[]));
}

/// [`RELIEF`] for every model under `light`, shading itself or not
/// (`own`).
pub fn bake(images: &mut Assets<Image>, proxies: &Proxies, light: Light, own: Own) {
    let (toward, reach) = (light.toward(), reach(light));
    let mut layers: Vec<Vec<[f32; 2]>> = proxies
        .all()
        .map(|(_, m)| relief(m, toward, reach, own))
        .collect();

    // As many layers as [`start`] made: the size stays.
    layers.resize(proxies.len(), vec![[0.0; 2]; (COLUMNS * ROWS) as usize]);

    let _ = images.insert(&RELIEF, layered(&layers));
}

/// [`STANDS`] for `stands`, slot by slot.
pub fn seat(images: &mut Assets<Image>, stands: &[Stand]) {
    let _ = images.insert(&STANDS, stood(stands));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sun::Sun;

    /// The default sun, toward it in the world frame.
    fn sun() -> Vec3 {
        Sun::default().light.toward()
    }

    /// A building-sized model's drawn extent sits inside its grid, with
    /// room for a sprite's flags and fringe past it. The guardhouse
    /// fixture is one.
    #[test]
    fn a_building_model_fits_its_grid() {
        const ROOM: f32 = 32.0;

        let grid = view_grid();
        let model = Model::gltf(include_bytes!("../../tests/fixtures/guardhouse.glb"), None)
            .expect("reads");

        for face in turned(&model) {
            for v in face {
                let p = v.truncate();
                let inside = grid.texel(p + Vec2::splat(ROOM)).is_some()
                    && grid.texel(p - Vec2::splat(ROOM)).is_some();

                assert!(inside, "{p}");
            }
        }
    }

    /// The shaders' copies of the grid's constants are these.
    #[test]
    fn the_shaders_constants_are_these() {
        let source = include_str!("../shaders/body.wesl");
        let value = |name: &str| {
            source
                .lines()
                .find_map(|l| l.strip_prefix(&format!("const {name}: ")))
                .and_then(|l| l.split_once(" = "))
                .map_or_else(
                    || panic!("{name}"),
                    |(_, v)| v.trim_end_matches(';').to_string(),
                )
        };

        assert_eq!(value("ANCHOR_COLUMN"), format!("{ANCHOR_COLUMN}"));
        assert_eq!(value("ANCHOR_ROW"), format!("{ANCHOR_ROW}"));
        assert_eq!(value("SLOT_TAG"), format!("{SLOT_TAG:#X}u"));
        assert_eq!(value("SHADED").parse::<f32>().ok(), Some(SHADED));
    }

    /// A box 20 wide, 40 tall about the anchor: its relief, shading
    /// itself or not (`own`).
    fn tower_as(own: Own) -> Vec<[f32; 2]> {
        let model = Model::parse(concat!(
            "v -10 -10 0\nv 10 -10 0\nv 10 10 0\nv -10 10 0\n",
            "v -10 -10 40\nv 10 -10 40\nv 10 10 40\nv -10 10 40\n",
            "f 1 2 3 4\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n",
        ))
        .expect("reads");

        relief(&model, sun(), FILTER_REACH, own)
    }

    /// The box's relief as by default.
    fn tower() -> Vec<[f32; 2]> {
        tower_as(Own::Painted)
    }

    /// The value at drawn point `p` about the anchor.
    fn at(layer: &[[f32; 2]], p: Vec2) -> f32 {
        layer[view_grid().texel(p).expect("on the grid")][0]
    }

    /// The lift at drawn point `p` about the anchor.
    fn lift(layer: &[[f32; 2]], p: Vec2) -> f32 {
        layer[view_grid().texel(p).expect("on the grid")][1]
    }

    /// A box's front corner, nearest the viewer, lies in front of the
    /// upright plane through the anchor, its roof's middle on it.
    #[test]
    fn a_box_stands_out_of_its_upright_plane() {
        let layer = tower();
        let front = frame::ART * Vec3::new(-10.0, -10.0, 20.0);
        let roof = frame::ART * Vec3::new(0.0, 0.0, 40.0);
        let tan = frame::ELEVATION.to_radians().tan();
        let near = at(&layer, front.truncate());
        let expected = front.z - front.y * tan;

        assert!(near > 0.0 && near < SHADED, "{near}");
        assert!((near - expected).abs() < 1.0, "{near} {expected}");
        assert!(at(&layer, roof.truncate()).abs() < 1.0);
    }

    /// The box's side away from the sun is its own shade; the side toward
    /// it is lit; a point off the box upright is lit.
    #[test]
    fn a_box_shades_its_far_side_alone() {
        let layer = tower();

        // The sun at φ 29° is right of the screen and toward the viewer:
        // the box's left face (X = −10 side seen on screen left) is shaded.
        let left = frame::ART * Vec3::new(-10.0, 0.0, 20.0);
        let right = frame::ART * Vec3::new(0.0, -10.0, 20.0);

        assert!(at(&layer, left.truncate() + Vec2::X) >= SHADED);
        assert!(at(&layer, right.truncate() - Vec2::X) < SHADED);
        assert!(at(&layer, Vec2::new(0.0, 200.0)).abs() < f32::EPSILON);
    }

    /// A lit point by its own shade's edge moves toward the sun past the
    /// box's faces there; one well clear of the box does not move.
    #[test]
    fn a_lit_point_by_its_own_shade_moves_clear_of_it() {
        let layer = tower();

        // The lit face's left edge, at the corner nearest the viewer,
        // where the shaded face meets it.
        let edge = frame::ART * Vec3::new(-10.0, -10.0, 20.0);
        let lit = edge.truncate() + Vec2::X * 1.5;

        assert!(at(&layer, lit) < SHADED);
        assert!(lift(&layer, lit) > 0.0, "{}", lift(&layer, lit));
        assert!(lift(&layer, Vec2::new(0.0, 200.0)).abs() < f32::EPSILON);
    }

    /// Shading itself, the box's far side is a point on it, unlifted, and
    /// its lit side is lifted at least CLEAR, at the same depth as by
    /// default; off the box as by default.
    #[test]
    fn a_box_shading_itself_marks_no_shade() {
        let painted = tower();
        let layer = tower_as(Own::Cast);
        let left = frame::ART * Vec3::new(-10.0, 0.0, 20.0);
        let right = frame::ART * Vec3::new(0.0, -10.0, 20.0);
        let far = left.truncate() + Vec2::X;
        let lit = right.truncate() - Vec2::X;

        let off = Vec2::new(0.0, 200.0);

        assert!(at(&layer, far) < SHADED);
        assert!(lift(&layer, far).abs() < f32::EPSILON);
        assert!(lift(&layer, lit) >= CLEAR, "{}", lift(&layer, lit));
        assert!((at(&layer, lit) - at(&painted, lit)).abs() < f32::EPSILON);
        assert_eq!(
            layer[view_grid().texel(off).expect("on the grid")],
            painted[view_grid().texel(off).expect("on the grid")]
        );
    }

    /// A slot's tag is never a footing's, nor none.
    #[test]
    fn a_slot_tag_is_no_footing() {
        for slot in [0, 1, SLOTS - 1] {
            let tag = model_tag(slot).value;

            assert!(!f32::from_bits(tag).is_finite());
            assert_ne!(tag, 0);
            assert_eq!(tag & !SLOT_TAG, slot);
        }
    }

    /// The linear sliding max is the plain one: each value the most
    /// within `around` of it, ends clipped.
    #[test]
    #[allow(clippy::float_cmp, reason = "the same values picked")]
    fn the_sliding_max_is_the_plain_one() {
        let values: Vec<f32> = (0..37_u16)
            .map(|i| f32::from((i * 17 + 5) % 23) - 11.0)
            .collect();

        for around in [0, 1, 3, 8, 40] {
            let plain: Vec<f32> = (0..values.len())
                .map(|i| {
                    let span = i.saturating_sub(around)..(i + around + 1).min(values.len());

                    values[span].iter().copied().fold(f32::MIN, f32::max)
                })
                .collect();

            assert_eq!(slide(&values, around), plain, "{around}");
        }
    }

    /// A sized sun reads farther: a point clear of the box's faces at
    /// the filter's reach is lifted past them at a wider one.
    #[test]
    fn a_wider_reach_lifts_farther_from_the_box() {
        let model = Model::parse(concat!(
            "v -10 -10 0\nv 10 -10 0\nv 10 10 0\nv -10 10 0\n",
            "v -10 -10 40\nv 10 -10 40\nv 10 10 40\nv -10 10 40\n",
            "f 1 2 3 4\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n",
        ))
        .expect("reads");
        let near = relief(&model, sun(), FILTER_REACH, Own::Painted);
        let wide = relief(&model, sun(), FILTER_REACH + 12.0, Own::Painted);
        let total = |layer: &[[f32; 2]]| layer.iter().map(|t| t[1]).sum::<f32>();

        assert!(total(&wide) > total(&near));
        assert!(near.iter().zip(&wide).all(|(n, w)| w[1] >= n[1]));
    }

    /// With no models the texture still holds its one layer: data and
    /// size agree, at `start` and at `bake`.
    #[test]
    fn no_models_make_one_empty_layer() {
        let layer = (COLUMNS * ROWS) as usize * size_of::<[f32; 2]>();
        let mut images = Assets::<Image>::default();
        let none = Proxies::default();

        assert_eq!(layered(&[]).data.as_ref().map(Vec::len), Some(layer));

        bake(&mut images, &none, Sun::default().light, Own::Painted);

        let baked = images.get(&RELIEF).expect("baked");

        assert_eq!(baked.data.as_ref().map(Vec::len), Some(layer));
        assert_eq!(baked.texture_descriptor.size.depth_or_array_layers, 1);
    }
}
