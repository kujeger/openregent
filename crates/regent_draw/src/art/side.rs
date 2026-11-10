//! The GPU's largest texture side, told egui: egui caps a texture there
//! (`max_texture_side`, 2048 untold), and the art service its upscales
//! (`scale::capped`).

use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;
use bevy_egui::{EguiInput, EguiPreUpdateSet};

/// The GPU's largest 2D texture side, pixels.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Side(pub usize);

/// Reads [`Side`] and tells egui.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, read)
            .add_systems(PreUpdate, tell.before(EguiPreUpdateSet::BeginPass));
    }
}

/// [`Side`] from the GPU, where there is one.
fn read(mut commands: Commands, device: Option<Res<RenderDevice>>) {
    if let Some(device) = device {
        let side = device.limits().max_texture_dimension_2d;

        commands.insert_resource(Side(side as usize));
    }
}

/// egui told [`Side`] each pass: its input is new each pass.
fn tell(side: Option<Res<Side>>, mut inputs: Query<&mut EguiInput>) {
    if let Some(side) = side {
        for mut input in &mut inputs {
            input.max_texture_side = Some(side.0);
        }
    }
}
