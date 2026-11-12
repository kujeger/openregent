//! Bevy's threads as the game runs them: every schedule on the
//! single-threaded executor.
//!
//! ```text
//!  game thread     the rules (game::thread)
//!  main thread     the main world's schedules, each system in turn
//!  render thread   the render world's schedules, each system in turn
//!  pools           the ground's patches (compute), upscaling (async compute)
//! ```
//!
//! Bevy's multithreaded executor wakes pool threads for each system. The
//! frame's systems are many and short, so the waking costs more than the
//! spread gains: on two cores a late game's frame takes half the time
//! single-threaded.
//!
//! Switches for the perf runs, each read once:
//!
//! ```text
//!  MAJ_PERF_EXECUTOR=multi   Bevy's multithreaded executor
//!  MAJ_PERF_POOL=<n>         the compute pool's threads; IO and async compute 1 each
//!  MAJ_PERF_UNPIPELINED=1    no render thread: main and render worlds on one
//! ```
//!
//! Bevy's default pools (`TaskPoolOptions`) take the affinity mask's cores:
//! 2 cores give compute 1, IO 1, async compute 1; 16 give 8, 4, 4.

use bevy::app::{
    PluginGroupBuilder, TaskPoolOptions, TaskPoolPlugin, TaskPoolThreadAssignmentPolicy,
};
use bevy::ecs::schedule::{Schedules, SingleThreadedExecutor};
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;

/// Bevy's own executor, e.g. `MAJ_PERF_EXECUTOR=multi`.
pub const EXECUTOR: &str = "MAJ_PERF_EXECUTOR";

/// [`EXECUTOR`]'s word for Bevy's multithreaded executor.
const MULTI: &str = "multi";

/// The compute pool's thread count, e.g. `MAJ_PERF_POOL=2`.
pub const POOL: &str = "MAJ_PERF_POOL";

/// No render thread, e.g. `MAJ_PERF_UNPIPELINED=1`.
pub const UNPIPELINED: &str = "MAJ_PERF_UNPIPELINED";

/// One thread for a pool: IO's and async compute's under [`POOL`].
fn one() -> TaskPoolThreadAssignmentPolicy {
    TaskPoolThreadAssignmentPolicy {
        min_threads: 1,
        max_threads: 1,
        percent: 0.0,
        on_thread_spawn: None,
        on_thread_destroy: None,
    }
}

/// `plugins` with [`POOL`]'s pools and without the render thread under
/// [`UNPIPELINED`].
pub fn plugins(plugins: PluginGroupBuilder) -> PluginGroupBuilder {
    let pooled = match std::env::var(POOL)
        .ok()
        .and_then(|n| n.parse::<usize>().ok())
    {
        Some(n) => plugins.set(TaskPoolPlugin {
            task_pool_options: TaskPoolOptions {
                // Room for all three pools whatever the affinity mask.
                min_total_threads: n + 2,
                max_total_threads: n + 2,
                io: one(),
                async_compute: one(),
                compute: TaskPoolThreadAssignmentPolicy {
                    min_threads: n,
                    max_threads: n,
                    percent: 1.0,
                    on_thread_spawn: None,
                    on_thread_destroy: None,
                },
            },
        }),
        None => plugins,
    };

    if std::env::var_os(UNPIPELINED).is_some() {
        pooled.disable::<PipelinedRenderingPlugin>()
    } else {
        pooled
    }
}

/// The single-threaded executor on every schedule of the main and render
/// worlds, unless [`EXECUTOR`] asks for Bevy's. Set once every plugin is
/// built: before the render thread takes the render world
/// (`PipelinedRenderingPlugin`'s cleanup).
pub struct Executors;

impl Plugin for Executors {
    fn build(&self, _app: &mut App) {}

    fn finish(&self, app: &mut App) {
        let multi = std::env::var(EXECUTOR).is_ok_and(|word| word == MULTI);

        if !multi {
            single(app.world_mut());

            if let Some(render) = app.get_sub_app_mut(RenderApp) {
                single(render.world_mut());
            }
        }
    }
}

/// Each of `world`'s schedules given the single-threaded executor.
fn single(world: &mut World) {
    let mut schedules = world.resource_mut::<Schedules>();

    for (_, schedule) in schedules.iter_mut() {
        schedule.set_executor(SingleThreadedExecutor::new());
    }
}
