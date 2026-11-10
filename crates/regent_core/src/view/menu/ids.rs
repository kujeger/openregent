//! Control ids many panels share, one role each in every panel that
//! holds them (`SMNU` key 6, [`crate::data::smnu::CONTROL`]).
//!
//! ```text
//!  8005  Textbox or Button   the heading: a menu's title, a selection
//!                            panel's name, a page's     100 + 5 panels
//!  8730  Button              the round quest pog         19 panels
//!  9101  Scrollbar           the list's or text's bar    11 panels
//!  8000  Button              zoom to the selection       77 panels
//!  8001  Button              track the selection         77 panels
//! ```
//!
//! Counted over the base dataset's panels (`regent_cli menu --record`); the
//! selection panels and the pages below share 8000 and 8001. Two
//! exceptions keep their own constants: `AP78`'s 8730 is the hero's
//! spell list, `AP64`'s 8001 the high scores'
//! list ([`super::scores::id::LIST`]). The Build Tree Editor's 8005 is the "SAVE AS:" label, its title
//! 8 ([`super::tree::NAME_LABEL`]).

/// A panel's heading, `0x1f45`: a menu's title ("High Scores",
/// quest title on the end pages), "MESSAGE", a selection
/// panel's name (`StatsWindowController`, a press renames), a
/// page's name.
pub const TITLE: i32 = 8005;

/// The round quest pog at a title bar's left, `0x221a`;
/// a panel without a
/// quest keeps its own picture.
pub const POG: i32 = 8730;

/// A panel's scroll bar, `0x238d`, bound to its list, text or chat.
pub const SCROLL_BAR: i32 = 9101;

/// Zoom, `0x1f40`: centres the main map on the selection, on its panel
/// and on every page below (`INTG` 1003, "Zoom the Main Map to this
/// ..").
pub const ZOOM: i32 = 8000;

/// Track, `0x1f41`: the tracking window follows the selection
/// (`StatsWindowController`, view follow; `INTG` 1000).
pub const TRACK: i32 = 8001;
