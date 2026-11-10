//! The multiplayer pages, as HD shows them (**Game**: `ui_mp_*.png`).
//!
//! ```text
//!  main menu Multiplayer (5003) ──> name prompt AP35 ──> AP42 protocol
//!   ──ACCEPT──> AP33 Multiplayer Setup ──CREATE GAME──> AP55 session ──> MX19 lobby
//!                                      └──JOIN GAME (a game listed)──────> MX19 lobby
//!   MX19 ADJUST SETTINGS ──> MX17 with its chat; START GAME ──> the game
//! ```
//!
//! **Binary**, screen switch: AP42 is mp protocol page, AP33
//! Mp games page, AP55 mp session dialog, AP73 session seed roll,
//! the lobby's loop. **Game**: `ui_mp_game_lobby.png` is
//! `MX19`'s layout (one scrolling settings box 2000); the base game's
//! `AP73` has a box a setting. **Inferred**: HD's lobby screen draws
//! `MX19`, as its other expansion panels win over the base game's.

use crate::data::fourcc::FourCc;

/// Multiplayer Protocol: PLEASE SELECT SERVICE, four radio boxes, ACCEPT
/// 5002 and CANCEL 8003. 7 modem, 8 serial and 9 LAN are enabled each by
/// a `DirectPlay` service found; 10, `GameSpy`, opens `majestyx-zone.url`.
/// The pick is kept in mp protocol pick.
pub const PROTOCOL: FourCc = FourCc(*b"AP42");

/// Controls on the protocol page (`AP42`).
pub mod protocol {
    pub const MODEM: i32 = 7;
    pub const SERIAL: i32 = 8;
    pub const LAN: i32 = 9;
    pub const INTERNET: i32 = 10;
    pub const ACCEPT: i32 = 5002;
    pub const CANCEL: i32 = 8003;
}

/// Multiplayer Setup: the open games and what to do with them.
pub const GAMES: FourCc = FourCc(*b"AP33");

/// [`GAMES`]' controls.
pub mod games {
    /// The list's heading, OPEN GAMES, is [`crate::view::menu::ids::TITLE`].
    ///
    /// The open games, a list box in `fn10`, and its scroll bar
    /// ([`crate::view::menu::ids::SCROLL_BAR`]).
    pub const LIST: i32 = 5000;

    /// CREATE GAME, hot key C: the session dialog, [`super::SESSION`].
    pub const CREATE: i32 = 5001;

    /// JOIN GAME, hot key Enter: the game listed, picked.
    pub const JOIN: i32 = 5002;

    /// CANCEL, `0x1f43`, hot key Escape.
    pub const CANCEL: i32 = 8003;

    /// The round quest pog, `INTD` 1038.
    pub use crate::view::menu::ids::POG;
}

/// The session dialog, from CREATE GAME: its name and player count.
pub const SESSION: FourCc = FourCc(*b"AP55");

/// [`SESSION`]'s controls.
pub mod session {
    /// The name's text box, `fnt2`.
    pub const NAME: i32 = 5001;

    /// The radio boxes for 2, 3 and 4 players: session.
    pub const PLAYERS: [(i32, u32); 3] = [(5002, 2), (5003, 3), (5004, 4)];

    /// ACCEPT, hot key Enter; CANCEL, `0x1f43`, hot key Escape.
    pub const ACCEPT: i32 = 5000;
    pub const CANCEL: i32 = 8003;

    /// The most characters the name takes: the box's set to 21,
    /// as the name prompt's.
    pub const LENGTH: usize = 21;

    /// `GMTX` "That session name is already in use.", for a name another
    /// open game has.
    pub const IN_USE_TEXT: usize = 306;
}

/// The lobby: the session's name, its settings, four seats and a chat.
pub const LOBBY: FourCc = FourCc(*b"MX19");

/// [`LOBBY`]'s controls (session seed roll's switch).
pub mod lobby {
    /// The title, "Session Name", `fn11`.
    pub const TITLE: i32 = 1021;

    /// SETTINGS, over the settings box, `fnt7`, and its scroll bar, over
    /// a strip of `IX22` 1001 (control 2).
    pub const SETTINGS_HEADING: i32 = 1;
    pub const SETTINGS: i32 = 2000;
    pub const SETTINGS_BAR: i32 = 2010;
    pub const SETTINGS_STRIP: i32 = 2;

    /// ADJUST SETTINGS, hot key A; LOAD GAME, hot key L.
    pub const ADJUST: i32 = 5002;
    pub const LOAD: i32 = 5008;

    /// GAME NAME, and the box under it.
    pub const GAME_NAME_HEADING: i32 = 1009;
    pub const GAME_NAME: i32 = 878;

    /// START GAME, hot key S, and Ready, hot key R, in one place: the
    /// host's and a guest's.
    pub const START: i32 = 5000;
    pub const READY: i32 = 5010;

    /// CANCEL, `0x1f43`, hot key Escape. "Start Replay", top right, is
    /// 5007.
    pub const CANCEL: i32 = 8003;

    /// Each seat's frame (`INMs` 1009 to 1012), plate (`INMs` 1004; seat
    /// 1's is control 16), check box with its status and name box; a combo
    /// box under each.
    pub const BORDERS: [i32; 4] = [2201, 2202, 2203, 2204];
    pub const PLATES: [i32; 4] = [16, 3802, 3803, 3804];
    pub const STATUS: [i32; 4] = [6701, 6702, 6703, 6704];
    pub const NAMES: [i32; 4] = [6401, 6402, 6403, 6404];
    pub const PICKS: [i32; 4] = [7401, 7402, 7403, 7404];

    /// The chat: CHAT ROOM, its box (`fnt7`, yellow) and scroll bar, the
    /// hint, the entry, SEND CHAT (hot key Enter).
    pub const CHAT_HEADING: i32 = 4;
    pub const CHAT: i32 = 5005;
    pub const CHAT_HINT: i32 = 1007;
    pub const CHAT_ENTRY: i32 = 5004;
    pub const SEND_CHAT: i32 = 5006;

    /// The round quest pog, `INTD` 1038.
    pub use crate::view::menu::ids::POG;

    /// `GMTX` "READY", "NOT READY".
    pub const READY_TEXT: usize = 315;
    pub const NOT_READY_TEXT: usize = 316;

    /// Seat `seat`'s status line: `GMTX` 284 "PLAYER ONE - HOST", 281 to
    /// 283 "PLAYER TWO - READY" on, 285 to 287 "... - NOT READY".
    #[must_use]
    pub const fn status_text(seat: usize, ready: bool) -> usize {
        const HOST: usize = 284;
        const READY: usize = 280;

        match (seat, ready) {
            (0, _) => HOST,
            (_, true) => READY + seat,
            (_, false) => HOST + seat,
        }
    }
}
