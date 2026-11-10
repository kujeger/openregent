//! The view's zoom: the original's two levels
//! ([`regent_core::view::map::zoom`]), or smooth, an enhancement
//! (`[enhancements] smooth_zoom`).
//!
//! ```text
//!  levels   wheel in: 1:1, wheel out: 2:1 (scale 0.5), about the pointer
//!  smooth   a notch steps STEPS, 0.5 ..= 4.0, about the pointer;
//!           a middle click or 0: 1:1
//! ```

use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use std::f32::consts::{FRAC_1_SQRT_2, SQRT_2};

use regent_core::view::map::zoom::{OUT, REGULAR, toggled};
use regent_draw::stage::{self, View};

use crate::files::config::Settings;
use crate::panel::Panels;
use crate::ui::{Covered, on_map};

/// How far the cursor may move, logical pixels, between a middle press
/// and release that still count as a click. **Inferred**: chosen by feel.
const CLICK_SLOP: f32 = 4.0;

/// Smooth zoom's scales: quarter octaves, `2^(k/4)` for k = -4..=8, and
/// an exact 3. A table, not a running product, so the anchors stay exact.
pub const STEPS: [f32; 14] = [
    OUT,
    0.594_603_6,
    FRAC_1_SQRT_2,
    0.840_896_4,
    REGULAR,
    1.189_207_1,
    SQRT_2,
    1.681_792_8,
    2.0,
    2.378_414_2,
    2.0 * SQRT_2,
    3.0,
    3.363_585_8,
    4.0,
];

/// [`REGULAR`]'s place in [`STEPS`].
const HOME: usize = 4;

/// Smooth zoom's place in [`STEPS`], and the part notch a touchpad has
/// sent towards the next.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Step {
    at: usize,
    carry: f32,
}

impl Default for Step {
    fn default() -> Self {
        Step {
            at: HOME,
            carry: 0.0,
        }
    }
}

impl Step {
    /// Turned `notches` (+ in): whole ones step, clamped at the ends, the
    /// low end the entry at or below `least`; the rest waits. Its scale
    /// after ([`Step::level`]).
    fn turn(&mut self, notches: f32, least: f32) -> f32 {
        let total = self.carry + notches;
        let whole = total.trunc();
        let floor = Step::under(least).at;

        self.carry = total - whole;
        self.at = shifted(self.at.max(floor), whole).max(floor);
        self.level(least)
    }

    /// Its scale, raised to `least`: the lowest the map's bound allows
    /// (`View::least`), between two entries as often as not.
    const fn level(&self, least: f32) -> f32 {
        STEPS[self.at].max(least)
    }

    /// The entry at or below `zoom` (the lowest below them all).
    fn under(zoom: f32) -> Step {
        let at = STEPS.iter().rposition(|s| *s <= zoom).unwrap_or(0);

        Step { at, carry: 0.0 }
    }
}

/// Index `at` moved `by` whole steps, kept in [`STEPS`].
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "small whole numbers, clamped to the table"
)]
fn shifted(at: usize, by: f32) -> usize {
    let last = (STEPS.len() - 1) as f32;

    (at as f32 + by).clamp(0.0, last) as usize
}

/// How the wheel zooms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The original's two levels.
    Levels,
    /// Any magnification.
    Smooth,
}

impl Mode {
    pub const fn of(settings: &Settings) -> Mode {
        if settings.enhancements.smooth_zoom {
            Mode::Smooth
        } else {
            Mode::Levels
        }
    }
}

/// The wheel's systems and smooth zoom's reset, each for its mode.
pub fn systems() -> ScheduleConfigs<ScheduleSystem> {
    (
        wheel.run_if(smooth),
        levels.run_if(not(smooth)),
        reset.run_if(smooth),
    )
        .into_configs()
}

/// Whether the zoom is smooth: a run condition.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn smooth(settings: Res<Settings>) -> bool {
    Mode::of(&settings) == Mode::Smooth
}

/// The wheel between the two levels: in to 1:1, out to 2:1.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn levels(
    mut wheel: MessageReader<MouseWheel>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut view: ResMut<View>,
    covered: Res<Covered>,
) {
    if let Some((notches, cursor)) = uncovered(&mut wheel, &window, &view, &covered) {
        let level = if notches > 0.0 { REGULAR } else { OUT };

        zoom_to(&mut view, level, Some(cursor));
    }
}

/// The wheel through [`STEPS`], about the pointer.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn wheel(
    mut wheel: MessageReader<MouseWheel>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut view: ResMut<View>,
    mut step: ResMut<Step>,
    covered: Res<Covered>,
) {
    if let Some((notches, cursor)) = uncovered(&mut wheel, &window, &view, &covered) {
        turn(&mut view, &mut step, notches, Some(cursor));
    }
}

/// The wheel's notches and the pointer, on the map: over a menu, the
/// settings editor or a panel ([`Covered`]) the wheel is theirs. Read
/// either way, so none wait for the next turn.
fn uncovered(
    wheel: &mut MessageReader<MouseWheel>,
    window: &Window,
    view: &View,
    covered: &Covered,
) -> Option<(f32, Vec2)> {
    stage::turned(wheel, window, view).filter(|(_, at)| on_map(view, covered, *at))
}

/// `view` turned `notches` (+ in) through [`STEPS`], holding `about`
/// still (`None`: the middle).
fn turn(view: &mut View, step: &mut Step, notches: f32, about: Option<Vec2>) {
    let level = step.turn(notches, view.least().unwrap_or(0.0));

    zoom_to(view, level, about);
}

/// Onto [`STEPS`] when something else set the zoom (`stage::fit`): the
/// entry at or below, so a fitted map still fits; no lower than the
/// map's bound allows (`View::least`), which `stage::bound` holds.
pub fn snap(mut view: ResMut<View>, mut step: ResMut<Step>) {
    let least = view.least().unwrap_or(0.0);
    #[allow(clippy::float_cmp, reason = "zoom_to sets the level exactly")]
    let off = view.zoom != step.level(least);

    if off {
        let under = Step::under(view.zoom).at;

        *step = Step {
            at: under.max(Step::under(least).at),
            carry: 0.0,
        };
        zoom_to(&mut view, step.level(least), None);
    }
}

/// Back to 1:1 in smooth zoom: the 0 key, or a middle click. A middle
/// drag pans (`stage::pan`), so the cursor must stay put.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn reset(
    (keys, buttons): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>),
    (covered, panels, mut contexts): (Res<Covered>, Res<Panels>, EguiContexts),
    window: Single<&Window, With<PrimaryWindow>>,
    (mut view, mut step): (ResMut<View>, ResMut<Step>),
    mut pressed: Local<Option<Vec2>>,
) {
    let cursor = window.cursor_position();

    // The keys are the map's as in `map_keys::keys`: not a text box's
    // (the chat) nor a menu's or the settings editor's.
    let typing = contexts
        .ctx_mut()
        .is_ok_and(|ctx| ctx.egui_wants_keyboard_input());
    let zero = keys.just_pressed(KeyCode::Digit0) && !typing && panels.state.options.open.is_none();

    // Pressed on the map: over a panel or menu the button is egui's.
    if buttons.just_pressed(MouseButton::Middle) {
        *pressed = cursor.filter(|at| on_map(&view, &covered, *at));
    }

    let released = if buttons.just_released(MouseButton::Middle) {
        pressed.take()
    } else {
        None
    };
    let clicked = released
        .zip(cursor)
        .is_some_and(|(down, up)| down.distance(up) <= CLICK_SLOP);

    if clicked || zero {
        *step = Step::default();
        zoom_to(&mut view, REGULAR, None);
    }
}

/// The minimap's zoom button: 1:1 to 2:1 out,
/// any other zoom to 1:1, about the view's middle.
pub fn toggle(view: &mut View) {
    zoom_to(view, toggled(view.zoom), None);
}

/// `--camera wheel`'s `n`th notch, about the middle: in to the top of
/// [`STEPS`] and back out, over and over.
pub fn sweep(view: &mut View, step: &mut Step, n: u32) {
    let last = u32::try_from(STEPS.len() - 1).unwrap_or(1);
    let notch = if ((n - 1) / last).is_multiple_of(2) {
        1.0
    } else {
        -1.0
    };

    turn(view, step, notch, None);
}

/// `view` at zoom `level`, holding `about` still (`None`: the middle).
fn zoom_to(view: &mut View, level: f32, about: Option<Vec2>) {
    view.magnify(level / view.zoom, about);

    // Exactly the level, whatever the division left.
    view.zoom = level;
}

/// The zoom's step in hand, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Step>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Out and back in about the same point: exactly 1:1, where it was.
    #[test]
    #[allow(clippy::float_cmp, reason = "the levels are exact")]
    fn the_levels_are_exact_and_hold_the_pointer() {
        let mut view = View {
            zoom: REGULAR,
            rect: Some(Rect::new(0.0, 0.0, 800.0, 600.0)),
            pan: Vec2::new(30.0, -12.0),
            ..default()
        };
        let at = Vec2::new(100.0, 450.0);
        let under = view.world_at(at);

        zoom_to(&mut view, OUT, Some(at));
        assert_eq!(view.zoom, OUT);
        assert_eq!(view.world_at(at), under);

        view.magnify(1.15, None);
        zoom_to(&mut view, REGULAR, Some(at));
        assert_eq!(view.zoom, REGULAR);
    }

    #[test]
    #[allow(clippy::float_cmp, reason = "the anchors are exact")]
    fn the_steps_are_quarter_octaves_with_exact_anchors() {
        assert_eq!(STEPS[0], 0.5);
        assert_eq!(STEPS[HOME], 1.0);
        assert_eq!(STEPS[8], 2.0);
        assert_eq!(STEPS[11], 3.0);
        assert_eq!(STEPS[13], 4.0);

        let octaves = STEPS.iter().filter(|s| **s != 3.0);

        for (k, s) in (-4_i8..=8).zip(octaves) {
            assert!((s - 2f32.powf(f32::from(k) / 4.0)).abs() < 1e-6, "{k}: {s}");
        }
    }

    /// `--camera wheel` runs to the top of the table, then the bottom.
    #[test]
    #[allow(clippy::float_cmp, reason = "table entries")]
    fn the_sweep_runs_in_to_the_top_then_out_to_the_bottom() {
        let mut view = View {
            rect: Some(Rect::new(0.0, 0.0, 800.0, 600.0)),
            ..default()
        };
        let mut step = Step::default();
        let last = u32::try_from(STEPS.len() - 1).expect("a short table");

        (1..=last).for_each(|n| sweep(&mut view, &mut step, n));
        assert_eq!(view.zoom, STEPS[STEPS.len() - 1]);

        (last + 1..=2 * last).for_each(|n| sweep(&mut view, &mut step, n));
        assert_eq!(view.zoom, STEPS[0]);
    }

    /// N in, N out: exactly where it started, at every entry.
    #[test]
    #[allow(clippy::float_cmp, reason = "table entries")]
    fn notches_in_then_out_return_exactly() {
        for (start, &zoom) in STEPS.iter().enumerate() {
            let room = STEPS.len() - 1 - start;

            for n in (0_u8..).take(room + 1).map(f32::from) {
                let mut step = Step {
                    at: start,
                    carry: 0.0,
                };
                let mut view = View {
                    zoom,
                    rect: Some(Rect::new(0.0, 0.0, 800.0, 600.0)),
                    ..default()
                };
                let at = Vec2::new(100.0, 450.0);

                let level = step.turn(n, 0.0);
                zoom_to(&mut view, level, Some(at));
                let level = step.turn(-n, 0.0);
                zoom_to(&mut view, level, Some(at));

                assert_eq!(view.zoom, zoom, "{start} +-{n}");
            }
        }

        let mut step = Step::default();
        step.turn(3.0, 0.0);
        assert_eq!(step.turn(-3.0, 0.0), 1.0);
    }

    #[test]
    #[allow(clippy::float_cmp, reason = "table entries")]
    fn the_steps_clamp_at_the_ends() {
        let mut step = Step::default();

        assert_eq!(step.turn(20.0, 0.0), 4.0);
        assert_eq!(step.turn(1.0, 0.0), 4.0);
        assert_eq!(step.turn(-30.0, 0.0), 0.5);
        assert_eq!(step.turn(-1.0, 0.0), 0.5);
        assert_eq!(step.turn(1.0, 0.0), STEPS[1]);
    }

    /// A touchpad's part notches add up to whole steps.
    #[test]
    #[allow(clippy::float_cmp, reason = "table entries")]
    fn part_notches_add_up_to_steps() {
        let mut step = Step::default();

        assert_eq!(step.turn(0.25, 0.0), 1.0);
        assert_eq!(step.turn(0.5, 0.0), 1.0);
        assert_eq!(step.turn(0.5, 0.0), STEPS[HOME + 1]);
        assert_eq!(step.turn(-0.5, 0.0), STEPS[HOME + 1]);
        assert_eq!(step.turn(-0.5, 0.0), STEPS[HOME + 1]);
        assert_eq!(step.turn(-0.75, 0.0), 1.0);
    }

    /// A fitted zoom goes to the entry at or below; below them all, 0.5.
    #[test]
    #[allow(clippy::float_cmp, reason = "table entries")]
    fn a_fitted_zoom_snaps_down() {
        assert_eq!(STEPS[Step::under(1.3).at], STEPS[HOME + 1]);
        assert_eq!(STEPS[Step::under(2.0).at], 2.0);
        assert_eq!(STEPS[Step::under(0.1).at], 0.5);
        assert_eq!(STEPS[Step::under(9.0).at], 4.0);
    }

    /// The minimap's button: out from 1:1, back to 1:1 from anything
    /// else, the view's middle held.
    #[test]
    #[allow(clippy::float_cmp, reason = "the levels are exact")]
    fn the_button_flips_between_the_two_levels() {
        let mut view = View {
            zoom: REGULAR,
            rect: Some(Rect::new(0.0, 0.0, 800.0, 600.0)),
            pan: Vec2::new(30.0, -12.0),
            ..default()
        };
        let middle = Vec2::new(400.0, 300.0);
        let under = view.world_at(middle);

        toggle(&mut view);
        assert_eq!(view.zoom, OUT);
        assert_eq!(view.world_at(middle), under);

        toggle(&mut view);
        assert_eq!(view.zoom, REGULAR);

        view.zoom = 2.0;
        toggle(&mut view);
        assert_eq!(view.zoom, REGULAR);
    }

    #[test]
    fn the_mode_is_the_settings() {
        let mut settings = Settings {
            enhancements: crate::files::config::Enhancements::classic(),
            ..Settings::default()
        };

        assert_eq!(Mode::of(&settings), Mode::Levels);

        settings.enhancements.smooth_zoom = true;
        assert_eq!(Mode::of(&settings), Mode::Smooth);
    }

    /// The zoom each frame's snap left, before the bound.
    #[derive(Resource, Default)]
    struct Snapped(Vec<f32>);

    #[allow(
        clippy::needless_pass_by_value,
        reason = "a Bevy system takes its parameters by value"
    )]
    fn snapped(view: Res<View>, mut snapped: ResMut<Snapped>) {
        snapped.0.push(view.zoom);
    }

    /// Wheeled out past the map: the zoom stops at the least that keeps
    /// the view on it (`View::least`, between two entries here) and
    /// stays there, the snap no longer stepping it back below each
    /// frame. A notch in leaves it.
    #[test]
    #[allow(clippy::float_cmp, reason = "set exactly")]
    fn wheeled_out_past_the_map_the_zoom_rests_at_the_least() {
        let mut app = App::new();
        let view = View {
            rect: Some(Rect::new(0.0, 0.0, 1000.0, 500.0)),
            zoom: REGULAR,
            bounds: Some(Vec2::new(1800.0, 1000.0)),
            ..default()
        };
        let least = view.least().expect("bounded");

        assert!(STEPS[0] < least && least < STEPS[1], "{least}");

        app.insert_resource(view)
            .init_resource::<Step>()
            .init_resource::<Snapped>()
            .add_systems(Update, (snap, snapped, stage::bound).chain());

        let notch = |app: &mut App, notches: f32| {
            app.world_mut()
                .resource_scope(|world, mut step: Mut<Step>| {
                    turn(&mut world.resource_mut::<View>(), &mut step, notches, None);
                });
        };

        notch(&mut app, -10.0);

        for _ in 0..3 {
            app.update();
        }

        let zoom = app.world().resource::<View>().zoom;

        assert_eq!(zoom, least);
        assert_eq!(app.world().resource::<Snapped>().0[1..], [least, least]);

        notch(&mut app, 1.0);
        app.update();

        assert_eq!(app.world().resource::<View>().zoom, STEPS[1]);
    }

    /// The zoom after a notch in, the pointer mid-map, `covered` over it.
    fn wheeled(mode: Mode, covered: Covered) -> f32 {
        let mut app = App::new();
        let mut settings = Settings::default();
        let mut window = Window::default();

        settings.enhancements.smooth_zoom = mode == Mode::Smooth;
        window.set_cursor_position(Some(Vec2::new(400.0, 300.0)));

        app.add_message::<MouseWheel>()
            .insert_resource(settings)
            .insert_resource(covered)
            .insert_resource(View {
                zoom: OUT,
                rect: Some(Rect::new(0.0, 0.0, 800.0, 600.0)),
                ..default()
            })
            .init_resource::<Step>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<crate::panel::Panels>()
            .init_resource::<bevy_egui::EguiUserTextures>()
            .add_systems(Update, systems());
        app.world_mut().spawn((window, PrimaryWindow));
        app.world_mut().write_message(MouseWheel {
            unit: bevy::input::mouse::MouseScrollUnit::Line,
            x: 0.0,
            y: 1.0,
            window: Entity::PLACEHOLDER,
            phase: bevy::input::touch::TouchPhase::Moved,
        });
        app.update();

        app.world().resource::<View>().zoom
    }

    /// A menu or the settings editor over the map takes the wheel: it
    /// scrolls them, not the map behind.
    #[test]
    #[allow(clippy::float_cmp, reason = "the levels are exact")]
    fn the_wheel_does_not_zoom_under_a_menu() {
        let whole = || Covered(vec![Rect::new(0.0, 0.0, 800.0, 600.0)]);

        for mode in [Mode::Levels, Mode::Smooth] {
            assert!(wheeled(mode, Covered::default()) > OUT, "{mode:?}");
            assert_eq!(wheeled(mode, whole()), OUT, "{mode:?}");
        }
    }

    /// A reset test's press.
    #[derive(Clone, Copy, Debug)]
    enum Press {
        Middle,
        Zero,
    }

    /// What lies over the map in a reset test.
    #[derive(Clone, Copy, Debug)]
    enum Over {
        Bare,
        /// A panel ([`Covered`]), the whole map.
        Panel,
        /// A menu or the settings editor: the whole map, and the keys.
        Menu,
        /// A text box (the chat) with the keyboard.
        Typing,
    }

    /// The zoom after `press`, smooth, the pointer mid-map, `over` it.
    fn reset_after(press: Press, over: Over) -> f32 {
        let mut app = App::new();
        let mut settings = Settings::default();
        let mut window = Window::default();
        let mut panels = crate::panel::Panels::default();
        let covered = match over {
            Over::Panel | Over::Menu => Covered(vec![Rect::new(0.0, 0.0, 800.0, 600.0)]),
            Over::Bare | Over::Typing => Covered::default(),
        };

        settings.enhancements.smooth_zoom = true;
        window.set_cursor_position(Some(Vec2::new(400.0, 300.0)));

        if matches!(over, Over::Menu) {
            panels.state.options.open = Some(crate::ui::options::Dialog::Options);
        }

        app.add_message::<MouseWheel>()
            .insert_resource(settings)
            .insert_resource(covered)
            .insert_resource(panels)
            .insert_resource(View {
                zoom: OUT,
                rect: Some(Rect::new(0.0, 0.0, 800.0, 600.0)),
                ..default()
            })
            .init_resource::<Step>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<bevy_egui::EguiUserTextures>()
            .add_systems(Update, systems());
        app.world_mut().spawn((window, PrimaryWindow));

        // The chat's line focused: egui wants the keyboard.
        if matches!(over, Over::Typing) {
            let mut egui = bevy_egui::EguiContext::default();

            egui.get_mut()
                .memory_mut(|m| m.request_focus(bevy_egui::egui::Id::new("chat")));
            app.world_mut().spawn((egui, bevy_egui::PrimaryEguiContext));
        }

        // A click: pressed one frame, released the next.
        match press {
            Press::Middle => {
                let mut buttons = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();

                buttons.press(MouseButton::Middle);
                app.update();

                let mut buttons = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();

                buttons.clear();
                buttons.release(MouseButton::Middle);
            }
            Press::Zero => app
                .world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::Digit0),
        }
        app.update();

        app.world().resource::<View>().zoom
    }

    /// A middle click resets the zoom but under a panel or menu; 0 but
    /// while a menu, the settings editor or a text box has the keyboard.
    #[test]
    #[allow(clippy::float_cmp, reason = "the levels are exact")]
    fn the_reset_is_the_bare_maps_alone() {
        let cases = [
            (Press::Middle, Over::Bare, REGULAR),
            (Press::Zero, Over::Bare, REGULAR),
            (Press::Middle, Over::Panel, OUT),
            (Press::Zero, Over::Panel, REGULAR),
            (Press::Middle, Over::Menu, OUT),
            (Press::Zero, Over::Menu, OUT),
            (Press::Zero, Over::Typing, OUT),
        ];

        for (press, over, zoom) in cases {
            assert_eq!(reset_after(press, over), zoom, "{press:?} {over:?}");
        }
    }
}
