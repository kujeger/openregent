//! A LAN game held for silent players: whom it waits on, the time left
//! before each seat is dropped, and on the host a press a seat that
//! drops it now. HD asks its host "Drop Player?" (`GMTX` 288, 289,
//! Drop player dialog) a box a player; here one box, in `AP11`'s art
//! (`notice`), asks once and has a row a player, growing with them.
//!
//! ```text
//!  host   ┌────────── Drop Player? ──────────┐
//!         │ Haven't heard from these players │   GMTX 289, "these players"
//!         │ in over 5 seconds.  Should they  │   for %s past one
//!         │ get dropped from the game?       │
//!         │ ──────────────────────────────── │
//!         │ Ann            [ Drop Player ]   │   a row a seat
//!         │ drops in 55 s                    │
//!         └──────────────────────────────────┘
//!  guest  ┌─── Waiting for Other Players ────┐   GMTX 345
//!         │ Waiting for Ann: dropped in 55 s.│   a line a seat
//!         └──────────────────────────────────┘
//! ```

use bevy::prelude::*;
use bevy_egui::EguiContexts;

use regent_core::data::text::{Text, printf};
use regent_core::view::panel::scroll::WAITING;
use regent_draw::art::Art;
use regent_draw::data::Corpus;

use super::notice::{self, Ink, Item, Means, Notice, Place, Pressed};
use crate::game::seen::{Ask, Outbox, Watch};
use crate::net::Wait;
use crate::panel::Panels;
use crate::ui::Covered;

/// `GMTX` 288 "Drop Player?": the host's title, and its press with its
/// mark dropped.
const DROP_TEXT: usize = 288;

/// `GMTX` 289 "Haven't heard from %s in over %d seconds.  Should they
/// get dropped from the game?": the host's question.
const ASKED_TEXT: usize = 289;

/// Either text's words without the data, and `GMTX` 345's.
const DROP_FALLBACK: &str = "Drop Player?";
const ASKED_FALLBACK: &str = "Haven't heard from %s in over %d seconds.";
const WAITING_FALLBACK: &str = "Waiting for Other Players";

/// Who the host's question names past one silent player: openregent's
/// own, for `GMTX` 289's `%s`.
const THESE_PLAYERS: &str = "these players";

/// What a guest says of a wait: a placeholder, HD has none found.
const SAID_WAITING: &str = "Waiting for %s: dropped in %d s.";

/// The time left on a host's row: a placeholder.
const LEFT: &str = "drops in %d s";

/// Between the question and the rows, and between a guest's lines,
/// pixels: openregent's own.
const GAP: f32 = 6.0;

/// Who reads the box: the host may drop a seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reader {
    Host,
    Guest,
}

/// The box while the game holds: a line or row each seat waited on,
/// and the host's presses, each dropping its seat.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn shown(
    mut contexts: EguiContexts,
    corpus: Res<Corpus>,
    seen: Watch,
    mut outbox: ResMut<Outbox>,
    (mut panels, mut art, mut covered): (ResMut<Panels>, ResMut<Art>, ResMut<Covered>),
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let reader = if seen.hosts {
        Reader::Host
    } else {
        Reader::Guest
    };

    if !seen.waiting.is_empty() {
        let said = said(&seen.waiting, reader, &corpus.text);
        let means = Means {
            panels: &mut panels,
            art: &mut art,
            corpus: &corpus,
        };

        if let Some((pressed, rect)) = notice::show(&ctx, means, &said, (Place::Top, "held")) {
            // A row's press drops its seat.
            if let Some(Pressed::Row(n)) = pressed
                && let Some(wait) = rows(&seen.waiting, &said).get(n).copied().flatten()
            {
                outbox.push(Ask::DropPlayer(wait.seat));
            }

            covered
                .0
                .push(Rect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y));
        }
    }

    Ok(())
}

/// The waits whose rows are `said`'s items, by item: `None` where an
/// item is no row.
fn rows<'a>(waiting: &'a [Wait], said: &Notice) -> Vec<Option<&'a Wait>> {
    let mut waits = waiting.iter();

    said.items
        .iter()
        .map(|item| match item {
            Item::Row { .. } => waits.next(),
            _ => None,
        })
        .collect()
}

/// `GMTX` `at`, trimmed; `fallback` without the data.
fn text_of(text: &Text<'_>, at: usize, fallback: &str) -> String {
    text.gmtx(at)
        .map_or_else(|| fallback.to_owned(), |t| t.trim().to_owned())
}

/// Whole seconds of `d`.
fn secs(d: std::time::Duration) -> u32 {
    u32::try_from(d.as_secs()).unwrap_or(u32::MAX)
}

/// What `reader` is told of `waiting`:
///
/// ```text
///  host   Drop Player?  Haven't heard from Ann in over 5 seconds.  Should
///         they get dropped from the game?  Ann | drops in 55 s | Drop Player
///  guest  Waiting for Other Players  Waiting for Ann: dropped in 55 s.
/// ```
fn said(waiting: &[Wait], reader: Reader, text: &Text<'_>) -> Notice {
    match reader {
        Reader::Host => {
            let title = text_of(text, DROP_TEXT, DROP_FALLBACK);
            let press = title.trim_end_matches('?').to_owned();
            let silent = waiting
                .iter()
                .map(|w| secs(w.silent))
                .min()
                .unwrap_or_default();
            let who = match waiting {
                [one] => one.nick.as_str(),
                _ => THESE_PLAYERS,
            };
            let asked = printf(
                &text_of(text, ASKED_TEXT, ASKED_FALLBACK),
                &[who.into(), silent.into()],
            );
            let rows = waiting.iter().flat_map(|w| {
                [
                    Item::Row {
                        name: w.nick.clone(),
                        said: printf(LEFT, &[secs(w.left).into()]),
                        press: Some(press.clone()),
                    },
                    Item::Rule,
                ]
            });
            let items = [Item::Text(asked, Ink::Plain), Item::Gap(GAP), Item::Rule]
                .into_iter()
                .chain(rows)
                .collect();

            Notice {
                title,
                items,
                button: None,
            }
        }
        Reader::Guest => {
            let title = text_of(text, WAITING, WAITING_FALLBACK);
            let lines = waiting.iter().enumerate().flat_map(|(n, w)| {
                let line = printf(SAID_WAITING, &[w.nick.as_str().into(), secs(w.left).into()]);
                let gap = (n > 0).then_some(Item::Gap(GAP));

                gap.into_iter().chain([Item::Text(line, Ink::Plain)])
            });

            Notice {
                title: title.trim_matches(['(', ')']).to_owned(),
                items: lines.collect(),
                button: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn wait(seat: u32, nick: &str, left: u64) -> Wait {
        Wait {
            seat,
            nick: nick.to_owned(),
            silent: Duration::from_millis(5_400),
            left: Duration::from_millis(left),
        }
    }

    /// The host is asked in the game's words, `GMTX` 288 and 289, with
    /// the silence; a row gives the time left. A guest is told whom it
    /// waits on under `GMTX` 345.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_wait_is_said_in_the_games_words() {
        let text = Text::build(crate::testdata::hd());
        let waits = [wait(1, "Ann", 54_600)];
        let host = said(&waits, Reader::Host, &text);
        let guest = said(&waits, Reader::Guest, &text);

        assert_eq!(host.title, "Drop Player?");
        assert_eq!(
            host.items[0],
            Item::Text(
                "Haven't heard from Ann in over 5 seconds.  Should they get dropped from the game?"
                    .to_owned(),
                Ink::Plain
            )
        );
        assert_eq!(
            host.items[3],
            Item::Row {
                name: "Ann".to_owned(),
                said: "drops in 54 s".to_owned(),
                press: Some("Drop Player".to_owned()),
            }
        );
        assert_eq!(guest.title, "Waiting for Other Players");
        assert_eq!(
            guest.items,
            [Item::Text(
                "Waiting for Ann: dropped in 54 s.".to_owned(),
                Ink::Plain
            )]
        );
    }

    /// Past one silent player the question names none, and each has a
    /// row, in the order waited on.
    #[test]
    fn several_waits_are_asked_once_with_a_row_each() {
        let catalogue = regent_core::data::catalogue::Catalogue::build(&[]).expect("empty");
        let text = Text::build(&catalogue);
        let waits = [wait(1, "Ann", 55_000), wait(3, "Bob", 31_000)];
        let host = said(&waits, Reader::Host, &text);
        let seats: Vec<u32> = rows(&waits, &host)
            .into_iter()
            .flatten()
            .map(|w| w.seat)
            .collect();

        assert!(matches!(&host.items[0], Item::Text(t, _) if t.contains(THESE_PLAYERS)));
        assert_eq!(seats, [1, 3]);
        assert_eq!(
            rows(&waits, &host).iter().position(Option::is_some),
            Some(3)
        );
    }
}
