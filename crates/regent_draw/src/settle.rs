//! The start-up draw work still pending, frame by frame: pipelines
//! compiling, the ground's patches the view wants, sprite pages made.
//! Settled: nothing pending [`QUIET`] frames in a row; at once without a
//! renderer. Interface pictures upscaling (`art::busy`) are not waited
//! for: they take minutes on a slow machine, and play needs none.
//!
//! ```text
//!  render world  Cleanup: pipelines waiting ──> Waiting (shared) ──┐
//!  main world    Last: Waiting, Maps::pending, pages made ─────────┴──> Settle::pending
//!                quiet frames counted ──> Settle::settled
//!  restart (a game put in) ──> waited for: what is pending said each LONG
//! ```
//!
//! A pipeline that will not compile leaves Bevy's waiting list: done.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

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

impl Pending {
    /// Nothing pending.
    const NONE: Pending = Pending {
        pipelines: 0,
        patches: 0,
        pages: 0,
    };

    /// Whether nothing is.
    #[must_use]
    pub const fn none(&self) -> bool {
        self.pipelines == 0 && self.patches == 0 && self.pages == 0
    }
}

/// Frames in a row nothing may be pending to be settled: a pipeline is
/// asked for a frame or two after its entity shows, and the render
/// world draws a frame behind.
pub const QUIET: u32 = 3;

/// How long a wait goes before what is pending is said, and again each
/// time after: a wait that never ends can be told apart.
const LONG: Duration = Duration::from_secs(10);

/// Whether the app draws: without a renderer nothing is ever pending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Renderer {
    Absent,
    Present,
}

/// The last frame's [`Pending`], and whether start-up's drawing has
/// settled.
#[derive(Resource)]
pub struct Settle {
    pending: Pending,
    /// [`sheet::pages_made`] as the last frame left it.
    made: u32,
    renderer: Renderer,
    /// Frames in a row with nothing pending.
    quiet: u32,
    /// Since [`Settle::restart`], until settled: when what is pending is
    /// next said.
    said: Option<Instant>,
}

/// As an app that draws: settled only after [`QUIET`] frames of nothing,
/// though without a renderer nothing is pending.
impl Default for Settle {
    fn default() -> Settle {
        Settle::new(Renderer::Present)
    }
}

impl Settle {
    const fn new(renderer: Renderer) -> Settle {
        Settle {
            pending: Pending::NONE,
            made: 0,
            renderer,
            quiet: 0,
            said: None,
        }
    }

    /// What the last frame left pending.
    #[must_use]
    pub const fn pending(&self) -> Pending {
        self.pending
    }

    /// Whether nothing has been pending [`QUIET`] frames in a row; at
    /// once without a renderer.
    #[must_use]
    pub const fn settled(&self) -> bool {
        match self.renderer {
            Renderer::Absent => true,
            Renderer::Present => self.quiet >= QUIET,
        }
    }

    /// Waited for afresh, as a game is put in: the frames before count
    /// for nothing.
    pub fn restart(&mut self) {
        self.quiet = 0;
        self.said = Some(Instant::now() + LONG);
    }

    /// A frame's `pending` taken: quiet frames counted; what is pending
    /// said, if waited for long.
    fn take(&mut self, pending: Pending) {
        self.pending = pending;
        self.quiet = if pending.none() {
            self.quiet.saturating_add(1)
        } else {
            0
        };

        if self.settled() {
            self.said = None;
        }

        if let Some(at) = self.said.filter(|at| Instant::now() >= *at) {
            warn!("drawing: start-up work still pending: {pending:?}");
            self.said = Some(at + LONG);
        }
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
        let renderer = if app.get_sub_app(RenderApp).is_some() {
            Renderer::Present
        } else {
            Renderer::Absent
        };

        app.insert_resource(Settle::new(renderer))
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
    let pending = Pending {
        pipelines: waiting.0.load(Ordering::Relaxed),
        patches: maps.map_or(0, |m| m.pending()),
        pages: made.wrapping_sub(settle.made),
    };

    settle.take(pending);
    settle.made = made;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame with `pages` made, nothing else pending.
    const fn paged(pages: u32) -> Pending {
        Pending {
            pages,
            ..Pending::NONE
        }
    }

    /// Settled after QUIET frames in a row of nothing; anything pending
    /// starts the count again, and so does a restart.
    #[test]
    fn settled_after_quiet_frames_in_a_row() {
        let mut settle = Settle::new(Renderer::Present);

        settle.restart();

        for _ in 1..QUIET {
            settle.take(paged(0));
        }

        assert!(!settle.settled());

        settle.take(paged(2));

        for _ in 0..QUIET {
            assert!(!settle.settled());
            settle.take(paged(0));
        }

        assert!(settle.settled());

        settle.restart();

        assert!(!settle.settled());
    }

    /// Interface pictures still upscaling hold nothing: they are made
    /// in the background, for minutes on a slow machine, and play does
    /// not need them.
    #[test]
    fn upscaling_pictures_do_not_unsettle() {
        let _upscaling = crate::art::Count::new();
        let mut app = App::new();

        app.insert_resource(Settle::new(Renderer::Present))
            .insert_resource(Waiting::default())
            .add_systems(Last, note);
        app.world_mut().resource_mut::<Settle>().restart();

        for _ in 0..QUIET {
            app.update();
        }

        assert!(app.world().resource::<Settle>().settled());
    }

    /// Without a renderer, settled at once, pending or not.
    #[test]
    fn without_a_renderer_settled_at_once() {
        let mut settle = Settle::new(Renderer::Absent);

        settle.restart();
        settle.take(paged(1));

        assert!(settle.settled());
    }
}
