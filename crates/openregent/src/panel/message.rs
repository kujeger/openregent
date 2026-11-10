//! The message scroll, `AP77` in the selection slot: a selected message
//! flag's text and what its buttons ask (`regent_core::view::panel::message`).
//!
//! ```text
//!  5000  AITX[APM:0], centred, wrapped, scrolled; arrows only when long
//!  5503  CLOSE ──> read: ME01, Input::Read, the panel gone
//!  6     zoom  ──> the view on the flag, then as CLOSE
//!  5005  the tracking window's option: a new message flag followed
//! ```

use regent_core::sim::world::World;
use regent_core::view::menu::Tick;
use regent_core::view::panel::message::{self, FOLLOW_DEFAULT, PANEL, id};
use regent_draw::data::Corpus;

use super::canvas::{Align, Canvas};
use super::sheet::{self, Able};
use super::upper::{Asked, Game};
use super::{Panels, bars, lower};

/// Controls drawn by hand, not as statics.
const OWN: [i32; 5] = [id::TEXT, id::SCROLL, id::CLOSE, id::ZOOM, id::FOLLOW];

/// The scroll's place, pixels, kept for one message flag.
#[derive(Default)]
pub struct Scroll {
    of: Option<u32>,
    by: f32,
}

/// A message flag's panel, `EventMessageController`.
pub struct Message {
    handle: u32,
    text: String,
    /// The tracking window's option, as it stands.
    follows: bool,
}

impl Message {
    /// Message flag `handle`, while it stands.
    pub fn of(corpus: &Corpus, world: &World, handle: u32) -> Option<Message> {
        let message = world.messages().into_iter().find(|m| m.handle == handle)?;
        let text = message::text(&corpus.text, message.text)
            .map(|t| t.trim().to_owned())
            .unwrap_or_default();

        Some(Message {
            handle,
            text,
            follows: FOLLOW_DEFAULT,
        })
    }

    /// With the tracking window's option `on` or off.
    #[must_use]
    pub fn following(self, on: bool) -> Message {
        Message {
            follows: on,
            ..self
        }
    }

    /// In `AP77`: the scroll, its title and text; CLOSE and the zoom
    /// read the flag, the zoom centring the view on it first.
    pub fn show(&self, canvas: &mut Canvas<'_>, panels: &mut Panels, game: &Game<'_>) -> Asked {
        let mut asked = Asked::default();
        let Some(layout) = panels.layout(&game.corpus.catalogue, &game.corpus.text, PANEL) else {
            return asked;
        };

        sheet::statics(canvas, &layout, None, &|c| OWN.contains(&c));

        // A new flag starts at the top.
        let scroll = &mut panels.state.message;

        if scroll.of != Some(self.handle) {
            *scroll = Scroll {
                of: Some(self.handle),
                by: 0.0,
            };
        }

        if let Some(c) = layout.control(id::TEXT)
            && let (Some(r), Some(font)) = (sheet::rect(c), c.font)
        {
            let tall = canvas.paragraphs(
                font,
                r,
                &self.text,
                (sheet::colour(c), Align::Centre, scroll.by),
            );
            // **Inferred**: no arrows while the text fits (`ui_message.png`).
            let bar = layout
                .control(id::SCROLL)
                .and_then(sheet::rect)
                .filter(|_| tall > r.height());

            lower::scrolled(canvas, &mut scroll.by, bar, (r, tall));
        }

        if let Some(c) = layout.control(id::FOLLOW) {
            let tick = if self.follows {
                Tick::Checked
            } else {
                Tick::Clear
            };

            if bars::toggle(canvas, c, tick) {
                asked.follows = Some(!self.follows);
            }
        }

        let close = layout
            .control(id::CLOSE)
            .is_some_and(|c| sheet::button(canvas, c, None, Able::Enabled));
        let zoom = layout
            .control(id::ZOOM)
            .is_some_and(|c| sheet::button(canvas, c, None, Able::Enabled));

        if zoom {
            asked.zoom = game.world.message_point(self.handle);
        }

        if close || zoom {
            asked.read = Some(self.handle);
        }

        asked
    }
}
