//! A selected message flag's panel, the message scroll: `SMNU` `AP77`
//! in the selection slot, run by `EventMessageController`. **Binary**.
//!
//! ```text
//!  ┌──────── MESSAGE ────────┐  8005 the panel's own label
//!  │  text 5000, AITX[APM:0] │▲ 5053 scroll bar, tied to 5000
//!  │  centred, scrolled      │▼
//!  │ [5005]  [ CLOSE ]  [6]  │  check, 5503, button 6
//!  └─────────────────────────┘
//!      (EventMessageController)  5000: APM:0 0 clears it, else activity text
//!                    5005: the mini view option (message mini view)
//!      (EventMessageController)  6     centre the view on the flag (view center unit),
//!                          then as 5503
//!                    5503  ME01 Message_OK, Begin; DestroyEventFlagMessage
//!                          (event flag destroy send), as a double click reads
//!                    5005  flips the option
//! ```
//!
//! No history: the side controls are the mini view option and a zoom.

use std::borrow::Cow;

use crate::data::fourcc::FourCc;
use crate::data::text::Text;
use crate::sim::world::sound::{self, BEGIN, Voice};

/// The message scroll's panel.
pub const PANEL: FourCc = FourCc(*b"AP77");

/// `AP77`'s controls.
pub mod id {
    /// The message, `fnt4`, colour `0x3f0000`.
    pub const TEXT: i32 = 5000;
    /// "MESSAGE", the panel's own text; the controller leaves it.
    pub use crate::view::menu::ids::TITLE;
    /// The text's scroll bar.
    pub const SCROLL: i32 = 5053;
    /// CLOSE: "Dismiss this message scroll."
    pub const CLOSE: i32 = 5503;
    /// "Zoom to and close this message scroll."
    pub const ZOOM: i32 = 6;
    /// "Toggles whether message scrolls zoom to the tracking window."
    pub const FOLLOW: i32 = 5005;
}

/// What CLOSE and the zoom play: `ME01 Message_OK`, phase `Begin`,
/// unplaced.
pub const CLOSE_SOUND: FourCc = FourCc(*b"ME01");

/// [`CLOSE_SOUND`] as the interface plays it.
#[must_use]
pub const fn closed() -> (Voice, sound::Phase) {
    (Voice::Sound(CLOSE_SOUND), sound::Phase::Id(BEGIN))
}

/// The mini view option's default: on.
pub const FOLLOW_DEFAULT: bool = true;

/// Message `message`'s text: none for 0, else its `AITX` entry.
#[must_use]
pub fn text<'a>(text: &Text<'a>, message: i32) -> Option<Cow<'a, str>> {
    if message == 0 {
        None
    } else {
        text.activity(message)
    }
}
