//! The menus' videos: CREDITS, the intro and a film full screen, a
//! briefing's in its porthole (`crate::video` plays them). An end panel's
//! film plays here too, over the game (`ui::end`).
//!
//! ```text
//!  Credits(n), Intro(n), Film(f) ──full screen──> a key, a press, or its end
//!  Background(q) ──QMnn in control 8886──> plays with the page
//! ```

use bevy_egui::egui::{self, Color32, Rect, Sense, pos2, vec2};

use super::art::Art;
use super::films::Films;
use super::font::spans;
use super::page::{Page, Press};
use super::paint::{self, Canvas};
use crate::video::Playing;
use regent_core::data::fourcc::FourCc;
use regent_core::view::menu as panels;
use regent_draw::art as service;

/// The page's video: the page, its id, and its player where it plays;
/// the frame the next is held at; the Cinematics page's list.
#[derive(bevy::prelude::Resource, Default)]
pub struct Video {
    shown: Option<(Page, FourCc, Option<Playing>)>,
    /// The frame the next video is held at (`--menu PAGE@FRAME`).
    pub at: Option<usize>,
    /// What the Cinematics page lists (`Progress::films`, as it opens).
    pub films: Films,
}

impl Video {
    /// The video `page` plays: CREDITS' or the intro's `n`-th, a film's,
    /// a briefing's quest's, in `art`.
    pub fn of(art: &Art, page: Page) -> Option<FourCc> {
        use regent_core::view::menu::{CREDITS, INTRO};

        match page {
            Page::Credits(n) => CREDITS.get(n).copied(),
            Page::Intro(n) => INTRO.get(n).copied(),
            Page::Film(film) => Some(film.id()),
            Page::Background(quest) => {
                regent_core::view::menu::quest_map::video(art.book.quests.get(quest)?.slot?)
            }
            _ => None,
        }
    }

    /// `on`'s video started as it opens, the last stopped; one not
    /// shipped is kept as missing.
    pub fn play(&mut self, on: Page, art: &Art) {
        let wanted = Video::of(art, on);

        if self.shown.as_ref().map(|(p, id, _)| (*p, *id)) != wanted.map(|id| (on, id)) {
            self.shown = wanted.map(|id| {
                let (start, pace) = match self.at.take() {
                    Some(frame) => (frame, crate::video::Pace::Held),
                    None => (0, crate::video::Pace::Run),
                };
                let playing = source_of(art, on, id).and_then(|source| {
                    Playing::new(id, source, start, pace)
                        .map_err(|why| bevy::log::warn!("{why}"))
                        .ok()
                });

                (on, id, playing)
            });
        }
    }

    /// The page's video: its id and its player, where it plays.
    pub fn shown(&self) -> Option<(FourCc, Option<&Playing>)> {
        self.shown.as_ref().map(|(_, id, p)| (*id, p.as_ref()))
    }

    /// The video playing, if any.
    pub fn playing(&self) -> Option<&Playing> {
        self.shown.as_ref().and_then(|(_, _, p)| p.as_ref())
    }

    /// The video playing, to take its sound (`video::sound`).
    pub fn playing_mut(&mut self) -> Option<&mut Playing> {
        self.shown.as_mut().and_then(|(_, _, p)| p.as_mut())
    }
}

/// Where `page`'s video `id` is, in `art`: the intro's and a film's in
/// the cinematics' archives (`menu::CINEMATICS`); the rest in the
/// datasets'.
pub(super) fn source_of(art: &Art, page: Page, id: FourCc) -> Option<crate::video::VideoSource> {
    let archive = match page {
        Page::Intro(_) => art.cinematics.first(),
        Page::Film(film) => art.cinematics.get(film.archive()),
        _ => None,
    };

    match archive {
        Some(path) => Some(crate::video::VideoSource::Archive(path.clone())),
        None => art
            .videos
            .get(&id)
            .map(|&record| crate::video::VideoSource::Record(record)),
    }
}

/// The full-screen videos' size, and the screen mode the original sets
/// for them: 640×480.
const FULL_SCREEN: egui::Vec2 = vec2(640.0, 480.0);

/// What shows in place of a video that will not play.
const MISSING: &str = "Video %s: not found, or it will not decode.";

/// The note's second line.
const NEXT: &str = "A key or a click goes on.";

/// The note's line pitch, layout pixels: any that parts two lines.
const PITCH: f32 = 24.0;

/// A full-screen video `id`, `playing` if it plays: the window black,
/// the video scaled to fill it, its shape kept. **Inferred** stand-in
/// for the original's 640×480 screen mode. A key or a press stops it, as
/// one stops the video; its end too.
pub fn full_screen(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    (id, playing): (FourCc, Option<&Playing>),
    layer: Layer,
) -> Option<Press> {
    let Art { ink, .. } = art;
    let mut canvas = match layer {
        Layer::Menus => Canvas::filling(ctx, (ink, shared), FULL_SCREEN),
        Layer::OverGame => Canvas::over(ctx, (ink, shared), FULL_SCREEN),
    };
    let window = canvas.ui.max_rect();

    canvas.ui.painter().rect_filled(window, 0.0, Color32::BLACK);

    match playing {
        Some(video) => {
            if let Some(frame) = video.frame() {
                let screen = canvas
                    .fit
                    .rect(Rect::from_min_size(pos2(0.0, 0.0), FULL_SCREEN));
                let (texture, screen) = frame.at(ctx.pixels_per_point(), screen);

                canvas
                    .ui
                    .painter()
                    .image(texture, screen, unit(), Color32::WHITE);
            }
        }
        None => note(&mut canvas, id),
    }

    let pressed = canvas
        .ui
        .interact(window, canvas.ui.id().with("credits"), Sense::click())
        .clicked();
    let keyed = ctx.input(|i| {
        i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key {
                    pressed: true,
                    repeat: false,
                    ..
                }
            )
        })
    });
    let ended = playing.is_some_and(Playing::ended);

    if pressed || keyed {
        Some(Press::Back)
    } else {
        ended.then_some(Press::Ended)
    }
}

/// Where a full-screen video draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Menus,
    /// Over the game's interface: an end panel's film.
    OverGame,
}

/// The whole texture.
const fn unit() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
}

/// Two lines about the middle of `FULL_SCREEN`: video `id` missing.
fn note(canvas: &mut Canvas, id: FourCc) {
    let lines = [MISSING.replace("%s", &id.to_string()), NEXT.to_owned()];

    for (n, line) in lines.iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "two lines")]
        let top = FULL_SCREEN.y / 2.0 + (n as f32 - 1.0) * PITCH;
        let rect = Rect::from_min_max(pos2(0.0, top), pos2(FULL_SCREEN.x, top + PITCH));

        canvas.centred(Some(panels::SMALL_FONT), rect, &spans(line, paint::PALE));
    }
}
