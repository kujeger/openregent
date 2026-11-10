//! The app: built ([`menu`]), a game put in it and taken out
//! ([`install`], [`uninstall`]), the opening view framed.
//!
//! ```text
//!  base ──> plugins, cameras, every game system (frame)
//!  install    a made game's resources ──> Screen::Playing
//!  uninstall  its entities gone, its corpus and art onto the shelf
//!  frame_start  once: on the palace, the map as bounds
//! ```
//!
//! The order the game's systems run in is `frame`'s; what lives for one
//! game is each module's (`scope`). The clock is `game::clock`'s, the
//! speed `game::speed`'s, the pieces' sprites `map::pieces`'.

pub(crate) mod frame;
pub(crate) mod fullscreen;
pub(crate) mod resynced;
pub(crate) mod shelf;
pub(crate) mod shot;

use std::path::PathBuf;
use std::sync::Arc;

use bevy::camera::CameraOutputMode;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::resource::IsResource;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, PrimaryEguiContext};

#[cfg(test)]
use regent_core::sim::game::Input;
#[cfg(test)]
use regent_core::sim::world::cheat::{Cheat, CheatCode};
use regent_draw::data::Corpus;
use regent_draw::map::load::Loaded;
use regent_draw::map::reel::Reel;
use regent_draw::map::{Maps, SCATTER};
use regent_draw::present::{self, ViewUpscale};
use regent_draw::{material, shadow, stage, terrain};

use crate::audio::Sound;
use crate::draw::shelf::{Shelf, Way};
use crate::draw::shot::Capture;
use crate::files::config::Settings;
use crate::game;
use crate::game::begin::{Made, Maker};
use crate::game::clock::Snapshot;
use crate::game::scope::{Leave, Scoped};
use crate::game::seen::Watch;
use crate::game::sim::Session;
use crate::game::speed::Speedup;
use crate::game::thread::SimThread;
use crate::map::mirror::Pieces;
use crate::map::pieces::{Lifts, Saved, Shots, cell};
use crate::menu::Screen;
use crate::{
    input, input::orders, map::cursor, map::effects, map::fog, map::ground, map::particles,
    map::preview, map::select, map::trail, map::zoom, ui, ui::chat,
};
use regent_draw::art::{self, Sprites};

/// The art disk cache's directory in [`crate::files::env::OWN`] under the XDG
/// cache directory.
const ART_CACHE: &str = "upscale";

/// Which of [`map::CHUNKS`](regent_draw::map::CHUNKS) the map draws from its source: scatter only.
/// The game draws buildings and mobiles from its world.
const FROM_SAVE: [bool; 4] = {
    let mut drawn = [false; 4];

    drawn[SCATTER] = true;
    drawn
};

/// Where the app draws.
#[derive(Clone, Copy)]
pub enum Display {
    /// A window and the GPU; sound as asked ([`Sound`]).
    Window,
    /// No window, GPU or sound: every system still runs (the app test).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "only the app test runs headless")
    )]
    Headless,
}

/// Open the window on the menus, or on the command line's game where
/// `menu` asks for it; each game made by `maker`. Run until it closes, or
/// with `shot` until it is captured: the game's opening view, else the
/// page.
///
/// ```text
///  base ──> menus, Maker, Shelf, Player ──> Screen::Menu
///    the command line's game: its loading page, begin ──> Screen::Playing
/// ```
///
/// # Errors
///
/// An app that exited with an error: a game the command line asked for
/// that would not start, a capture that failed.
pub fn menu(
    opened: impl FnOnce() -> Result<(crate::menu::Opened, crate::files::profile::Player), String>,
    maker: Maker,
    (speed, clock): (Option<f32>, chat::Clock),
    (settings, shot): (Settings, Option<crate::draw::shot::Shot>),
) -> Result<(), String> {
    // A screenshot is muted: no device.
    let sound = if shot.is_some() {
        Sound::Muted
    } else {
        Sound::Played
    };
    let mut app = base(speed, settings, (Display::Window, sound));
    // The menus, read meanwhile (`main::menus`).
    let (menu, player) = opened()?;
    let game = menu.commanded();
    let lore = crate::files::lore::Catalogue::read(&maker.install().hd)?;

    menus(&mut app, menu, (maker, player));
    app.insert_resource(lore);

    app.insert_resource(clock)
        .insert_resource(crate::perf::mode(Capture::of(shot.as_ref())));

    // A won game's high scores are written, but not from a screenshot.
    app.insert_resource(crate::files::scores::Scoring {
        place: shot.is_none().then(crate::files::scores::place).flatten(),
        scored: false,
    });

    match shot {
        Some(shot) if game => crate::draw::shot::install(&mut app, shot),
        Some(shot) => crate::draw::shot::menus(&mut app, shot),
        None => {}
    }

    exited(&app.run())
}

/// The app on the menus, built, not run: a test's.
#[cfg(test)]
fn menu_app(
    menu: crate::menu::Opened,
    (maker, player): (Maker, crate::files::profile::Player),
    speed: f32,
    settings: Settings,
    display: Display,
) -> App {
    let mut app = base(Some(speed), settings, (display, Sound::Muted));

    menus(&mut app, menu, (maker, player));
    app.insert_resource(game::thread::Mode::Sync);
    app
}

/// `menu` and its makers in `app`, opened on the menus.
fn menus(
    app: &mut App,
    menu: crate::menu::Opened,
    (maker, player): (Maker, crate::files::profile::Player),
) {
    let art = regent_draw::art::Art::pinned(menu.sources())
        .cached(art_cache())
        .overridden(art_overrides());
    let sprites = Sprites::pinned(menu.sources());

    menu.insert(app.world_mut());
    app.add_plugins((crate::menu::Plugin, crate::video::Plugin))
        .insert_resource(art)
        .insert_resource(sprites)
        .insert_resource(maker.install().clone())
        .insert_resource(maker)
        .init_resource::<Shelf>()
        .insert_resource(player)
        .init_resource::<crate::files::scores::Scoring>()
        .add_systems(OnExit(Screen::Menu), keep)
        .add_systems(OnExit(Screen::Playing), uninstall.in_set(Leave::Take))
        .insert_state(Screen::Menu);
}

/// An app's exit as a result.
fn exited(exit: &AppExit) -> Result<(), String> {
    match *exit {
        AppExit::Success => Ok(()),
        AppExit::Error(code) => Err(format!("exited with {code}")),
    }
}

/// wgpu's experimental features, never used here and not asked of the
/// device: Bevy asks every feature the adapter lists, and lavapipe
/// (Mesa's software Vulkan) lists cooperative matrices, then refuses the
/// device ("Requested feature is not available on this device").
///
/// The whole mask, so a new wgpu's experimental features are refused too.
const UNUSED_FEATURES: bevy::render::settings::WgpuFeatures =
    bevy::render::settings::WgpuFeatures::all_experimental_mask();

/// The window's graphics backends: `WGPU_BACKEND` if set, else Vulkan where
/// it has a GPU, else all of them for Bevy to choose from (DX12, Metal).
/// A software Vulkan alone falls through, as a real GPU may be on another.
fn backends() -> bevy::render::settings::Backends {
    use bevy::render::settings::Backends;

    Backends::from_env().unwrap_or_else(|| {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapters = bevy::tasks::block_on(instance.enumerate_adapters(Backends::VULKAN));

        if adapters
            .iter()
            .any(|a| a.get_info().device_type != wgpu::DeviceType::Cpu)
        {
            Backends::VULKAN
        } else {
            Backends::all()
        }
    })
}

/// Bevy's plugins for `display` and `sound`, the window opening in `mode`.
fn defaults(
    (display, sound): (Display, Sound),
    mode: crate::files::config::WindowMode,
) -> bevy::app::PluginGroupBuilder {
    let plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: "OpenRegent".to_owned(),
                present_mode: crate::perf::present_mode(),
                mode: crate::draw::fullscreen::mode(mode),
                ..default()
            }),
            ..default()
        })
        // `log::start` set the subscriber already.
        .disable::<bevy::log::LogPlugin>()
        // Unused: Bevy 0.20's `custom_cursor` feature pulls picking in.
        .disable::<bevy::picking::input::PointerInputPlugin>()
        .disable::<bevy::picking::PickingPlugin>()
        .disable::<bevy::picking::InteractionPlugin>();
    let window = match sound {
        Sound::Played => plugins,
        Sound::Muted => plugins.disable::<bevy::audio::AudioPlugin>(),
    };

    match display {
        Display::Window => window.set(bevy::render::RenderPlugin {
            render_creation: bevy::render::settings::WgpuSettings {
                backends: Some(backends()),
                disabled_features: Some(UNUSED_FEATURES),
                // GPU memory taken in blocks of 8 to 64 MB, not 128 to 256
                // (wgpu's `Performance`): a 512-cell game's 145 MB held 448
                // MB, live textures and meshes left scattered across blocks.
                memory_hints: bevy::render::settings::MemoryHints::MemoryUsage,
                ..default()
            }
            .into(),
            ..default()
        }),
        Display::Headless => {
            window
                .disable::<bevy::winit::WinitPlugin>()
                .set(bevy::render::RenderPlugin {
                    render_creation: bevy::render::settings::WgpuSettings {
                        backends: None,
                        ..default()
                    }
                    .into(),
                    ..default()
                })
        }
    }
}

/// Put a made game in `world`: what the game's systems read of it. Its
/// systems run once the state is [`Screen::Playing`].
///
/// # Errors
///
/// A world with no map.
pub fn install(world: &mut World, made: Made) -> Result<(), String> {
    let Made {
        corpus,
        sim,
        session,
        art,
    } = made;
    if !world.contains_resource::<crate::files::lore::Catalogue>() {
        let lore = world
            .get_resource::<crate::files::env::Install>()
            .map(|install| crate::files::lore::Catalogue::read(&install.hd))
            .transpose()?
            .unwrap_or_default();
        world.insert_resource(lore);
    }
    let paths = corpus.paths().to_vec();
    let ground = world
        .get_resource_mut::<Shelf>()
        .and_then(|mut shelf| shelf.take(&paths))
        .unwrap_or_else(|| terrain::TileArt::open(&corpus));
    // The world's map, drawn once: the world redraws it (`ground`).
    let art = art.unwrap_or_else(|| regent_core::sim::mapgen::custom::Art::of(&corpus.catalogue));
    let (source, laid) =
        ground::Laid::drawn(sim.world(), art).ok_or("game: the world has no map")?;
    let saved = Saved::of(sim.world());
    let resumed = session.resumed.as_ref();
    let mut maps = Maps::with(vec![Loaded::drawn("game".to_owned(), source)]);
    // A new game starts with the local player's palace selected.
    let start = || sim.world().selection(sim.local());
    let selected = select::Selected(resumed.map_or_else(start, |r| r.interface.selected));
    let reading = select::Reading(resumed.and_then(|r| r.interface.reading));
    // The build menu's data: the preview's run on the sim thread needs it.
    let menu = Arc::new(regent_core::sim::world::offer::Menu::of(&corpus.catalogue));
    let clock = world
        .get_resource::<game::thread::Mode>()
        .copied()
        .unwrap_or(game::thread::Mode::Sync);
    let (thread, frame) = SimThread::spawn(sim, menu.clone(), clock)?;

    maps.objects = FROM_SAVE;

    world.insert_resource(ground);
    world.insert_resource(corpus);
    world.insert_resource(maps);
    world.insert_resource(saved);
    world.insert_resource(laid);
    world.insert_resource(orders::Rules(menu));
    world.insert_resource(frame);
    world.insert_resource(thread);
    world.insert_resource(session);
    world.insert_resource(selected);
    world.insert_resource(reading);
    world.insert_resource(ui::Paused(false));

    // The art the last game on this dataset made, in place of none.
    shelf::shift(world, &paths, Way::On);

    Ok(())
}

/// The entities standing as the menus make way for a game: what
/// [`uninstall`] keeps.
#[derive(Resource, Default)]
struct Kept(std::collections::HashSet<Entity>);

/// Note what stands before a game: cameras, windows, egui's.
fn keep(world: &mut World) {
    let standing = world
        .query_filtered::<Entity, Without<IsResource>>()
        .iter(world)
        .collect();

    world.insert_resource(Kept(standing));
}

/// Take the finished game's entities out of `world`, for the menus and
/// the next game: every entity made since [`keep`]; its dataset's corpus,
/// ground and art onto the shelf. Its resources go next
/// (`scope::Leave::Reset`): the sim thread, joined as its `SimThread` drops,
/// frees the `Sim`, its `World`, VM and recorder.
///
/// ```text
///  entities   all but Kept: the map, pieces, effects, sounds
///  shelf      the corpus, ground and art, for its dataset (shelf)
/// ```
pub fn uninstall(world: &mut World) {
    let kept = world.remove_resource::<Kept>().unwrap_or_default().0;
    // Resources are entities too: those are put back below, not despawned.
    let made: std::collections::HashSet<Entity> = world
        .query_filtered::<Entity, Without<IsResource>>()
        .iter(world)
        .filter(|e| !kept.contains(e))
        .collect();

    // Each made entity whose parent was not made: its children go with it.
    let roots: Vec<Entity> = world
        .query::<(Entity, Option<&ChildOf>)>()
        .iter(world)
        .filter(|(e, parent)| {
            made.contains(e) && parent.is_none_or(|p| !made.contains(&p.parent()))
        })
        .map(|(e, _)| e)
        .collect();

    for entity in roots {
        let _ = world.try_despawn(entity);
    }

    // The corpus and its art stay for the next game on the dataset.
    if let Some(paths) = world.get_resource::<Corpus>().map(|c| c.paths().to_vec()) {
        shelf::shift(world, &paths, Way::Off);
        shelf::shelve::<terrain::TileArt>(world, &paths);
        shelf::shelve::<Corpus>(world, &paths);
    }
}

/// The drawing's resources, a game's: the view and what is drawn; the
/// game's thread, its corpus, map and save's records, which it is made
/// with ([`install`]).
fn plugin(app: &mut App) {
    app.per_game_as(|| stage::View {
        zoom: START_ZOOM,
        ..default()
    })
    .per_game::<Framed>()
    .per_game::<Reel>()
    .per_game::<terrain::MapShown>()
    .per_game::<Snapshot>()
    .per_game::<Lifts>()
    .per_game::<Shots>()
    .made_by_game::<SimThread>()
    .made_by_game::<Session>()
    .made_by_game::<terrain::TileArt>()
    .made_by_game::<Corpus>()
    .made_by_game::<Maps>()
    .made_by_game::<Saved>();
}

/// The art service's disk cache ([`ART_CACHE`]); `None` with no cache
/// directory. Tests' beside the test binary, not the user's.
fn art_cache() -> Option<PathBuf> {
    if cfg!(test) {
        std::env::current_exe()
            .ok()
            .map(|e| e.with_file_name("art-cache"))
    } else {
        dirs::cache_dir().map(|d| d.join(crate::files::env::OWN).join(ART_CACHE))
    }
}

/// The art override folder (`art::over`): PNGs the user drops in to
/// replace interface pictures; `None` with no data directory. Tests have
/// none: the user's files would change their pictures.
fn art_overrides() -> Option<PathBuf> {
    if cfg!(test) {
        None
    } else {
        dirs::data_dir().map(|d| d.join(crate::files::env::OWN).join(art::over::DIR))
    }
}

/// The user's model folder (`regent_draw::sun::model`): glTF or OBJ files
/// named by a unit's code, adding or replacing shadow proxies; `None`
/// with no data directory, and in tests.
#[cfg_attr(test, allow(dead_code, reason = "tests draw a fixture"))]
fn model_overrides() -> Option<PathBuf> {
    if cfg!(test) {
        None
    } else {
        dirs::data_dir().map(|d| {
            d.join(crate::files::env::OWN)
                .join(regent_draw::sun::model::DIR)
        })
    }
}

/// The art override folder scanned at start.
fn overrides(mut art: ResMut<art::Art>) {
    art.rescan();
}

/// The art override folder scanned again as the settings editor closes,
/// on the menus or in a game: a user swapping files has a way to show
/// them.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn rescan(editor: Res<crate::ui::editor::Editor>, mut art: ResMut<art::Art>, mut was: Local<bool>) {
    let open = editor.is_open();

    if *was && !open {
        art.rescan();
    }

    *was = open;
}

/// The app without a game: plugins, the shared state, and every game
/// system, held until [`Screen::Playing`] (`frame`). Each module's plugin
/// registers the state it keeps for one game (`scope`).
fn base(speed: Option<f32>, settings: Settings, output: (Display, Sound)) -> App {
    let mut app = App::new();
    let starts = speed.unwrap_or_else(|| settings.game.speed.factor());

    // The shadow proxies, the user's folder over the shipped ones: before
    // the plugins, which size their textures by the count at start.
    // Tests draw the guardhouse's model, none shipped.
    #[cfg(test)]
    app.insert_resource(crate::testdata::proxies());
    #[cfg(not(test))]
    app.insert_resource(regent_draw::sun::model::Proxies::load(
        model_overrides().as_deref(),
    ));

    app.add_plugins(defaults(output, settings.ui.window))
        .add_plugins((EguiPlugin::default(), crate::ui::typeface::TypefacePlugin))
        .add_plugins((art::side::Plugin, art::gpu::Plugin, art::fsr::Plugin))
        .add_plugins((
            material::Plugin,
            regent_draw::ring::Plugin,
            shadow::Plugin,
            regent_draw::sun::Plugin,
            present::Plugin,
            regent_draw::smooth::Plugin,
            regent_draw::veil::Plugin,
            crate::audio::Plugin,
        ))
        .insert_resource(settings)
        .insert_resource(crate::files::config::trees())
        .init_resource::<crate::ui::editor::Editor>()
        .insert_resource(
            regent_draw::art::Art::default()
                .cached(art_cache())
                .overridden(art_overrides()),
        )
        .init_resource::<Sprites>()
        .insert_resource(Speedup(starts))
        .insert_resource(crate::game::speed::Given(speed))
        .add_plugins((
            crate::game::scope::plugin,
            crate::draw::frame::plugin,
            crate::game::seen::plugin,
            crate::map::mirror::plugin,
            plugin,
        ))
        .add_plugins((
            (
                effects::plugin,
                fog::Plugin,
                cursor::plugin,
                crate::map::words::Plugin,
            ),
            (crate::ui::end::plugin, orders::plugin),
            crate::ui::lore::plugin,
            (
                trail::plugin,
                particles::plugin,
                select::plugin,
                input::plugin,
            ),
            (
                crate::ui::minimap::plugin,
                crate::map::hp_bars::plugin,
                crate::map::labels::plugin,
            ),
            (
                crate::panel::plugin,
                ui::plugin,
                preview::plugin,
                crate::map::reach::plugin,
            ),
            (
                zoom::plugin,
                crate::map::quake::plugin,
                crate::ui::frame_rate::plugin,
                chat::plugin,
            ),
            (
                ground::plugin,
                crate::save::dialog::plugin,
                crate::input::map_keys::plugin,
                crate::map::pan::plugin,
            ),
            (
                crate::ui::tracking::plugin,
                crate::draw::resynced::plugin,
                crate::game::speed::plugin,
                crate::draw::fullscreen::plugin,
            ),
            regent_draw::cull::plugin,
        ))
        .add_systems(Startup, (cameras, overrides))
        .add_systems(Update, rescan)
        .add_systems(
            PostUpdate,
            (
                upscaling.before(present::Present),
                shadowing,
                sunning,
                winding,
                world_shown.before(present::Present),
            ),
        );

    crate::perf::install(&mut app);
    app
}

/// The view's upscale: the settings' in a game; nearest on the menus,
/// which draw no world.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn upscaling(
    settings: Res<Settings>,
    screen: Option<Res<State<Screen>>>,
    mut mode: ResMut<ViewUpscale>,
) {
    let wanted = if crate::menu::in_game(screen) {
        settings.enhancements.view_upscale
    } else {
        ViewUpscale::Nearest
    };

    mode.set_if_neq(wanted);
}

/// The world camera on only in a game: on the menus its passes (the
/// shadow resolve over the whole window, its copy to the window) drew
/// nothing egui does not cover. Without it egui's camera is the first to
/// write the window, so it clears it first.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn world_shown(
    screen: Option<Res<State<Screen>>>,
    world: Single<&mut Camera, With<stage::WorldCamera>>,
    egui: Single<&mut Camera, (With<PrimaryEguiContext>, Without<stage::WorldCamera>)>,
) {
    let screen = screen.map_or(Screen::Menu, |s| *s.get());
    let shown = screen == Screen::Playing;
    let (mut world, mut egui) = (world.into_inner(), egui.into_inner());
    let cleared = matches!(
        egui.output_mode,
        CameraOutputMode::Write {
            clear_color: ClearColorConfig::Default,
            ..
        }
    );

    if world.is_active != shown {
        world.is_active = shown;
    }

    if cleared == shown {
        egui.output_mode = egui_output(screen);
    }
}

/// How egui's camera writes the window: over the world camera's picture
/// in a game, else over the clear colour.
const fn egui_output(screen: Screen) -> CameraOutputMode {
    CameraOutputMode::Write {
        blend_state: Some(BlendState::ALPHA_BLENDING),
        clear_color: match screen {
            Screen::Playing => ClearColorConfig::None,
            Screen::Menu => ClearColorConfig::Default,
        },
    }
}

/// The settings' shadows, for `regent_draw::shadow`.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn shadowing(settings: Res<Settings>, mut shadows: ResMut<regent_draw::shadow::Shadows>) {
    shadows.set_if_neq(settings.enhancements.shadows);
}

/// The settings' cast, sun and self-shadows, for `regent_draw::sun`:
/// painted under blended shadows.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn sunning(settings: Res<Settings>, mut sun: ResMut<regent_draw::sun::Sun>) {
    let e = &settings.enhancements;

    // The models' shadows are coverage, which only soft shadows resolve.
    let cast = match e.shadows {
        regent_draw::shadow::Shadows::Soft => e.cast_now(),
        regent_draw::shadow::Shadows::Blend => regent_draw::sun::Cast::Painted,
    };

    sun.set_if_neq(regent_draw::sun::Sun {
        cast,
        light: regent_draw::sun::cast::Light {
            direction: e.sun_direction,
            elevation: e.sun_elevation,
            size: e.sun_size,
        },
        own: if e.self_shadows_now() {
            regent_draw::sun::Own::Cast
        } else {
            regent_draw::sun::Own::Painted
        },
    });
}

/// The settings' wind, for `regent_draw::wind`: its lean, 0 when off.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn winding(settings: Res<Settings>, mut wind: ResMut<regent_draw::wind::Wind>) {
    let reach = settings.enhancements.wind_now();

    wind.set_if_neq(regent_draw::wind::Wind { reach, ..*wind });
}

/// Two cameras, as the viewer's: the world's, clipped to the room the bar
/// leaves, and egui's on top, over `present`'s too.
fn cameras(mut commands: Commands, mut egui: ResMut<EguiGlobalSettings>) {
    // Else bevy_egui attaches to the clipped world camera.
    egui.auto_create_primary_context = false;

    // The game's cursor everywhere (`cursor`), egui's never.
    egui.enable_cursor_icon_updates = false;

    // MSAA off on both: the shadow pass keeps a ramp index in alpha, and
    // the two share a colour target. The world's is 3D (`regent_draw::frame`).
    commands.spawn((stage::eye(), stage::WorldCamera, shadow::Resolve));
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        Msaa::Off,
        RenderLayers::none(),
        Camera {
            order: present::TOP_ORDER,
            output_mode: egui_output(Screen::Playing),
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
}

/// The engine's regular zoom: a world unit, an art pixel, per logical
/// point (the viewer's "1:1", `stage::STEPS`).
const START_ZOOM: f32 = regent_core::view::map::zoom::REGULAR;

/// Whether the opening view is set: once the map is drawn.
#[derive(Resource, Default)]
pub(crate) struct Framed(pub(crate) bool);

/// The opening view, once the map's extent is known: the map bounds the
/// view ([`stage::bound`], as `IsoGameView` holds it), and
/// the view centres on the player's palace, else its outpost
/// (`World::player_palace`, new game setup), else fits the whole
/// map. A palace waits for its mirror.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub(crate) fn frame_start(
    mut framed: ResMut<Framed>,
    mut view: ResMut<stage::View>,
    maps: Res<Maps>,
    mirrors: Pieces,
    settings: Res<Settings>,
    (seen, session): (Watch, Res<Session>),
) {
    let saved = session.resumed.as_ref().map(|r| &r.interface);
    let agent = seen.world().player_palace(seen.local);
    let palace = agent.and_then(|agent| mirrors.get(agent));
    // Pipelined, the first drawn frames may come before the sim thread's
    // first state: the palace not yet mirrored, no reason to fit the map.
    let waiting = saved.is_none() && agent.is_some() && palace.is_none();

    if framed.0 || view.content == Vec2::ZERO || waiting {
        return;
    }

    view.bounds = Some(view.content);

    // A saved game looks where it looked. Fitting picks a zoom of its
    // own: not one of the two levels.
    match (saved, palace, zoom::Mode::of(&settings)) {
        (Some(s), ..) => {
            view.pan = Vec2::new(s.pan.0, s.pan.1);
            view.zoom = s.zoom;
        }
        (None, Some(p), _) => view.pan = maps.standing_at(cell(p.at, None, 0.0)),
        (None, None, zoom::Mode::Smooth) => view.refit = true,
        (None, None, zoom::Mode::Levels) => {}
    }

    framed.0 = true;
}

#[cfg(test)]
mod lifecycle;

#[cfg(test)]
mod tests {
    use super::*;

    /// The refused features are wgpu's `EXPERIMENTAL_*` ones: all of them
    /// (lavapipe's cooperative matrix among them), and nothing else.
    #[test]
    fn unused_features_are_the_experimental_ones() {
        use bevy::render::settings::WgpuFeatures as F;

        let experimental = F::all()
            .iter_names()
            .filter(|(name, _)| name.starts_with("EXPERIMENTAL_"))
            .fold(F::empty(), |all, (_, flag)| all | flag);

        assert_eq!(UNUSED_FEATURES, experimental);
        assert!(UNUSED_FEATURES.contains(F::EXPERIMENTAL_COOPERATIVE_MATRIX));
    }
}
