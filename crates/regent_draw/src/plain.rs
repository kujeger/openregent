//! A mesh's own colours under the 3D world camera, in place of 2D's
//! `ColorMaterial`: a colour, times its vertex colours and its picture
//! where it has them. The ground, a direct-colour frame, the fog's
//! reach, the hit point bars, the viewer's pictures.

use bevy::asset::{AssetPath, embedded_asset, embedded_path};
use bevy::mesh::{Mesh, MeshVertexBufferLayoutRef};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::{ShaderDefVal, ShaderRef, load_shader_library};

use crate::material::CUTOFF;

/// How a [`Plain`] meets what is under it: two of `ColorMaterial`'s three.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Alpha {
    /// Drawn whole where its alpha reaches [`CUTOFF`], else not at all.
    Mask,
    /// As [`Alpha::Mask`], but in Bevy's opaque phase, which draws before
    /// the mask phase: what a `Mask` lays under it then fails the early
    /// depth test, not shaded. The ground's patches over the floor
    /// (`map::floor`).
    MaskFirst,
    /// Blended over, in draw order.
    #[default]
    Blend,
}

/// Whether a ground mesh reads the models' shadows (`lit.wesl`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Shade {
    /// Nothing casts: no lookup, alpha 1, as the lookup gives then.
    #[default]
    Bare,
    /// Models cast (`sun::Cast::Models`): the lookup, into alpha.
    Cast,
}

/// Where a [`Plain`] mesh's vertices are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lay {
    /// As any mesh: its depth its own.
    #[default]
    Card,
    /// The ground: each vertex at its metric point, drawn at its
    /// translation's depth (`card.wesl`, [`crate::frame`]).
    Ground,
}

/// A mesh's colour and picture, its alpha and where its vertices are.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
#[bind_group_data(Key)]
pub struct Plain {
    #[uniform(0)]
    pub colour: LinearRgba,
    #[texture(1)]
    #[sampler(2)]
    pub picture: Option<Handle<Image>>,
    pub alpha: Alpha,
    pub lay: Lay,
    pub shade: Shade,
}

impl Default for Plain {
    fn default() -> Self {
        Plain {
            colour: LinearRgba::WHITE,
            picture: None,
            alpha: Alpha::Blend,
            lay: Lay::Card,
            shade: Shade::Bare,
        }
    }
}

impl Plain {
    /// A colour, blended.
    #[must_use]
    pub fn colour(colour: Color) -> Plain {
        Plain {
            colour: colour.into(),
            ..default()
        }
    }

    /// The ground's: `picture`, cut out at [`CUTOFF`] (cells without art
    /// leave holes), drawn first ([`Alpha::MaskFirst`]).
    #[must_use]
    pub fn ground(picture: Handle<Image>) -> Plain {
        Plain {
            picture: Some(picture),
            alpha: Alpha::MaskFirst,
            lay: Lay::Ground,
            ..default()
        }
    }
}

/// What a [`Plain`]'s pipeline is specialised on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub alpha: Alpha,
    pub lay: Lay,
    pub pictured: bool,
    pub shade: Shade,
}

impl From<&Plain> for Key {
    fn from(plain: &Plain) -> Key {
        Key {
            alpha: plain.alpha,
            lay: plain.lay,
            pictured: plain.picture.is_some(),
            shade: plain.shade,
        }
    }
}

/// What every world material's pipeline needs under the 3D camera: both
/// faces drawn, as in 2D (Bevy's 3D culls back faces, and a ground cell's
/// cut may wind either way).
pub const fn unculled(descriptor: &mut RenderPipelineDescriptor) {
    descriptor.primitive.cull_mode = None;
}

/// `defs` added to both stages of `descriptor`.
pub fn define(descriptor: &mut RenderPipelineDescriptor, defs: &[ShaderDefVal]) {
    descriptor.vertex.shader_defs.extend_from_slice(defs);

    if let Ok(fragment) = descriptor.fragment_mut() {
        fragment.shader_defs.extend_from_slice(defs);
    }
}

/// The vertex stage's def for a [`Lay::Ground`] mesh (`card.wesl`).
pub const GROUND: &str = "GROUND";

/// The fragment stage's def for a ground mesh under [`Shade::Cast`].
pub const CAST: &str = "CAST";

/// The ground's defs: [`GROUND`], and [`CAST`] under [`Shade::Cast`].
pub fn grounded(shade: Shade) -> Vec<ShaderDefVal> {
    let cast = (shade == Shade::Cast).then_some(CAST);

    [Some(GROUND), cast]
        .into_iter()
        .flatten()
        .map(Into::into)
        .collect()
}

impl Plain {
    /// [`Material::specialize`]'s work, without Bevy's pipeline: the
    /// compile tests call it.
    pub fn shape(descriptor: &mut RenderPipelineDescriptor, key: Key) {
        unculled(descriptor);

        let alpha = match key.alpha {
            Alpha::Mask | Alpha::MaskFirst => Some("MASK"),
            Alpha::Blend => None,
        };
        let mut defs: Vec<ShaderDefVal> = [alpha, key.pictured.then_some("PICTURE")]
            .into_iter()
            .flatten()
            .map(Into::into)
            .collect();

        if key.lay == Lay::Ground {
            defs.extend(grounded(key.shade));
        }

        define(descriptor, &defs);
    }
}

impl Material for Plain {
    fn vertex_shader() -> ShaderRef {
        Plain::fragment_shader()
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("shaders/plain.wesl")).with_source("embedded"),
        )
    }

    fn alpha_mode(&self) -> AlphaMode {
        match self.alpha {
            Alpha::Mask => AlphaMode::Mask(CUTOFF),
            // The shader still cuts out (`MASK`).
            Alpha::MaskFirst => AlphaMode::Opaque,
            Alpha::Blend => AlphaMode::Blend,
        }
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        Plain::shape(descriptor, key.bind_group_data);

        Ok(())
    }
}

/// The stores [`picture`] adds to, as a system takes them.
pub type Pictures<'w> = (
    ResMut<'w, Assets<Image>>,
    ResMut<'w, Assets<Mesh>>,
    ResMut<'w, Assets<Plain>>,
);

/// A picture drawn as a 2D sprite was: a quad `size` across, its point
/// `anchor` (a share of the size from the middle, y up, as Bevy's
/// sprite `Anchor`) on the entity's translation, blended.
pub fn picture(
    image: Handle<Image>,
    (size, anchor): (Vec2, Vec2),
    (meshes, plains): (&mut Assets<Mesh>, &mut Assets<Plain>),
) -> (Mesh3d, MeshMaterial3d<Plain>) {
    let quad = Mesh::from(Rectangle::from_size(size)).translated_by((-anchor * size).extend(0.0));
    let plain = Plain {
        picture: Some(image),
        ..default()
    };

    (Mesh3d(meshes.add(quad)), MeshMaterial3d(plains.add(plain)))
}

/// Register the material and the vertex stage it shares.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/plain.wesl");
        load_shader_library!(app, "shaders/card.wesl");

        app.add_plugins(MaterialPlugin::<Plain>::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ground draws in the opaque phase, before the mask phase the
    /// floor under it draws in, and still cuts out its holes.
    #[test]
    fn the_ground_draws_before_the_floor_and_cuts_out() {
        let ground = Plain::ground(Handle::default());
        let mut descriptor = RenderPipelineDescriptor::default();

        Plain::shape(&mut descriptor, Key::from(&ground));

        assert_eq!(ground.alpha_mode(), AlphaMode::Opaque);
        assert!(descriptor.vertex.shader_defs.contains(&"MASK".into()));
    }

    /// The defs a ground pipeline takes for `alpha` and `shade`.
    fn ground_defs(alpha: Alpha, shade: Shade) -> Vec<ShaderDefVal> {
        let ground = Plain {
            alpha,
            shade,
            ..Plain::ground(Handle::default())
        };
        let mut descriptor = RenderPipelineDescriptor::default();

        Plain::shape(&mut descriptor, Key::from(&ground));
        descriptor.vertex.shader_defs
    }

    /// The ground reads the shadow map only while models cast.
    #[test]
    fn the_ground_reads_the_shadow_map_only_while_models_cast() {
        assert!(!ground_defs(Alpha::MaskFirst, Shade::Bare).contains(&CAST.into()));
        assert!(ground_defs(Alpha::MaskFirst, Shade::Cast).contains(&CAST.into()));
        assert!(ground_defs(Alpha::MaskFirst, Shade::Cast).contains(&GROUND.into()));
    }
}
