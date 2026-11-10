//! The game speed's setting (`[game] speed`, a `runtime::PACES` slider
//! position) applied to the clock ([`Speedup`]), and the speed keys.
//!
//! ```text
//!  a game's first frame ──> Speedup: --speed, else the setting
//!  the setting changed (the editor, < >) ──> Speedup
//!  < > ──> Speedup stepped along PACES, the setting following
//!  1 2 3 ──> Speedup 1×, 2×, 4× (`[enhancements] speed_keys`)
//!  a LAN game: --speed or the engine's at the first frame; then neither
//!              the setting nor any key change it
//! ```
//!
//! The original starts each game at the default pace and keeps the
//! slider's for the session alone; a departure. In a
//! network game it refuses `<` `>` without a word.

use bevy::prelude::*;
use regent_core::gpl::runtime::{self, SpeedKey};

use crate::files::config::{Settings, SpeedSlider};
use crate::ui;
use crate::ui::options::Play;

/// `--speed`: each game starts at it, not at the setting.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct Given(pub Option<f32>);

/// The setting the clock last took; none before a game's first frame.
#[derive(Resource, Default)]
pub struct Applied(Option<SpeedSlider>);

/// The setting onto the clock, as a game starts and as it changes.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn apply(
    (settings, given): (Res<Settings>, Res<Given>),
    frame: Option<Res<crate::game::seen::Frame>>,
    mut applied: ResMut<Applied>,
    mut speed: ResMut<Speedup>,
) {
    let now = settings.game.speed;
    let play = Play::of(frame.map_or(1, |f| f.players));

    if applied.0 != Some(now) {
        if let Some(taken) = taken(applied.0, (now, given.0), play) {
            speed.0 = taken;
        }

        applied.0 = Some(now);
    }
}

/// The clock's speed as the setting turns `now` from `was` (none: the
/// game's first frame), `--speed` `given`, in a game `play`; `None`,
/// the clock left as it is.
fn taken(
    was: Option<SpeedSlider>,
    (now, given): (SpeedSlider, Option<f32>),
    play: Play,
) -> Option<f32> {
    match (was, given, play) {
        (None, Some(given), _) => Some(given),
        (None, None, Play::Lan) => Some(SpeedSlider::default().factor()),
        (Some(_), _, Play::Lan) => None,
        (_, _, Play::Alone) => Some(now.factor()),
    }
}

/// `<` or `>` (`way`) in a game `play`: alone, the clock's pace stepped
/// and the setting following, as the original's slider shows the key's
/// pace, not written; in a LAN game,
/// nothing.
pub fn key(way: SpeedKey, play: Play, (speed, settings): (&mut Speedup, &mut Settings)) {
    if play == Play::Alone {
        speed.0 = paced(speed.0, way);

        if let Some(now) = SpeedSlider::of_factor(speed.0) {
            settings.game.speed = now;
        }
    }
}

/// The speed factor after a speed key at `factor`: its pace stepped
/// (`runtime::stepped`), as a factor of the default pace.
fn paced(factor: f32, way: SpeedKey) -> f32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a pace is a few hundred ms"
    )]
    let pace = (runtime::DEFAULT_PACE as f32 / factor).round() as u32;

    #[allow(clippy::cast_precision_loss, reason = "a pace is a few hundred ms")]
    let stepped = runtime::DEFAULT_PACE as f32 / runtime::stepped(pace, way) as f32;

    stepped
}

/// The setting a game's.
pub fn plugin(app: &mut App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Applied>();
}

/// What the sim thread's clock runs at: times the engine's speed (`--speed`, the
/// speed keys). Not a game's: the next game keeps it.
#[derive(Resource, Clone, Copy)]
pub(crate) struct Speedup(pub(crate) f32);

/// The speeds keys 1, 2 and 3 pick, an enhancement
/// (`[enhancements] speed_keys`): the original's options offer
/// `runtime::PACES` instead.
#[derive(Clone, Copy)]
enum SpeedPreset {
    Normal,
    Double,
    Quadruple,
}

impl SpeedPreset {
    /// Each speed and its key.
    const KEYS: [(KeyCode, SpeedPreset); 3] = [
        (KeyCode::Digit1, SpeedPreset::Normal),
        (KeyCode::Digit2, SpeedPreset::Double),
        (KeyCode::Digit3, SpeedPreset::Quadruple),
    ];

    /// Times the engine's default, as `--speed`.
    const fn factor(self) -> f32 {
        match self {
            SpeedPreset::Normal => 1.0,
            SpeedPreset::Double => 2.0,
            SpeedPreset::Quadruple => 4.0,
        }
    }
}

/// Keys 1, 2 and 3 set the game speed, when enabled. A stopped game
/// stays stopped (`game::thread::Clock::rate`). In a LAN game they do
/// nothing, as `<` `>` do not (`game::speed::key`).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub(crate) fn speed_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut live: crate::input::map_keys::Live,
    (settings, frame): (Res<Settings>, Option<Res<crate::game::seen::Frame>>),
    mut speed: ResMut<Speedup>,
) {
    let picked = SpeedPreset::KEYS
        .iter()
        .find(|(key, _)| keys.just_pressed(*key))
        .map(|(_, speed)| speed.factor());
    let play = ui::options::Play::of(frame.map_or(1, |f| f.players));

    if let Some(picked) = picked
        && settings.enhancements.speed_keys
        && play == ui::options::Play::Alone
        && live.keys()
    {
        speed.0 = picked;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clock's speed after a frame at `setting`, then after one at
    /// `changed`, `--speed` `given`.
    fn speeds(given: Option<f32>, setting: SpeedSlider, changed: SpeedSlider) -> [f32; 2] {
        let mut app = App::new();
        let mut settings = Settings::default();

        settings.game.speed = setting;
        app.insert_resource(settings)
            .insert_resource(Given(given))
            .insert_resource(Speedup(1.0))
            .init_resource::<Applied>()
            .add_systems(Update, apply);

        app.update();

        let first = app.world().resource::<Speedup>().0;

        app.world_mut().resource_mut::<Settings>().game.speed = changed;
        app.update();

        [first, app.world().resource::<Speedup>().0]
    }

    /// A game starts at the setting, or `--speed`; a change of the
    /// setting applies at once.
    #[test]
    #[allow(clippy::float_cmp, reason = "the same division each side")]
    fn a_game_starts_at_the_setting_and_follows_it() {
        assert_eq!(speeds(None, SpeedSlider::S9, SpeedSlider::S1), [5.0, 0.2]);
        assert_eq!(
            speeds(Some(3.0), SpeedSlider::S9, SpeedSlider::S1),
            [3.0, 0.2]
        );
        assert_eq!(
            speeds(Some(3.0), SpeedSlider::S9, SpeedSlider::S9),
            [3.0, 3.0]
        );
    }

    /// A LAN game starts at `--speed` or the engine's; a change of the
    /// setting leaves it.
    #[test]
    #[allow(clippy::float_cmp, reason = "the same division each side")]
    fn a_lan_game_takes_no_speed_setting() {
        let lan = |was, given| taken(was, (SpeedSlider::S9, given), Play::Lan);

        assert_eq!(lan(None, None), Some(1.0));
        assert_eq!(lan(None, Some(2.0)), Some(2.0));
        assert_eq!(lan(Some(SpeedSlider::S5), None), None);
        assert_eq!(
            taken(Some(SpeedSlider::S5), (SpeedSlider::S9, None), Play::Alone),
            Some(5.0)
        );
    }

    #[test]
    #[allow(clippy::float_cmp, reason = "the same division each side")]
    fn a_speed_key_steps_the_pace_as_a_factor() {
        assert_eq!(paced(1.0, SpeedKey::Slower), 50.0 / 85.0);
        assert_eq!(paced(1.0, SpeedKey::Faster), 50.0 / 40.0);
        assert_eq!(paced(5.0, SpeedKey::Faster), 5.0);
        // `--speed 3`: no pace of the table, so the default.
        assert_eq!(paced(3.0, SpeedKey::Faster), 1.0);
    }

    /// `<` `>` alone step the clock and the setting; in a LAN game they
    /// do nothing.
    #[test]
    #[allow(clippy::float_cmp, reason = "the same division each side")]
    fn the_speed_keys_step_alone_and_do_nothing_in_a_lan_game() {
        let pressed = |play| {
            let (mut speed, mut settings) = (Speedup(1.0), Settings::default());

            key(SpeedKey::Faster, play, (&mut speed, &mut settings));
            (speed.0, settings.game.speed)
        };

        assert_eq!(pressed(Play::Alone), (50.0 / 40.0, SpeedSlider::S6));
        assert_eq!(pressed(Play::Lan), (1.0, SpeedSlider::S5));
    }
}
