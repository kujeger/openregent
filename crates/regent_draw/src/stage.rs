//! The camera: where the picture is, how magnified, and what the panels
//! leave of the window. openregent and `regent_viewer` share it.

use bevy::camera::Viewport;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::light::cluster::{ClusterConfig, GlobalClusterSettings};
use bevy::prelude::*;
use bevy::render::view::NoIndirectDrawing;

use crate::data::{ZOOM_MAX, ZOOM_MIN};

/// The picture: where it is, and how magnified.
///
/// `rect`: the picture area in logical points, written by the panel pass;
/// the wheel and clicks test the cursor against it.
///
/// # Zoom is the camera only
///
/// [`aim`] writes `pan` into the world camera's translation and `1 / zoom`
/// into its orthographic scale. Entities spawn at true position, scale one.
#[derive(Resource, Clone, Copy)]
pub struct View {
    pub rect: Option<Rect>,
    pub zoom: f32,
    /// Where the camera is looking, in world units.
    pub pan: Vec2,
    /// Unmagnified content size, for [`fit`]. Written by the active subject;
    /// zero means nothing to frame.
    pub content: Vec2,
    /// Request a reframe, once content is measured.
    pub refit: bool,
    /// What shadows fall on (indices 247 to 254 shade the pixel below; see
    /// [`regent_core::view::raster::shade`]). Also the camera's clear colour.
    pub backdrop: [u8; 3],
    /// A world rect, centred on the origin, the view may not leave
    /// ([`bound`]): the map's extent in openregent. `None` scrolls anywhere,
    /// as the viewer does.
    pub bounds: Option<Vec2>,
}

/// What the unit browser opens at: twice life size.
///
/// Fixed, not fitted, so units can be compared.
pub const BROWSE_ZOOM: f32 = 2.0;

/// The lit dirt beside the Wizards Tower's shadow, measured off
/// `local/shots/heighty_lowerleft_native.png` and quoted in `docs/formats/art.md`.
///
/// Every channel lies on the 565 grid, confirming an unscaled capture.
pub const DIRT: [u8; 3] = [112, 84, 56];

impl Default for View {
    fn default() -> Self {
        View {
            rect: None,
            zoom: BROWSE_ZOOM,
            pan: Vec2::ZERO,
            content: Vec2::ZERO,
            // The unit browser does not fit; other subjects request it.
            refit: false,
            backdrop: DIRT,
            bounds: None,
        }
    }
}

impl View {
    /// The world point under a cursor given in logical points.
    ///
    /// From `pan` and `zoom`, not
    /// [`bevy::camera::Camera::viewport_to_world_2d`] (last frame's transform).
    /// `None` until the panels set `rect`.
    pub fn world_at(&self, cursor: Vec2) -> Option<Vec2> {
        let from_middle = cursor - self.rect?.center();

        // Window y counts down, world y up.
        Some(self.pan + Vec2::new(from_middle.x, -from_middle.y) / self.zoom)
    }

    /// Where a world point lands on screen, in logical points.
    ///
    /// Inverse of [`View::world_at`], for egui legends.
    pub fn screen_at(&self, world: Vec2) -> Option<Vec2> {
        let middle = self.rect?.center();
        let from_pan = (world - self.pan) * self.zoom;

        Some(Vec2::new(middle.x + from_pan.x, middle.y - from_pan.y))
    }

    /// The world rect on screen, as [`View::world_at`] puts its corners.
    /// `None` until the panels set `rect`.
    pub fn seen(&self) -> Option<Rect> {
        let rect = self.rect?;

        [rect.min, rect.max]
            .map(|at| self.world_at(at))
            .into_iter()
            .try_fold(Rect::EMPTY, |seen, at| Some(seen.union_point(at?)))
    }

    /// Drag the picture by a cursor movement, in logical points.
    ///
    /// Keeps the grabbed point under the cursor at any zoom.
    pub fn drag(&mut self, moved: Vec2) {
        // Camera moves opposite the cursor; mouse y counts down.
        self.pan += Vec2::new(-moved.x, moved.y) / self.zoom;
    }

    /// The least zoom [`View::bounds`] allows ([`least`]); `None`
    /// unbounded or before the panels set `rect`.
    pub fn least(&self) -> Option<f32> {
        let (extent, rect) = self.bounds.zip(self.rect)?;

        Some(least(rect.size(), extent))
    }

    /// The pan nearest `pan` that keeps the view inside [`View::bounds`]
    /// ([`bounded`]); `pan` itself unbounded or before the panels set
    /// `rect`.
    pub fn kept(&self, pan: Vec2) -> Vec2 {
        match (self.bounds, self.rect) {
            (Some(extent), Some(rect)) => bounded(pan, self.zoom, rect.size(), extent),
            _ => pan,
        }
    }

    /// Look at world point `at`, the pan kept inside [`View::bounds`]
    /// ([`View::kept`]) at once: for a pan set after [`bound`] has run,
    /// as the panels' pass sets it (a minimap drag).
    pub fn centre(&mut self, at: Vec2) {
        self.pan = self.kept(at);
    }

    /// Multiply the magnification, holding one screen point still.
    ///
    /// `about`: the pointer, kept fixed. `None` zooms about the middle.
    pub fn magnify(&mut self, factor: f32, about: Option<Vec2>) {
        let held = about.and_then(|at| self.world_at(at).map(|world| (at, world)));

        self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);

        if let Some((at, world)) = held
            && let Some(now) = self.world_at(at)
        {
            self.pan += world - now;
        }
    }
}

/// The pan nearest `pan` whose view, `room` logical points at `zoom`,
/// stays inside `extent`, a world rect centred on the origin. On an axis
/// where the view is wider than the extent, the extent is centred.
///
/// ```text
///  extent 1000 wide, view 400 wide: pan.x within -300 ..= 300
///  extent 1000 wide, view 1200 wide: pan.x is 0
/// ```
#[must_use]
pub fn bounded(pan: Vec2, zoom: f32, room: Vec2, extent: Vec2) -> Vec2 {
    let slack = ((extent - room / zoom) / 2.0).max(Vec2::ZERO);

    pan.clamp(-slack, slack)
}

/// The least zoom at which a view `room` logical points across stays
/// inside `extent` on both axes: the axis it would first show past
/// binds.
///
/// ```text
///  extent 4096 x 2350 (128 cells), room 2270 x 1000: 0.55, the width binds
/// ```
#[must_use]
pub fn least(room: Vec2, extent: Vec2) -> f32 {
    (room / extent).max_element()
}

/// Drag to pan, on the middle button or with a modifier held.
///
/// Left paints and right erases in the painter.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn pan(
    mut motion: MessageReader<bevy::input::mouse::MouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut view: ResMut<View>,
) {
    let dragging = buttons.pressed(MouseButton::Middle)
        || (buttons.pressed(MouseButton::Left) && keys.pressed(KeyCode::ShiftLeft));

    let moved: Vec2 = motion.read().map(|m| m.delta).sum();

    if dragging && moved != Vec2::ZERO {
        view.drag(moved);
    }
}

/// The camera that draws the world, as opposed to the one that draws egui.
///
/// Own marker, not `Without<EguiContext>`: independent of when `bevy_egui`
/// inserts its component.
#[derive(Component)]
pub struct WorldCamera;

/// A camera that draws the world (`crate::frame`): 3D, orthographic with
/// the 2D camera's depth range (`-1000 ..= 1000`, so every draw depth is
/// where it was), and none of 3D's defaults that change colours or
/// coverage: MSAA (averages the shadows' coverage in alpha), tonemapping,
/// deband dither. Not HDR: the 8-bit sRGB target the resolve expects.
///
/// No indirect draws: Bevy 0.20 culls them on the GPU, and each sprite,
/// its own material, is its own batch, so every frame pays buffer
/// writes, bind groups and compute passes per sprite, for nothing to cull
/// (about twice 2D's frame; `docs/openregent/decisions.md`).
#[must_use]
pub fn eye() -> impl Bundle {
    (
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection::default_2d()),
        Msaa::Off,
        Tonemapping::None,
        DebandDither::Disabled,
        // No point or spot lights: one cluster. `None` leaves Bevy 0.20's
        // GPU clustering a zero-sized texture, a validation error.
        ClusterConfig::Single,
        NoIndirectDrawing,
    )
}

/// Bevy's shadow maps' texels a side while nothing casts: the
/// directional one also while no model does (`sun`), the point lights'
/// always, as there are none. Bevy allocates both whether or not a light
/// draws into them: by default 2048² and a 1024² cube, about 40 MB.
pub const IDLE_SHADOW_MAP: usize = 1;

/// Light clusters worked out on the CPU, not the GPU: with no point or
/// spot lights the GPU's way still ran five passes and a readback every
/// frame (about 0.5 ms), and its indirect draws kept wgpu's validation
/// buffers (16 to 20 MB). Startup, after the PBR plugin made the settings.
pub fn clusters(settings: Option<ResMut<GlobalClusterSettings>>) {
    if let Some(mut settings) = settings {
        settings.gpu_clustering = None;
    }
}

/// Put the camera where the pan says, at the magnification the zoom says.
///
/// Orthographic `scale` is world units per pixel, so `1 / zoom`. Also sets
/// the clear colour to [`View::backdrop`].
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn aim(
    view: Res<View>,
    mut clear: ResMut<ClearColor>,
    camera: Single<(&mut Transform, &mut Projection), With<WorldCamera>>,
) {
    let (mut transform, mut projection) = camera.into_inner();

    transform.translation = view.pan.extend(transform.translation.z);

    if let Projection::Orthographic(ortho) = &mut *projection {
        ortho.scale = 1.0 / view.zoom;
    }

    let [r, g, b] = view.backdrop;

    clear.0 = Color::srgb_u8(r, g, b);
}

/// Keep the view inside [`View::bounds`], after every pan, zoom and resize:
/// zoomed in to [`View::least`] if out past it, then panned in
/// ([`View::kept`]).
pub fn bound(mut view: ResMut<View>) {
    if let Some(least) = view.least().filter(|least| view.zoom < *least) {
        let by = least / view.zoom;

        view.magnify(by, None);
        view.zoom = least;
    }

    let pan = view.kept(view.pan);

    // Written only on change: `View` drives the ground's redraw.
    if pan != view.pan {
        view.pan = pan;
    }
}

/// Frame whatever is on screen, when something has asked for it.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn fit(mut view: ResMut<View>) {
    let (Some(rect), true) = (view.rect, view.refit) else {
        return;
    };

    let room = rect.size();

    if view.content.x <= 0.0 || view.content.y <= 0.0 || room.x <= 0.0 || room.y <= 0.0 {
        return;
    }

    let scale = (room / (view.content * FIT_MARGIN)).min_element();

    view.zoom = scale.clamp(ZOOM_MIN, ZOOM_MAX);
    view.pan = Vec2::ZERO;
    view.refit = false;
}

/// How much room to leave around a fitted picture.
const FIT_MARGIN: f32 = 1.04;

/// Touchpad pixels per wheel notch. Chosen by feel.
pub const PIXELS_PER_NOTCH: f32 = 40.0;

/// The wheel's notches this frame and the cursor, when turned over the
/// picture; over a panel the wheel belongs to egui.
pub fn turned(
    wheel: &mut MessageReader<MouseWheel>,
    window: &Window,
    view: &View,
) -> Option<(f32, Vec2)> {
    let notches: f32 = wheel
        .read()
        .map(|e| match e.unit {
            MouseScrollUnit::Line => e.y,
            MouseScrollUnit::Pixel => e.y / PIXELS_PER_NOTCH,
        })
        .sum();
    let cursor = window.cursor_position()?;

    (notches != 0.0 && view.rect?.contains(cursor)).then_some((notches, cursor))
}

/// Clip the world camera to whatever the panels left over.
///
/// Logical points to physical pixels. Saturating: panels may exceed the window.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a float-to-int `as` saturates, and a panel extent is already non-negative"
)]
pub fn viewport(camera: &mut Camera, window: &Window, [left, right, top, bottom]: [f32; 4]) {
    let scale = window.scale_factor();
    let physical = |points: f32| (points * scale).round().max(0.0) as u32;

    let pos = UVec2::new(physical(left), physical(top));
    let taken = pos + UVec2::new(physical(right), physical(bottom));
    let whole = UVec2::new(window.physical_width(), window.physical_height());

    // wgpu rejects zero-sized viewports; keep the last good one.
    let size = UVec2::new(
        whole.x.saturating_sub(taken.x),
        whole.y.saturating_sub(taken.y),
    );

    if size.x > 0 && size.y > 0 {
        camera.viewport = Some(Viewport {
            physical_position: pos,
            physical_size: size,
            ..default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two cameras: a `Single` on `Camera` alone would match twice, and
    /// Bevy skips it silently.
    #[test]
    fn panning_moves_the_world_camera_and_leaves_the_panels_one_alone() {
        let mut app = App::new();

        app.insert_resource(View {
            pan: Vec2::new(30.0, -12.0),
            ..default()
        });
        // `aim` writes it; no render plugins here to supply it.
        app.insert_resource(ClearColor::default());
        app.add_systems(Update, aim);

        // As `cameras` spawns them: only one the world.
        let world = app
            .world_mut()
            .spawn((eye(), WorldCamera, Transform::from_xyz(0.0, 0.0, 5.0)))
            .id();
        let panels = app.world_mut().spawn(Camera2d).id();

        app.update();

        let at = |entity| app.world().get::<Transform>(entity).unwrap().translation;

        assert_eq!(
            at(world),
            Vec3::new(30.0, -12.0, 5.0),
            "the world camera did not follow the pan"
        );
        assert_eq!(at(panels), Vec3::ZERO, "the panels' camera moved");
    }

    /// The world camera draws as the 2D one did: 2D's depth range, and
    /// none of 3D's defaults that change colours or coverage.
    #[test]
    #[allow(clippy::float_cmp, reason = "copied, not computed")]
    fn the_world_camera_keeps_2ds_depths_and_colours() {
        let mut world = World::new();
        let camera = world.spawn(eye()).id();
        let at = world.entity(camera);

        let Some(Projection::Orthographic(ortho)) = at.get::<Projection>() else {
            panic!("not orthographic");
        };
        let flat = OrthographicProjection::default_2d();

        assert_eq!((ortho.near, ortho.far), (flat.near, flat.far));
        assert_eq!(at.get::<Msaa>(), Some(&Msaa::Off));
        assert_eq!(at.get::<Tonemapping>(), Some(&Tonemapping::None));
        assert_eq!(at.get::<DebandDither>(), Some(&DebandDither::Disabled));
        assert!(!at.contains::<bevy::camera::Hdr>());
    }

    /// An 800 by 600 picture region, offset by panels.
    ///
    /// Offset, so ignoring the panels' width fails.
    fn view(zoom: f32, pan: Vec2) -> View {
        View {
            rect: Some(Rect {
                min: Vec2::new(320.0, 40.0),
                max: Vec2::new(1120.0, 640.0),
            }),
            zoom,
            pan,
            ..default()
        }
    }

    #[test]
    fn the_two_conversions_are_inverses() {
        let view = view(3.5, Vec2::new(-12.0, 40.0));

        for world in [
            Vec2::ZERO,
            Vec2::new(100.0, -250.0),
            Vec2::new(-8192.0, 8192.0),
        ] {
            let there = view.screen_at(world).expect("a viewport");
            let back = view.world_at(there).expect("a viewport");

            assert!(
                (back - world).length() < 1e-3,
                "{world} went to {there} and came back {back}"
            );
        }
    }

    /// `aim` relies on this.
    #[test]
    fn the_pan_is_what_sits_in_the_middle() {
        let view = view(2.0, Vec2::new(50.0, -30.0));
        let middle = view.rect.unwrap().center();

        assert_eq!(view.world_at(middle), Some(Vec2::new(50.0, -30.0)));
    }

    /// Corners too: a pan sign error is invisible at the centre.
    #[test]
    fn zooming_about_a_point_holds_that_point_still() {
        for cursor in [
            Vec2::new(720.0, 340.0),
            Vec2::new(340.0, 60.0),
            Vec2::new(1100.0, 620.0),
        ] {
            for factor in [1.15, 0.5, 4.0] {
                let mut view = view(3.0, Vec2::new(7.0, -11.0));
                let before = view.world_at(cursor).expect("a viewport");

                view.magnify(factor, Some(cursor));

                let after = view.world_at(cursor).expect("a viewport");

                assert!(
                    (after - before).length() < 1e-3,
                    "at {cursor} by {factor}: {before} became {after}"
                );
                assert!((view.zoom - 3.0 * factor).abs() < 1e-3, "{}", view.zoom);
            }
        }
    }

    /// Tested either side of zoom 1, where a missing division is invisible.
    #[test]
    fn dragging_keeps_the_world_under_the_cursor() {
        for zoom in [0.05, 1.0, 4.0, 22.0] {
            let mut view = view(zoom, Vec2::new(7.0, -11.0));

            let from = Vec2::new(600.0, 300.0);
            let moved = Vec2::new(-37.0, 21.0);
            let held = view.world_at(from).expect("a viewport");

            view.drag(moved);

            let now = view.world_at(from + moved).expect("a viewport");

            assert!(
                (now - held).length() < 1e-3,
                "at zoom {zoom}: {held} slid to {now}"
            );
        }
    }

    #[test]
    fn a_bounded_view_stays_inside_the_extent() {
        let extent = Vec2::new(1000.0, 800.0);
        let room = Vec2::new(400.0, 200.0);

        // Inside: untouched.
        let inside = Vec2::new(100.0, -50.0);
        assert_eq!(bounded(inside, 1.0, room, extent), inside);

        // Past an edge: the view's edge on the extent's.
        assert_eq!(
            bounded(Vec2::new(900.0, -900.0), 1.0, room, extent),
            Vec2::new(300.0, -300.0)
        );

        // Zoomed in, more slack: 200 world units of view leave 400.
        assert_eq!(
            bounded(Vec2::new(900.0, 0.0), 2.0, room, extent),
            Vec2::new(400.0, 0.0)
        );

        // Wider than the extent: centred on that axis only. 1600 by 400
        // world units of view.
        assert_eq!(
            bounded(Vec2::new(50.0, 50.0), 0.25, Vec2::new(400.0, 100.0), extent),
            Vec2::new(0.0, 50.0)
        );
    }

    /// Zoomed out past the map on either axis or panned past its edge,
    /// at any window shape and map size: [`bound`] zooms in until the
    /// view fits and pans it in, so nothing past the map's edge shows.
    #[test]
    fn a_bounded_view_never_shows_past_the_extent() {
        use regent_core::data::settings::SQUASH;

        // Cells a side, as the art layer draws them (`map::Layer::pitch`).
        const PITCH: f32 = 32.0;

        let rooms = [
            Vec2::new(1628.0, 1060.0),
            Vec2::new(2270.0, 1000.0),
            Vec2::new(990.0, 650.0),
            Vec2::new(620.0, 1300.0),
        ];

        for side in [128.0, 256.0, 512.0] {
            let extent = Vec2::new(side * PITCH, side * PITCH * SQUASH);

            for room in rooms {
                for (zoom, pan) in [0.02, 0.25, 0.5, 1.0, 2.0]
                    .into_iter()
                    .flat_map(|z| [Vec2::new(-1e5, 1e5), Vec2::new(1e5, -1e5)].map(|p| (z, p)))
                {
                    let mut app = App::new();

                    app.insert_resource(View {
                        rect: Some(Rect::from_corners(
                            Vec2::new(290.0, 40.0),
                            Vec2::new(290.0, 40.0) + room,
                        )),
                        zoom,
                        pan,
                        bounds: Some(extent),
                        ..default()
                    });
                    app.add_systems(Update, bound);
                    app.update();

                    let view = app.world().resource::<View>();
                    let seen = view.seen().expect("a room");
                    let edge = Rect::from_center_size(Vec2::ZERO, extent).inflate(0.5);

                    assert!(
                        edge.contains(seen.min) && edge.contains(seen.max),
                        "{side} cells, room {room}, zoom {zoom}, pan {pan}: sees {seen:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn zooming_about_nothing_leaves_the_pan_alone() {
        let mut view = view(3.0, Vec2::new(7.0, -11.0));

        view.magnify(2.0, None);

        assert_eq!(view.pan, Vec2::new(7.0, -11.0));
        assert!((view.zoom - 6.0).abs() < 1e-3);
    }

    /// Orthographic `scale` is world units per pixel: the inverse of zoom.
    #[test]
    fn the_zoom_reaches_the_camera_as_the_reciprocal_of_its_scale() {
        let mut app = App::new();

        app.insert_resource(View {
            zoom: 8.0,
            ..default()
        });
        app.insert_resource(ClearColor::default());
        app.add_systems(Update, aim);

        let camera = app.world_mut().spawn((eye(), WorldCamera)).id();

        app.update();

        let Some(Projection::Orthographic(ortho)) = app.world().get::<Projection>(camera) else {
            panic!("the camera lost its orthographic projection");
        };

        assert!((ortho.scale - 0.125).abs() < 1e-6, "{}", ortho.scale);
    }
}
