//! The earthquake's shake of the view. **Binary**:
//! each run steps the view
//! `min` to `max` pixels each way, x then y, from where it is, so it
//! wanders; the draws are on the paint stream, seeded by the clock.
//!
//! ```text
//!  Output::shakes ──> one x draw, one y draw each ──> View::pan
//! ```
//!
//! The engine shakes its four other views too, after telling them to
//! follow nothing; openregent has none.

use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use regent_core::base::rng;
use regent_draw::stage::View;

use crate::game::seen::News;

/// Paint rng's stream state, seeded from the clock on first use
/// (rng seed: the state takes the seed).
#[derive(Resource)]
pub struct Paint(u32);

impl Default for Paint {
    fn default() -> Self {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |d| d.as_millis());

        // `timeGetTime`'s word: the low 32 bits.
        Paint(u32::try_from(ms & u128::from(u32::MAX)).unwrap_or(1))
    }
}

/// Each shake the world ran, a step of the view in screen pixels.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
#[allow(clippy::cast_precision_loss, reason = "a step is tens of pixels")]
pub fn shake(mut news: ResMut<News>, mut paint: ResMut<Paint>, mut view: ResMut<View>) {
    let zoom = view.zoom;
    let step: Vec2 = std::mem::take(&mut news.shakes)
        .iter()
        .map(|s| {
            let x = s.step(rng::draw(&mut paint.0, s.bound()));
            let y = s.step(rng::draw(&mut paint.0, s.bound()));

            // Screen y runs down, the world's up.
            Vec2::new(x as f32, -(y as f32)) / zoom
        })
        .sum();

    if step != Vec2::ZERO {
        view.pan += step;
    }
}

/// The quake's paint, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Paint>();
}
