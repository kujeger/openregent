//! The tracking window: mini view 1, the picture under the page, drawn
//! by a second camera into an image and shown in the column. What it
//! follows is the library's ([`regent_core::view::panel::tracking`]).
//!
//! ```text
//!  Frame::Step  Mini on the published world: a new game's palace, a
//!               save's state; new heroes and message flags, Autoscan,
//!               the follow step (an unseen piece deselected)
//!  Frame::Pick  the pointer on the picture is on the map, through the
//!               window: every mode's click and cursor as the main view's
//!               (select::Pointing; cursor view)
//!  Frame::View  the camera on the followed piece or message flag's
//!               drawing, a building's exit, or where it stopped;
//!               Inset: its ground's rect
//!  the camera   layers 0 (the world), OVERLAY (bars), INSET_LAYER (its
//!               ground past the main sheet), LIT_LAYER (the models'
//!               sun, its own cascade: sun::fitted); one texel an art pixel,
//!               shadows resolved into the image, as the world image is;
//!               every EVERY drawn frames
//!  the column   the image in APMK 2002's rect, bilinear: 0.5 of the art
//!               times the column's scale, whatever view_upscale says;
//!               then names and amounts by words::shown_at at 0.5
//! ```

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy_egui::egui::{self, LayerId};
use bevy_egui::{EguiContexts, EguiTextureHandle, EguiUserTextures};

use regent_core::sim::units;
use regent_core::view::panel::tracking::{self, Focus, Follows, Jump, Live, Step, Target, Tracker};
use regent_draw::map::Maps;
use regent_draw::map::redraw::{INSET_LAYER, Inset};
use regent_draw::present::OVERLAY;
use regent_draw::shadow;
use regent_draw::stage::View;
use regent_draw::sun;

use crate::files::config::Settings;
use crate::game::seen::Watch;
use crate::game::sim::Session;
use crate::map::effects::Hung;
use crate::map::mirror::Pieces;
use crate::map::select::Selected;
use crate::map::words::{self, Chain, Words};

/// The picture's rect in the column, `APMK` 2002 at 1920 × 1200.
pub const PICTURE: egui::Rect = egui::Rect {
    min: egui::pos2(6.0, 676.0),
    max: egui::pos2(196.0, 802.0),
};

/// The image's texels a side: the picture's art pixels at the window's
/// zoom ([`tracking::ZOOM`]).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a few hundred texels"
)]
const IMAGE: UVec2 = UVec2::new(
    ((PICTURE.max.x - PICTURE.min.x) / tracking::ZOOM) as u32,
    ((PICTURE.max.y - PICTURE.min.y) / tracking::ZOOM) as u32,
);

/// Drawn before the world camera: its image is egui's by then.
const ORDER: isize = -1;

/// The camera draws every this many drawn frames: half the cost of a
/// second view of the world, at half the main view's rate (a cosmetic
/// departure: the original redraws it with every frame).
const EVERY: u32 = 2;

/// Where the camera snaps, a texel past whole: texel middles off the art
/// pixels' edges (as `regent_draw::present`'s `SNAP`).
const SNAP: f32 = 0.75;

/// The camera that draws the window.
#[derive(Component)]
pub struct Camera;

/// The image the camera draws, and egui's id for it.
#[derive(Resource)]
pub struct Picture {
    image: Handle<Image>,
    id: Option<egui::TextureId>,
}

/// Where the camera looks this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Aim {
    /// Where the window's middle is: it follows nothing.
    #[default]
    Middle,
    Focus(Focus),
    /// Where it was: its piece is unseen.
    Hold,
}

/// The window's state, a game's.
#[derive(Resource, Default)]
pub struct Tracking {
    /// `None` until the game's first frame.
    pub mini: Option<Tracker>,
    aim: Aim,
    /// Where the window looks, world units.
    looked: Option<Vec2>,
    /// Where its camera is: `looked` snapped ([`SNAP`]).
    camera: Option<Vec2>,
    /// Where the picture drew, logical points.
    rect: Option<egui::Rect>,
    /// Whether the pointer was on it in egui's last pass, over nothing
    /// else: cleared as the camera is aimed, set again as it is shown.
    hovered: bool,
    /// The highest agent and the message flags seen: what is new.
    highest: Option<u32>,
    messages: Vec<u32>,
    /// The caption, made for a game frame and a target.
    named: Option<((u32, Follows), Option<String>)>,
}

impl Tracking {
    /// `world`'s pieces and message flags taken as standing, not made
    /// since (a LAN resync's world); the caption made again.
    pub fn anew(&mut self, world: &regent_core::sim::world::World) {
        self.highest = world.shown().iter().map(|s| s.agent).max();
        self.messages = world.messages().iter().map(|m| m.handle).collect();
        self.named = None;
    }

    /// Where the picture looks, world units, and its texels: its
    /// frame on the minimap.
    pub fn frame(&self) -> Option<(Vec2, Vec2)> {
        self.looked.map(|at| (at, IMAGE.as_vec2()))
    }

    /// The world point under `at`, logical points on the picture.
    fn world_at(&self, at: egui::Pos2) -> Option<Vec2> {
        let (rect, camera) = (self.rect?, self.camera?);
        let per = points_per_texel(rect);
        let from = at - rect.center();

        Some(camera + Vec2::new(from.x, -from.y) / per)
    }

    /// A window shown in `rect`, its camera at `camera`, world units, one
    /// a texel, the pointer on it: a test's.
    #[cfg(test)]
    pub fn shown(rect: egui::Rect, camera: Vec2) -> Tracking {
        Tracking {
            camera: Some(camera),
            rect: Some(rect),
            hovered: true,
            ..Tracking::default()
        }
    }

    /// The world point the pointer at `at`, window points, is on through
    /// the picture, as the main view draws it; `None` off the picture or
    /// with something over it.
    pub fn pointed(&self, at: Vec2) -> Option<Vec2> {
        let at = egui::pos2(at.x, at.y);

        self.rect
            .filter(|r| self.hovered && r.contains(at))
            .and_then(|_| self.world_at(at))
    }
}

/// Logical points a texel of the image in `rect`.
#[allow(clippy::cast_precision_loss, reason = "a few hundred texels")]
fn points_per_texel(rect: egui::Rect) -> f32 {
    rect.width() / IMAGE.x as f32
}

/// The window's image and camera, idle until a game.
fn spawn(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let mut image = Image::new_uninit(
        Extent3d {
            width: IMAGE.x,
            height: IMAGE.y,
            ..default()
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );

    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    // Bilinear: the column shows it at 0.5 times its scale.
    image.sampler = ImageSampler::linear();

    let image = images.add(image);

    commands.spawn((
        Camera,
        regent_draw::stage::eye(),
        shadow::Resolve,
        bevy::camera::Camera {
            order: ORDER,
            is_active: false,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        RenderTarget::Image(ImageRenderTarget {
            handle: image.clone(),
            scale_factor: 1.0,
        }),
        RenderLayers::from_layers(&[0, OVERLAY, INSET_LAYER, sun::LIT_LAYER]),
    ));
    commands.insert_resource(Picture { image, id: None });
}

/// egui's id for the image, once its user textures exist.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn lend(mut picture: ResMut<Picture>, textures: Option<ResMut<EguiUserTextures>>) {
    if let (None, Some(mut textures)) = (picture.id, textures) {
        picture.id = Some(textures.add_image(EguiTextureHandle::Strong(picture.image.clone())));
    }
}

/// The camera on in a game alone, every [`EVERY`]th drawn frame: the
/// image keeps the last picture between.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn shown(
    screen: Option<Res<State<crate::menu::Screen>>>,
    frames: Res<bevy::diagnostic::FrameCount>,
    camera: Single<&mut bevy::camera::Camera, With<Camera>>,
) {
    let playing = screen.is_some_and(|s| *s.get() == crate::menu::Screen::Playing);
    let on = playing && frames.0.is_multiple_of(EVERY);
    let mut camera = camera.into_inner();

    if camera.is_active != on {
        camera.is_active = on;
    }
}

/// What the window follows, brought up to the published world: started
/// on its first frame; then new heroes and message flags, Autoscan, and
/// the follow step. An unseen followed piece is deselected.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn step(
    seen: Watch,
    session: Res<Session>,
    pieces: Pieces,
    mut tracking: ResMut<Tracking>,
    mut selected: ResMut<Selected>,
) {
    let world = seen.world();
    let live = Live {
        world,
        local: seen.local,
    };
    let state = &mut *tracking;
    let messages: Vec<u32> = world.messages().iter().map(|m| m.handle).collect();
    let highest = pieces.highest();

    match &mut state.mini {
        None => {
            let (mini, fresh) = started(&session, &live);

            // A new game's message flags came after its palace: a quest's
            // opening one is followed (11 corpus saves).
            let mut mini = mini;

            if fresh {
                for handle in &messages {
                    mini.message(*handle, &live);
                }
            }

            state.mini = Some(mini);
        }
        Some(mini) => {
            let born = pieces
                .iter()
                .map(|s| s.agent)
                .filter(|a| state.highest.is_none_or(|h| *a > h));

            for agent in born {
                mini.born(agent, &live);
            }

            for handle in messages.iter().filter(|h| !state.messages.contains(h)) {
                mini.message(*handle, &live);
            }
        }
    }

    state.highest = highest.max(state.highest);
    state.messages = messages;

    if let Some(mini) = &mut state.mini {
        mini.tick(seen.now, &live);

        state.aim = match mini.step(&live) {
            Step::Centre(focus) => Aim::Focus(focus),
            Step::Unseen(agent) => {
                if selected.0 == Some(agent) {
                    selected.0 = None;
                }

                Aim::Hold
            }
            Step::Still => Aim::Middle,
        };
    }
}

/// The window a game starts with: a resumed game's, else a new game's on
/// its palace; whether it is new.
fn started(session: &Session, live: &Live<'_>) -> (Tracker, bool) {
    match &session.resumed {
        Some(resumed) => (resumed.interface.tracking, false),
        None => (Tracker::started(live), true),
    }
}

/// The camera where the window looks: the followed piece's drawing, a
/// building's exit, a message flag's drawing, or the window's middle. Its
/// ground's rect for [`Inset`].
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
pub fn aim(
    mut tracking: ResMut<Tracking>,
    maps: Res<Maps>,
    pieces: Pieces,
    hung: Res<Hung>,
    drawn: Query<&Transform, Without<Camera>>,
    mut inset: ResMut<Inset>,
    mut camera: Single<&mut Transform, With<Camera>>,
) {
    let ground = |at: (i32, i32)| maps.standing_at(cells(at));
    let middle = tracking.mini.and_then(|m| m.middle).map(ground);
    let looked = match tracking.aim {
        Aim::Focus(Focus::Piece(agent)) => pieces
            .drawn(agent)
            .and_then(|e| drawn.get(e).ok())
            .map(|t| t.translation.truncate())
            .or(middle),
        Aim::Focus(Focus::Exit(_, at)) => Some(ground(at)),
        Aim::Focus(Focus::Point(at)) => {
            let follows = tracking.mini.map(|m| m.follows);
            let flag = follows.and_then(|f| match f {
                Follows::On(Target::Message(handle)) => hung
                    .get(handle)
                    .and_then(|entity| drawn.get(entity).ok())
                    .map(|t| t.translation.truncate()),
                _ => None,
            });

            Some(point_at(follows, flag, ground(at)))
        }
        Aim::Middle => middle,
        Aim::Hold => tracking.looked,
    };
    // The picture shows it again if drawn this frame.
    tracking.hovered = false;

    let Some(at) = looked else {
        return;
    };
    let snapped = snap(at);

    tracking.looked = Some(at);
    tracking.camera = Some(snapped);
    inset.seen = Some(Rect::from_center_size(at, IMAGE.as_vec2()));
    camera.translation = snapped.extend(camera.translation.z);
}

/// Where a camera looking at `at` snaps.
fn snap(at: Vec2) -> Vec2 {
    (at - SNAP).round() + SNAP
}

fn point_at(follows: Option<Follows>, flag: Option<Vec2>, ground: Vec2) -> Vec2 {
    match follows {
        Some(Follows::On(Target::Message(_))) => flag.unwrap_or(ground),
        _ => ground,
    }
}

/// 32nds of a cell as cells (`units::cells_f32`).
pub(crate) fn cells(at: (i32, i32)) -> Vec2 {
    Vec2::from(units::cells_f32(at))
}

/// The picture in `rect`, logical points; whether the pointer is on it,
/// for [`Tracking::pointed`].
pub fn show(ui: &mut egui::Ui, rect: egui::Rect, (tracking, picture): (&mut Tracking, &Picture)) {
    if let Some(id) = picture.id {
        let whole = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

        ui.painter().image(id, rect, whole, egui::Color32::WHITE);
    }

    // Hover alone: a press is the map's (`input::click`, `select::pick`).
    let response = ui.interact(rect, ui.id().with("tracking"), egui::Sense::hover());

    tracking.rect = Some(rect);
    tracking.hovered = response.hovered();
}

/// The followed piece's name, panel name with flags 0: its own,
/// else its title, "Palace" (**Game**: `ui_full.png`); a message flag's
/// title; none for nothing. Made once a game frame and target.
pub fn caption(
    corpus: &regent_draw::data::Corpus,
    (world, frames): (&regent_core::sim::world::World, u32),
    tracking: &mut Tracking,
) -> Option<String> {
    let follows = tracking.mini?.follows;
    let key = (frames, follows);

    if let Some((kept, named)) = &tracking.named
        && *kept == key
    {
        return named.clone();
    }

    let catalogue = &corpus.catalogue;
    let named = match follows {
        Follows::On(Target::Piece(agent)) => world.piece(agent).map(|p| {
            let code = p.code;
            let who = regent_core::view::panel::portrait::who(world, agent, code);

            regent_core::view::panel::portrait::dropped_name(catalogue, &corpus.text, world, agent)
                .unwrap_or_else(|| {
                    regent_core::view::panel::portrait::listed_name(catalogue, &corpus.text, &who)
                })
        }),
        Follows::On(Target::Message(_)) => regent_core::view::panel::tip::message_flag(catalogue),
        Follows::Nothing | Follows::Stopped => None,
    };

    tracking.named = Some((key, named.clone()));
    named
}

/// Names and amounts over the picture, as the main view paints them, by
/// the same zoom rule ([`words::shown_at`]) at the window's zoom.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn paint(
    mut contexts: EguiContexts,
    settings: Res<Settings>,
    tracking: Res<Tracking>,
    words: Query<(Entity, &Words)>,
    chain: Chain,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    if let (Some(rect), Some(camera), true) = (
        tracking.rect,
        tracking.camera,
        words::shown_at(&settings, tracking::ZOOM),
    ) {
        let view = as_view(rect, camera);
        let painter = ctx
            .layer_painter(LayerId::background())
            .with_clip_rect(rect);

        words::paint_on(&painter, &view, &words, &chain);
    }

    Ok(())
}

/// The picture in `rect` as a view: its middle on `camera`, a texel a
/// world unit.
fn as_view(rect: egui::Rect, camera: Vec2) -> View {
    View {
        rect: Some(Rect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y)),
        zoom: points_per_texel(rect),
        pan: camera,
        ..default()
    }
}

/// A press of the row's `column`: the
/// check's state after it, Ctrl held, the selection. Where 1040 puts the
/// main view, 32nds or an agent's place.
pub fn press(
    tracking: &mut Tracking,
    live: &Live<'_>,
    (column, checked, ctrl): (usize, bool, bool),
    selected: Option<u32>,
) -> Option<(i32, i32)> {
    let jump = tracking
        .mini
        .as_mut()?
        .press(column, (checked, ctrl), selected, live)?;

    match jump {
        Jump::Piece(agent) => live.world.position(agent),
        Jump::Point(at) => Some(at),
    }
}

/// Follow nothing, centred on `at`, 32nds: the minimap's right press.
pub fn stop_at(tracking: &mut Tracking, live: &Live<'_>, at: (i32, i32)) {
    if let Some(mini) = &mut tracking.mini {
        mini.stop_at(at, live);
    }
}

/// Follow `target`, asked: a panel's track button, a list row, a key.
pub fn follow(tracking: &mut Tracking, live: &Live<'_>, target: Target) {
    if let Some(mini) = &mut tracking.mini {
        mini.follow(target, live);
    }
}

/// The window's image and camera, its state a game's, its systems.
pub fn plugin(app: &mut App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Tracking>()
        .per_game::<Inset>()
        .add_systems(Startup, spawn)
        .add_systems(PreUpdate, lend)
        .add_systems(Update, shown)
        .add_systems(
            bevy_egui::EguiPrimaryContextPass,
            paint
                .after(crate::ui::bar)
                .in_set(crate::draw::frame::Ui::Bar)
                .run_if(crate::menu::in_game),
        );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_focus_uses_the_drawn_flag() {
        let ground = Vec2::new(100.0, 100.0);
        let flag = Vec2::new(100.0, 160.0);
        let follows = Some(Follows::On(Target::Message(42)));

        assert_eq!(point_at(follows, Some(flag), ground), flag);
        assert_eq!(point_at(follows, None, ground), ground);
        assert_eq!(point_at(None, Some(flag), ground), ground);
    }

    /// The pointer on the picture is on the world point the camera draws
    /// there: the middle on the snapped focus, a texel right and up a
    /// world unit right and up; a name painted at a world point is under
    /// the pointer on it.
    #[test]
    fn the_pointer_lands_on_what_the_window_draws() {
        let at = Vec2::new(300.3, 500.6);
        let rect =
            egui::Rect::from_min_size(egui::pos2(10.0, 20.0), IMAGE.as_vec2().to_array().into());
        let snapped = snap(at);
        let tracking = Tracking {
            camera: Some(snapped),
            rect: Some(rect),
            hovered: true,
            ..Tracking::default()
        };
        let middle = Vec2::new(rect.center().x, rect.center().y);
        let per = points_per_texel(rect);

        assert!(snapped.distance(at) <= 0.5_f32.hypot(0.5));
        assert_eq!(tracking.pointed(middle), Some(snapped));
        assert_eq!(
            tracking.pointed(middle + Vec2::new(per, -per)),
            Some(snapped + Vec2::ONE)
        );

        let named = snapped + Vec2::new(-40.0, 25.0);
        let painted = as_view(rect, snapped).screen_at(named).expect("a rect");

        assert!(tracking.pointed(painted).unwrap().distance(named) < 1e-3);
    }

    /// A new game's window follows the palace and centres on its exit,
    /// before the first sight pass as after.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_new_game_shows_the_palace_at_its_exit() {
        let (begun, _) = crate::game::begin::test_game(&[]);
        let mut sim = begun.sim;

        for frames in 0..3 {
            let (world, local) = (sim.world(), sim.local());
            let live = Live { world, local };
            let mut mini = Tracker::started(&live);
            let palace = world.player_palace(local).expect("a palace");
            let exit = world.exit_of(palace).expect("an exit");

            assert_eq!(mini.follows, Follows::On(Target::Piece(palace)));
            assert_eq!(
                mini.step(&live),
                Step::Centre(Focus::Exit(palace, exit)),
                "after {frames} frames"
            );

            sim.step();
        }
    }
}
