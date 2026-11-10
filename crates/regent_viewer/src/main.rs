//! A browser for Majesty's unit art and tileset.
//!
//!     regent_viewer <file.cam|dir>...
//!
//! Takes archives like `regent_cli units`. Not yet supported: dataset
//! definitions, whose load order would settle id collisions.

mod browser;
mod grid;
mod sprites;
mod tiles;
mod units;

use std::path::PathBuf;
use std::process::ExitCode;

use bevy::camera::CameraOutputMode;
use bevy::camera::visibility::RenderLayers;
use bevy::log::tracing_subscriber::{self, EnvFilter};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext};
use clap::Parser;

use regent_draw::data::Corpus;
use regent_draw::{material, shadow, stage, terrain};

use tiles::{Subject, Tiles};
use units::{Playback, Rows, Selection};

/// The workspace's crates, as tracing names their targets.
const OURS: [&str; 3] = ["regent_viewer", "regent_draw", "regent_core"];

#[derive(Parser)]
#[command(name = "regent_viewer", version, about, max_term_width = 100)]
struct Cli {
    /// Archives, or directories to search for them
    paths: Vec<String>,
}

#[allow(clippy::too_many_lines, reason = "the app's systems, listed")]
fn main() -> ExitCode {
    let cli = Cli::parse();

    logs();

    let files = match gather(&cli.paths) {
        Ok(files) if files.is_empty() => {
            error!("no .cam archives in those paths");
            return ExitCode::FAILURE;
        }
        Ok(found) => found,
        Err(e) => {
            error!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let corpus = match Corpus::open(&files) {
        Ok(corpus) => corpus,
        Err(e) => {
            error!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let rows = Rows::open(&corpus);

    // Also on the terminal, before the window opens.
    println!("{}", rows.summary(&corpus));
    for id in &rows.unreadable {
        warn!("unreadable unit {id}");
    }

    // Decode tiles once (about 3 MB), not on every painter edit.
    let ground = terrain::TileArt::open(&corpus);
    println!("{}", ground.summary());

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "OpenRegent viewer".to_owned(),
                        ..default()
                    }),
                    ..default()
                })
                // `logs` set the subscriber already.
                .disable::<bevy::log::LogPlugin>(),
        )
        .add_plugins(EguiPlugin::default())
        .add_plugins((material::Plugin, shadow::Plugin))
        .insert_resource(ground)
        .insert_resource(corpus)
        .insert_resource(rows)
        .init_state::<Subject>()
        .init_resource::<Selection>()
        .init_resource::<Playback>()
        .init_resource::<Tiles>()
        .init_resource::<sprites::StageShown>()
        .init_resource::<terrain::MapShown>()
        .init_resource::<stage::View>()
        .init_resource::<sprites::Legends>()
        .add_systems(Startup, cameras)
        // In `First`, so any drawing system can append without ordering.
        .add_systems(First, sprites::clear_legends)
        .add_systems(Update, (sprites::zoom, stage::pan))
        .add_systems(
            Update,
            (
                sprites::advance,
                sprites::follow_selection,
                sprites::points,
                sprites::anchor,
            )
                .chain()
                .run_if(in_state(Subject::Units)),
        )
        .add_systems(Update, tiles::browse.run_if(in_state(Subject::Tileset)))
        // After the subjects, so it frames content measured this frame.
        .add_systems(
            Update,
            (stage::fit, stage::aim)
                .chain()
                .after(tiles::browse)
                .after(sprites::follow_selection),
        )
        .add_systems(OnEnter(Subject::Units), browse)
        .add_systems(OnEnter(Subject::Tileset), refit)
        .add_systems(OnExit(Subject::Units), clear_stage)
        .add_systems(OnExit(Subject::Tileset), clear_terrain)
        .add_systems(EguiPrimaryContextPass, browser::panels)
        .run();

    ExitCode::SUCCESS
}

/// The terminal's log, set before the window so the loading's warnings
/// show: `RUST_LOG` when set, else Bevy's default (`LogPlugin`'s) with our
/// crates at warnings and up.
fn logs() {
    let ours = OURS.map(|c| format!("{c}=warn")).join(",");
    let default = format!("info,{}{ours}", bevy::log::DEFAULT_FILTER);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

/// Ask for a reframe: the last subject's magnification does not carry over.
fn refit(mut view: ResMut<stage::View>) {
    view.refit = true;
}

/// The unit browser opens at [`stage::BROWSE_ZOOM`], so units share a scale.
fn browse(mut view: ResMut<stage::View>) {
    view.zoom = stage::BROWSE_ZOOM;
    view.pan = Vec2::ZERO;
    view.refit = false;
}

/// Despawn the unit stage on leaving it.
fn clear_stage(
    mut commands: Commands,
    mut shown: ResMut<sprites::StageShown>,
    entities: Query<Entity, With<sprites::Stage>>,
) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }

    shown.forget();
}

/// Despawn the terrain views and reset their state.
fn clear_terrain(
    mut commands: Commands,
    mut shown: ResMut<terrain::MapShown>,
    mut tiles: ResMut<Tiles>,
    entities: Query<Entity, With<terrain::Drawn>>,
) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }

    shown.forget();
    tiles.forget();
}

/// Two cameras, so the panels shrink the picture rather than cover it.
///
/// The world camera's viewport is clipped to the space the panels leave. egui
/// gets its own camera, composited on top. Pattern from `bevy_egui`'s
/// `side_panel` example.
fn cameras(mut commands: Commands, mut egui_settings: ResMut<EguiGlobalSettings>) {
    // Else bevy_egui attaches to the clipped world camera.
    egui_settings.auto_create_primary_context = false;

    // MSAA off: the shadow pass stores a ramp index in alpha, which
    // multisampling would average. Off on both cameras: they share a colour
    // target, and mismatched sample counts fail validation.
    // The world's is 3D (`regent_draw::frame`).
    commands.spawn((stage::eye(), stage::WorldCamera, shadow::Resolve));

    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        Msaa::Off,
        // Panels only.
        RenderLayers::none(),
        Camera {
            order: 1,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
}

/// Every `.cam` under the given paths. Directories are walked; a named
/// file is an archive, whatever its extension.
///
/// Copy of `regent_cli`'s `gather_all`: keeps filesystem policy out of the library.
fn gather(paths: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();

    for path in paths.iter().map(PathBuf::from) {
        if path.is_dir() {
            walk(&path, &mut found)?;
        } else {
            found.push(path);
        }
    }

    found.sort();

    Ok(found)
}

fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;

    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            walk(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("cam"))
        {
            out.push(path);
        }
    }

    Ok(())
}
