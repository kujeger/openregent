//! A LAN game held for a silent player: whom it waits on, the time left
//! before the seat is dropped, and on the host a press that drops it
//! now. HD asks its host "Drop Player?" (`GMTX` 288, 289,
//! Drop player dialog); the look here is a placeholder.
//!
//! ```text
//!  host    Haven't heard from Ann in over 5 seconds.  Should they get
//!          dropped from the game? (55 s)                [Drop Player]
//!  guest   Waiting for Ann: dropped in 55 s.
//! ```

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use regent_core::data::text::{Text, printf};
use regent_draw::data::Corpus;

use crate::game::seen::{Ask, Outbox, Watch};
use crate::net::Wait;

/// `GMTX` 288 "Drop Player?": the host's press, its mark dropped.
const DROP_TEXT: usize = 288;

/// `GMTX` 289 "Haven't heard from %s in over %d seconds.  Should they
/// get dropped from the game?": the host's question.
const ASKED_TEXT: usize = 289;

/// Either text's words without the data.
const DROP_FALLBACK: &str = "Drop Player";
const ASKED_FALLBACK: &str = "Haven't heard from %s in over %d seconds.";

/// What a guest says of a wait: a placeholder, HD has none found.
const WAITING: &str = "Waiting for %s: dropped in %d s.";

/// The time left beside the host's question.
const LEFT: &str = "(%d s)";

/// Where the box sits: down from the window's top centre, below the
/// top bar.
const AT: egui::Vec2 = egui::vec2(0.0, 96.0);

/// Who reads the box: the host may drop a seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reader {
    Host,
    Guest,
}

/// The box while the game holds: a line each seat waited on, and the
/// host's press, which drops the seat.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn shown(
    mut contexts: EguiContexts,
    corpus: Res<Corpus>,
    seen: Watch,
    mut outbox: ResMut<Outbox>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let reader = if seen.hosts {
        Reader::Host
    } else {
        Reader::Guest
    };
    let pressed = text_of(&corpus.text, DROP_TEXT, DROP_FALLBACK);
    let pressed = pressed.trim_end_matches('?');

    if !seen.waiting.is_empty() {
        egui::Area::new(egui::Id::new("waiting"))
            .anchor(egui::Align2::CENTER_TOP, AT)
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    for wait in &seen.waiting {
                        ui.label(said(wait, reader, &corpus.text));

                        if reader == Reader::Host && ui.button(pressed).clicked() {
                            outbox.push(Ask::DropPlayer(wait.seat));
                        }
                    }
                });
            });
    }

    Ok(())
}

/// `GMTX` `at`, trimmed; `fallback` without the data.
fn text_of(text: &Text<'_>, at: usize, fallback: &str) -> String {
    text.gmtx(at)
        .map_or_else(|| fallback.to_owned(), |t| t.trim().to_owned())
}

/// What `reader` is told of `wait`:
///
/// ```text
///  host   Haven't heard from Ann in over 5 seconds.  Should they get
///         dropped from the game? (55 s)
///  guest  Waiting for Ann: dropped in 55 s.
/// ```
fn said(wait: &Wait, reader: Reader, text: &Text<'_>) -> String {
    let secs = |d: std::time::Duration| u32::try_from(d.as_secs()).unwrap_or(u32::MAX);
    let left = secs(wait.left);

    match reader {
        Reader::Host => {
            let asked = text_of(text, ASKED_TEXT, ASKED_FALLBACK);
            let asked = printf(
                &asked,
                &[wait.nick.as_str().into(), secs(wait.silent).into()],
            );

            format!("{asked} {}", printf(LEFT, &[left.into()]))
        }
        Reader::Guest => printf(WAITING, &[wait.nick.as_str().into(), left.into()]),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// The host is asked in the game's words, `GMTX` 289, with the
    /// silence and the time left; a guest told whom it waits on.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_wait_is_said_in_the_games_words() {
        let text = Text::build(crate::testdata::hd());
        let wait = Wait {
            seat: 1,
            nick: "Ann".to_owned(),
            silent: Duration::from_millis(5_400),
            left: Duration::from_millis(54_600),
        };

        assert_eq!(
            said(&wait, Reader::Host, &text),
            "Haven't heard from Ann in over 5 seconds.  Should they get dropped from the game? (54 s)"
        );
        assert_eq!(
            said(&wait, Reader::Guest, &text),
            "Waiting for Ann: dropped in 54 s."
        );
        assert_eq!(text_of(&text, DROP_TEXT, DROP_FALLBACK), "Drop Player?");
    }
}
