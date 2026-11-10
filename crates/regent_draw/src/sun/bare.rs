//! The models themselves, drawn plain grey in their buildings' place
//! while their sprites are hidden ([`Sprites`], openregent's debug key O):
//! to see what casts each shadow.
//!
//! ```text
//!  shape     the model in the world's metric frame, as its caster
//!  depth     the building's draw depth (its piece's, in the mesh tag),
//!            each point moved by its metric depth times SQUEEZE, so the
//!            model hides its own back faces near the building's place
//!            in the order (**Inferred**: close enough for a look)
//!  colour    grey, lit by the sun's angle on each face, darkened by the
//!            models' shadows (`lit.wesl`)
//! ```

use bevy::asset::{AssetPath, embedded_asset, embedded_path};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

/// Whether a building casting from its model draws its sprite, or its
/// model plain in its place.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sprites {
    #[default]
    Drawn,
    Hidden,
}

impl Sprites {
    /// The other one.
    #[must_use]
    pub const fn toggled(self) -> Sprites {
        match self {
            Sprites::Drawn => Sprites::Hidden,
            Sprites::Hidden => Sprites::Drawn,
        }
    }
}

/// A model drawn plain: no fields, the shader's own grey.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub struct Bare {}

impl Material for Bare {
    fn vertex_shader() -> ShaderRef {
        Bare::fragment_shader()
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("bare.wesl")).with_source("embedded"),
        )
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// Both faces, as every world material (`plain::unculled`): a model's
    /// faces wind either way.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        crate::plain::unculled(descriptor);

        Ok(())
    }
}

/// Register the material and its shader.
pub(super) fn register(app: &mut App) {
    embedded_asset!(app, "bare.wesl");

    app.add_plugins(MaterialPlugin::<Bare>::default())
        .init_resource::<Sprites>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The key flips the sprites between drawn and hidden, and back.
    #[test]
    fn the_sprites_toggle_back_and_forth() {
        assert_eq!(Sprites::default(), Sprites::Drawn);
        assert_eq!(Sprites::Drawn.toggled(), Sprites::Hidden);
        assert_eq!(Sprites::Hidden.toggled(), Sprites::Drawn);
    }
}
