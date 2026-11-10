//! A hit point bar on a 2D camera, as the shown camera draws it
//! (`present`): its fill over its frame however the bar came into view.
//! Bevy's renderer on this machine's GPU, its own process: no window.
//!
//! ```text
//!  view pans left ──> the frame's right end in view first: queued, z 0.004
//!  a bar elsewhere goes ──> this bar's z 0.000, its frame's key kept
//!  the fill in view ──> queued at 0.003, under the frame's 0.004: a black bar
//! ```

use bevy::asset::RenderAssetUsages;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::core_2d::Transparent2d;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::render_phase::ViewSortedRenderPhases;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

/// The image the camera draws, pixels a side.
const SIDE: u32 = 256;

/// A building's bar, as `view::map::bar` places it: 60 long, its frame
/// one round it, the fill from the left.
const WIDTH: f32 = 60.0;

/// Its left end, left of the view at the start.
const LEFT: f32 = -200.0;

/// The fill, short: a building just begun.
const LONG: f32 = 6.0;

/// One bar's step nearer than the bar before it, and the fill's past
/// its frame (openregent's `hp_bars`).
const BAR_STEP: f32 = 0.004;
const FILL_Z: f32 = 0.003;

/// The pan each frame.
const PAN: f32 = 4.0;

/// An app drawing with Bevy's renderer into an image, a 2D camera on it;
/// the render world kept on the main thread, to read.
fn app() -> App {
    let mut app = App::new();

    app.add_plugins((
        DefaultPlugins
            .build()
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .disable::<bevy::audio::AudioPlugin>()
            .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            }),
        regent_draw::present::Rekeyed,
    ));

    while app.plugins_state() == bevy::app::PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }

    app.finish();
    app.cleanup();

    let mut image = Image::new_fill(
        Extent3d {
            width: SIDE,
            height: SIDE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );

    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;

    let handle = app.world_mut().resource_mut::<Assets<Image>>().add(image);

    app.world_mut().spawn((
        Camera2d,
        RenderTarget::Image(ImageRenderTarget {
            handle,
            scale_factor: 1.0,
        }),
    ));

    app
}

/// A part: a unit sprite scaled to `[x0, x1]`, 3 tall, at `z`.
fn part(colour: Color, [x0, x1]: [f32; 2], z: f32, bar: Entity) -> impl Bundle {
    (
        Sprite {
            color: colour,
            custom_size: Some(Vec2::ONE),
            ..default()
        },
        Transform {
            translation: Vec3::new(f32::midpoint(x0, x1), 0.0, z),
            scale: Vec3::new(x1 - x0, 3.0, 1.0),
            ..default()
        },
        ChildOf(bar),
    )
}

/// Where `entity`'s item stands in the sorted order, if queued.
fn place(app: &App, entity: Entity) -> Option<usize> {
    let phases = app
        .sub_app(RenderApp)
        .world()
        .resource::<ViewSortedRenderPhases<Transparent2d>>();

    phases
        .values()
        .find_map(|phase| phase.items.values().position(|i| i.entity.1.id() == entity))
}

/// The bar comes in from the left, its frame first; while only the frame
/// is in view a bar elsewhere goes, so this one's z drops a step, as
/// openregent ranks every bar on the map. The fill, in view later, draws
/// over the frame.
#[test]
fn a_fill_in_view_after_its_frame_and_a_rank_change_draws_over_it() {
    let mut app = app();
    let world = app.world_mut();
    let bar = world
        .spawn((
            Transform::from_xyz(0.0, 0.0, BAR_STEP),
            Visibility::Inherited,
        ))
        .id();
    let frame = world
        .spawn(part(
            Color::BLACK,
            [LEFT - 1.0, LEFT + WIDTH + 1.0],
            0.0,
            bar,
        ))
        .id();
    let fill = world
        .spawn(part(
            Color::srgb(0.0, 0.0, 1.0),
            [LEFT, LEFT + LONG],
            FILL_Z,
            bar,
        ))
        .id();
    let mut ranked = false;

    while place(&app, fill).is_none() {
        let mut camera = app
            .world_mut()
            .query_filtered::<&mut Transform, With<Camera>>()
            .single_mut(app.world_mut())
            .expect("a camera");

        camera.translation.x -= PAN;
        assert!(camera.translation.x > LEFT, "the fill never came in view");

        app.update();

        if !ranked && place(&app, frame).is_some() {
            ranked = true;
            app.world_mut()
                .get_mut::<Transform>(bar)
                .expect("the bar")
                .translation
                .z = 0.0;
        }
    }

    assert!(ranked, "the frame came in view first");

    app.update();

    assert!(place(&app, fill) > place(&app, frame));
}
