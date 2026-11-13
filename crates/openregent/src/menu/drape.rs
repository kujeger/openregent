//! The loading page over a game just put in, while its drawing's
//! start-up work settles (`regent_draw::settle`): the world draws under
//! it, so that work is done before play. A new game takes its first
//! steps under it, the start's ground, scatter and fog work with them;
//! its clock does not start (`game::clock::tick`), no input reaches it,
//! no game sound starts: the steps' sounds play as the page goes. A
//! held game ([`Hold`]) then offers Start Quest; another's page goes by
//! itself. A LAN game is never draped: its peers start as before.
//!
//! ```text
//!  begin: made ──> draw::install ──> Drape up ──> Screen::Playing: Settle restarted
//!  the page's second frame on: a new game steps to PRIMING frames, clock held
//!  Last, each frame: those steps taken and the drawing settled, or up GIVE_UP:
//!      Hold::Free    ──> down
//!      Hold::Making  ──> Hold::Made: Start Quest offered
//!  the egui pass: the page over the game; Start Quest ──> Hold::Free
//!  down: the clock starts at the next tick; input and game sound on
//! ```

use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy_egui::EguiContexts;
use regent_draw::settle::Settle;

use super::cinema::Layer;
use super::paint::{self, Offer};
use super::{Art, Hold, Menu, Quests, Start};
use crate::audio::ClickSounds;
use crate::game::seen::Frame;
use crate::game::thread::PRIMING;

/// How long the page waits for the drawing to settle before it goes
/// anyway: a pipeline whose shader never loads stays pending.
const GIVE_UP: Duration = Duration::from_secs(30);

/// The loading page over a game just put in; `None` once it went.
#[derive(Resource, Default)]
pub struct Drape(Option<Up>);

/// A page up: since when, for how many drawn frames.
struct Up {
    since: Instant,
    frames: u32,
}

impl Drape {
    /// Up from now.
    pub fn up() -> Drape {
        Drape(Some(Up {
            since: Instant::now(),
            frames: 0,
        }))
    }

    /// Whether the page covers the game.
    pub const fn is_up(&self) -> bool {
        self.0.is_some()
    }

    /// Drawn frames the page has been up, as `lift` last counted; `None`
    /// once it went.
    pub fn frames(&self) -> Option<u32> {
        self.0.as_ref().map(|up| up.frames)
    }
}

/// Whether no loading page covers the game: a run condition.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn undraped(drape: Option<Res<Drape>>) -> bool {
    drape.is_none_or(|d| !d.is_up())
}

/// The wait for the drawing begun with the game's first drawn frame,
/// under the page.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn began(drape: Res<Drape>, mut settle: ResMut<Settle>) {
    if drape.is_up() {
        settle.restart();
    }
}

/// The page's next step, once the drawing settled or the wait passed
/// [`GIVE_UP`]: a free game's page goes, a held one's offers Start
/// Quest; pressed, it goes.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn lift(
    mut drape: ResMut<Drape>,
    mut menu: ResMut<Menu>,
    (settle, frame): (Res<Settle>, Res<Frame>),
) {
    if let Some(up) = &mut drape.0 {
        up.frames += 1;

        // A new game's first steps taken and drawn: its start's work.
        let primed = frame.frames >= PRIMING;
        let late = up.since.elapsed() >= GIVE_UP;
        let done = settle.settled() && primed;
        let ready = done || late;
        let hold = match &mut menu.start {
            Some(Start::Shown(_, _, hold)) => Some(hold),
            _ => None,
        };
        let gave_up = || {
            if !done {
                warn!(
                    "game: unsettled after {GIVE_UP:?}, at frame {}: {:?}",
                    frame.frames,
                    settle.pending()
                );
            }
        };

        match (ready, hold) {
            (true, Some(hold @ Hold::Making)) => {
                gave_up();
                *hold = Hold::Made;
            }
            (true, Some(Hold::Free) | None) => {
                let ms = up.since.elapsed().as_secs_f64() * 1e3;

                gave_up();
                info!("game: loading page up {ms:.0} ms, {} frames", up.frames);
                crate::perf::lifted(ms, up.frames);
                drape.0 = None;
            }
            (true, Some(Hold::Made)) | (false, _) => {}
        }
    }
}

/// The loading page over the game, as the menus showed it while it was
/// made: Start Quest once offered, its click heard.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn shown(
    mut contexts: EguiContexts,
    (mut menu, mut art, quests): (ResMut<Menu>, ResMut<Art>, Res<Quests>),
    mut shared: ResMut<regent_draw::art::Art>,
    mut clicks: ResMut<ClickSounds>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    if let Some(wave) = paint::click(ctx).and_then(|w| art.clicks.get(&w).copied()) {
        clicks.0.push(wave);
    }

    match &mut menu.start {
        Some(Start::Shown(_, Some(shows), hold)) => {
            let offer = match hold {
                Hold::Made => Offer::StartQuest,
                Hold::Free | Hold::Making => Offer::Nothing,
            };
            let page = (*shows, offer);
            let pressed = paint::loading(
                ctx,
                (&mut art, &mut shared),
                page,
                &quests.0,
                Layer::OverGame,
            );

            if pressed {
                *hold = Hold::Free;
            }
        }
        _ => paint::cover(ctx),
    }

    Ok(())
}
