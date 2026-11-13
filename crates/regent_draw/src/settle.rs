//! The start-up draw work still pending, frame by frame: pipelines
//! compiling, the ground's patches the view wants, sprite pages made.
//!
//! ```text
//!  render world  Cleanup: pipelines waiting ──> Waiting (shared) ──┐
//!  main world    Last: Waiting, Maps::pending, pages made ─────────┴──> Settle::pending
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::prelude::*;
use bevy::render::render_resource::PipelineCache;
use bevy::render::{Render, RenderApp, RenderSystems};

use crate::map::Maps;
use crate::map::sheet;

/// What one frame left pending.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    /// Pipelines queued or compiling, as the render world last stood.
    pub pipelines: usize,
    /// Ground patches the view wants, not yet made.
    pub patches: usize,
    /// Sprite pages made this frame.
    pub pages: u32,
}

/// The last frame's [`Pending`].
#[derive(Resource, Default)]
pub struct Settle {
    pending: Pending,
    /// [`sheet::pages_made`] as the last frame left it.
    made: u32,
}

impl Settle {
    /// What the last frame left pending.
    #[must_use]
    pub const fn pending(&self) -> Pending {
        self.pending
    }
}

/// Pipelines waiting, written by the render world.
#[derive(Resource, Clone, Default)]
struct Waiting(Arc<AtomicUsize>);

/// Where [`Settle`] is noted, in `Last`: a reader goes after.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Noted;

/// [`Settle`] kept each frame.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        let waiting = Waiting::default();

        app.init_resource::<Settle>()
            .insert_resource(waiting.clone())
            .add_systems(Last, note.in_set(Noted));

        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .insert_resource(waiting)
                .add_systems(Render, compiling.in_set(RenderSystems::Cleanup));
        }
    }
}

/// The pipelines still waiting after this frame's compiles.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn compiling(cache: Res<PipelineCache>, waiting: Res<Waiting>) {
    waiting
        .0
        .store(cache.waiting_pipelines().count(), Ordering::Relaxed);
}

/// The frame's [`Pending`] noted, after its drawing.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn note(mut settle: ResMut<Settle>, waiting: Res<Waiting>, maps: Option<Res<Maps>>) {
    let made = sheet::pages_made();

    settle.pending = Pending {
        pipelines: waiting.0.load(Ordering::Relaxed),
        patches: maps.map_or(0, |m| m.pending()),
        pages: made.wrapping_sub(settle.made),
    };
    settle.made = made;
}
