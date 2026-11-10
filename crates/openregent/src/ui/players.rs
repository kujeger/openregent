//! A LAN game's player list in the map's top right (`[ui] player_list`,
//! this game's): each player in play, its colour, and, in `details`, its
//! round trip from this machine (`net::Node::pings`; this machine's own
//! row has none) and under the rows the lockstep delay in force here.
//!
//! ```text
//!  Vincent ■  234ms
//!      Bob ■   91ms
//!    Floof ■      -     not yet answered
//!      Ann ■            this machine's own
//!  delay 3 frames, 150 ms
//! ```

use std::sync::Arc;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};

use regent_core::view::panel::minimap::Looks;
use regent_core::view::raster::colour::quantise;
use regent_draw::data::Corpus;
use regent_draw::stage::View;

use crate::files::config::{PlayerList, Settings};
use crate::game::seen::{Listed, Ping, Watch};
use crate::net::Delay;
use crate::ui::typeface;

/// The text's size, HD pixels: under the chat box's 15, to keep the
/// map clear.
const FONT_SIZE: f32 = 10.0;

/// The rows' pitch, HD pixels.
const ROW_PITCH: f32 = 13.0;

/// The list's inset from the map's top right corner, HD pixels.
const INSET: f32 = 8.0;

/// Between the columns, HD pixels.
const GAP: f32 = 5.0;

/// The colour square's side, HD pixels.
const SWATCH: f32 = 9.0;

/// The text's shadow, and the square's border, HD pixels: legible on
/// any ground.
const SHADOW: f32 = 1.0;

/// A round trip not yet answered.
const NO_PING: &str = "-";

/// The list, while `[ui] player_list` holds in a LAN game.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn shown(
    mut contexts: EguiContexts,
    (seen, corpus, settings): (Watch, Res<Corpus>, Res<Settings>),
    (view, window): (Res<View>, Single<&Window, With<PrimaryWindow>>),
    mut looks: Local<Option<Looks>>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let mode = settings.ui.player_list;
    let shown = mode != PlayerList::Off && !seen.listed.is_empty();

    if let (true, Some(area)) = (shown, view.rect) {
        let scale = crate::ui::layout::scale(egui::vec2(window.width(), window.height()));
        let looks = looks.get_or_insert_with(|| Looks::new(&corpus.catalogue));
        let corner = egui::pos2(area.max.x - INSET * scale, area.min.y + INSET * scale);

        draw(
            &ctx,
            (corner, scale),
            (&seen.listed, seen.delay, mode),
            looks,
        );
    }

    Ok(())
}

/// A round trip as the list shows it: `234ms`, [`NO_PING`], or blank
/// for this machine's own.
fn ping_text(ping: Ping) -> String {
    match ping {
        Ping::Own => String::new(),
        Ping::Unknown => NO_PING.to_owned(),
        Ping::Ms(ms) => format!("{ms}ms"),
    }
}

/// One row's text, laid out: its shadow, and the text itself.
struct Laid {
    shade: Arc<egui::Galley>,
    ink: Arc<egui::Galley>,
}

impl Laid {
    fn of(ctx: &egui::Context, font: &egui::FontId, text: &str) -> Laid {
        let lay =
            |colour| ctx.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font.clone(), colour));

        Laid {
            shade: lay(egui::Color32::BLACK),
            ink: lay(egui::Color32::WHITE),
        }
    }

    fn width(&self) -> f32 {
        self.ink.size().x
    }

    /// Drawn with its right edge at `right`, its top at `top`.
    fn draw(&self, painter: &egui::Painter, (right, top): (f32, f32), shadow: f32) {
        let at = egui::pos2(right - self.width(), top);

        painter.galley(
            at + egui::Vec2::splat(shadow),
            self.shade.clone(),
            egui::Color32::BLACK,
        );
        painter.galley(at, self.ink.clone(), egui::Color32::WHITE);
    }
}

/// The columns' right edges and the square, from the list's top right
/// `corner`, for the widest ping `ping_width` (`None`: no ping column,
/// the square meets the corner), `scale` points an HD pixel:
///
/// ```text
///  name ■ ping
///     ^ ^    ^ corner.x
///     | square
///     names' right edge
/// ```
fn columns(corner: egui::Pos2, ping_width: Option<f32>, scale: f32) -> (f32, egui::Rect) {
    let square_right = ping_width.map_or(corner.x, |w| corner.x - w - GAP * scale);
    let square = egui::Rect::from_min_max(
        egui::pos2(square_right - SWATCH * scale, corner.y),
        egui::pos2(square_right, corner.y + SWATCH * scale),
    );

    (square.min.x - GAP * scale, square)
}

/// The delay line: "delay 3 frames, 150 ms".
fn delay_text(frames: u32) -> String {
    format!("delay {}", Delay::shown(frames))
}

/// `listed`, a row each, from the top right `corner`, `scale` points an
/// HD pixel, its colours from `looks`; `mode` `details` adds the round
/// trips and the `delay` line.
#[allow(clippy::cast_precision_loss, reason = "a few rows")]
fn draw(
    ctx: &egui::Context,
    (corner, scale): (egui::Pos2, f32),
    (listed, delay, mode): (&[Listed], u32, PlayerList),
    looks: &Looks,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("player list"),
    ));
    let font = egui::FontId::proportional(typeface::like_egui(FONT_SIZE * scale));
    let lift = typeface::lift(FONT_SIZE * scale);
    let pitch = ROW_PITCH * scale;
    let shadow = SHADOW * scale;

    let details = mode == PlayerList::Details;
    let rows: Vec<(Laid, Laid, &Listed)> = listed
        .iter()
        .map(|l| {
            let name = Laid::of(ctx, &font, &l.nick);
            let text = if details {
                ping_text(l.ping)
            } else {
                String::new()
            };
            let ping = Laid::of(ctx, &font, &text);

            (name, ping, l)
        })
        .collect();
    let ping_width = details.then(|| rows.iter().map(|(_, p, _)| p.width()).fold(0.0, f32::max));
    let (names_right, square) = columns(corner, ping_width, scale);

    for (n, (name, ping, listed)) in rows.iter().enumerate() {
        let top = corner.y + n as f32 * pitch;
        let text_top = top - lift;

        // The square stands centred on the row's text.
        let square = square.translate(egui::vec2(
            0.0,
            n as f32 * pitch + (name.ink.size().y - square.height()) / 2.0 - lift,
        ));
        let colour = u8::try_from(listed.player).map_or(egui::Color32::GRAY, |p| {
            let rgb = quantise(looks.colour(p));

            egui::Color32::from_rgb(rgb.r, rgb.g, rgb.b)
        });

        name.draw(&painter, (names_right, text_top), shadow);
        painter.rect_filled(square.expand(shadow), 0.0, egui::Color32::BLACK);
        painter.rect_filled(square, 0.0, colour);
        ping.draw(&painter, (corner.x, text_top), shadow);
    }

    if details {
        let top = corner.y + rows.len() as f32 * pitch - lift;

        Laid::of(ctx, &font, &delay_text(delay)).draw(&painter, (corner.x, top), shadow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ping_shows_in_ms_an_unknown_one_as_a_dash_and_the_own_blank() {
        assert_eq!(ping_text(Ping::Ms(234)), "234ms");
        assert_eq!(ping_text(Ping::Unknown), NO_PING);
        assert_eq!(ping_text(Ping::Own), "");
    }

    #[test]
    fn the_delay_line_names_frames_and_milliseconds() {
        assert_eq!(delay_text(3), "delay 3 frames, 150 ms");
    }

    /// Right to left from the corner: the pings' column, a gap, the
    /// square, a gap, the names' right edge; all scaled.
    #[test]
    fn the_columns_stand_right_to_left_from_the_corner() {
        let corner = egui::pos2(500.0, 40.0);
        let (names_right, square) = columns(corner, Some(30.0), 2.0);

        assert_eq!(square.max.x, 500.0 - 30.0 - GAP * 2.0);
        assert_eq!(square.width(), SWATCH * 2.0);
        assert_eq!(square.min.y, 40.0);
        assert_eq!(names_right, square.min.x - GAP * 2.0);

        // No ping column: the square meets the corner.
        let (names_right, square) = columns(corner, None, 2.0);

        assert_eq!(square.max.x, 500.0);
        assert_eq!(names_right, square.min.x - GAP * 2.0);
    }
}
