//! The front end's panels: the main menu and a quest's briefing.
//!
//! ```text
//!  main menu  AP29  ── Play Game ──▶ quest map (AP30) ──▶ briefing  APc9
//!                                                          │ Continue
//!                                                          ▼
//!                                                         summary   AP44
//!                                                          │ Start Quest
//!                                                          ▼
//!                                                         the game
//! ```
//!
//! A panel is an `SMNU` record and the `STRT` table of the same id
//! ([`crate::data::smnu`]); [`layout()`] resolves both against a catalogue.
//!
//! The group also holds what panels share in and out of play (controls,
//! list boxes, hot keys, `IMAG` fonts) and the pages around a game: the
//! quest map, the game's end, the high scores, saves and the autosave.
//!
//! **Game**: the captures show the base game's panels.
//! `menu_main.png` (800 by 600) is `textdata.cam`'s `AP29`: its room is
//! `INDm` 1009; the expansion `AP29`'s, `IX51` 1000, has a snowy window.
//! The Barren Waste's briefing is `textdata.cam`'s `APc9`, the advisor and
//! the logo in its frame `INDb` 1002. **Binary**: quest briefing
//! applies the quest's own dataset, then makes `APc9`.
//! `main` and each return to the menus apply the
//! expansion's (tag `0x406c6552`), which opens `mx_textdata.cam` after
//! `textdata.cam`; why the main menu is still the base `AP29` is open.
//!
//! **Inferred** (`docs/formats/panels.md`): a `UIData_<w>_<h>.dat` panel shadows
//! the dataset's. `UIData_800_600.dat` holds none of these three; the other
//! twelve hold an `AP29` 800 by 768 with a mods row under the buttons.

use crate::data::fourcc::FourCc;

pub mod animate;
pub mod autosave;
pub mod bevel;
pub mod colour;
pub mod control;
pub mod end;
pub mod extras;
pub mod faces;
pub mod film;
pub mod font;
pub mod hotkey;
pub mod ids;
pub mod layout;
pub mod listbox;
pub mod media;
pub mod multiplayer;
pub mod music;
pub mod presets;
pub mod quest_map;
pub mod row;
pub mod saves;
pub mod scores;
pub mod tree;

/// The main menu.
pub const MAIN: FourCc = FourCc(*b"AP29");

/// The briefing's first page: background text, Continue, Cancel.
pub const BRIEFING: FourCc = FourCc(*b"APc9");

/// The briefing's second page: background and goals, Start Quest, Cancel.
pub const SUMMARY: FourCc = FourCc(*b"AP44");

/// The freestyle pick page, from the quest map's compass: Beginner,
/// Advanced and Expert Random 5000, 5003, 5007, Manual Settings 5004,
/// Cancel 5005 (map number roll; `crate::sim::mapgen::preset`).
pub const FREESTYLE: FourCc = FourCc(*b"APd2");

/// [`FREESTYLE`]'s Manual Settings and Cancel.
pub const MANUAL_SETTINGS: i32 = 5004;
pub const FREESTYLE_CANCEL: i32 = 5005;

/// Play Game's name prompt: field `0x138a`, OK 5000 on to the quest map,
/// Cancel `0x1f43` back.
pub const NAME_PROMPT: FourCc = FourCc(*b"AP35");

/// The prompt's name while none is stored (prefs record 1,
/// `LastNameUsed`): `GMTX` 186, `Your Majesty`.
pub const NAME_TEXT: usize = 186;

/// [`NAME_PROMPT`]'s controls.
pub mod prompt {
    /// The heading, "Please enter your Name...", `fn11`.
    pub const HEADING: i32 = 1;

    /// The name's text box, `0x138a`, `fnt2`.
    pub const FIELD: i32 = 5002;

    /// ACCEPT, hot key Enter: the name kept, on to the quest map.
    pub const ACCEPT: i32 = 5000;

    /// CANCEL, `0x1f43`, hot key Escape: back to the main menu.
    pub const CANCEL: i32 = 8003;

    /// The most characters the field takes. **Binary**: the dialog sets
    /// the box's to 21 while download hidden is 0 (HD
    /// shows the download jewel), else 12;
    /// `CYDialogTextboxItem` types a character only while the
    /// text is shorter.
    pub const LENGTH: usize = 21;
}

/// The message dialog, `AP11`: a title, a text, and OK alone or Yes and
/// No. Erase Victories asks with it, the quest map's hint and a game's
/// end tell with it.
pub const MESSAGE: FourCc = FourCc(*b"AP11");

/// The quit question, `AP04` (`QuitMenuController`, "Really exit
/// Majesty?"): the main menu's Exit asks it (main menu,
/// main menu); YES ends the game as [`room::EXIT`] plays out, NO goes
/// back. OPTIONS' QUIT MAJESTY asks the same.
pub mod quit {
    use crate::data::fourcc::FourCc;

    pub const PANEL: FourCc = FourCc(*b"AP04");

    /// YES (`0x157d`, hot key Y) and NO (hot key N).
    pub const YES: i32 = 5501;
    pub const NO: i32 = 5001;
}

/// [`MESSAGE`]'s controls and which buttons it shows.
///
/// **Binary**: quest map screen hides OK (`0x157f`) for Erase
/// Victories; quest map hint's caller hides Yes and No (`0x157d`,
/// `0x157e`), so OK closes the hint (`DialogView`, show count 0).
pub mod message {
    /// The title: "MESSAGE" until set.
    pub use super::ids::TITLE;

    /// The text.
    pub const TEXT: i32 = 5000;

    /// OK, hot key Enter.
    pub const OK: i32 = 5503;

    /// YES and NO, hot keys Y and N.
    pub const YES: i32 = 5501;
    pub const NO: i32 = 5502;

    /// What the dialog is for.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Ask {
        /// A note: OK alone.
        Tell,
        /// A question: Yes and No.
        Question,
    }

    /// The buttons shown for `ask`, in panel order.
    #[must_use]
    pub const fn shown(ask: Ask) -> &'static [i32] {
        match ask {
            Ask::Tell => &[OK],
            Ask::Question => &[YES, NO],
        }
    }
}

/// The main menu's buttons, by control id ([`smnu::CONTROL`](crate::data::smnu::CONTROL)).
///
/// **Corpus**: the label is the button's `STRT AP29` text; the hot key
/// is its [`smnu::HOTKEY`](crate::data::smnu::HOTKEY). 5000 and 5020 are the cheat replay and the
/// GPL debugger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Play = 5002,
    Multiplayer = 5003,
    Load = 5001,
    Settings = 5504,
    Credits = 5006,
    HighScores = 5010,
    Cinematic = 5005,
    Exit = 5004,
    Replay = 5000,
    Debugger = 5020,
}

impl Button {
    /// The eight the capture shows, in reading order.
    pub const SHOWN: [Button; 8] = [
        Button::Play,
        Button::Multiplayer,
        Button::Credits,
        Button::HighScores,
        Button::Load,
        Button::Settings,
        Button::Cinematic,
        Button::Exit,
    ];

    /// Its control id.
    #[must_use]
    pub const fn id(self) -> i32 {
        self as i32
    }
}

/// What CREDITS plays: `PICT` videos, `MV06` (`mx_cinedata1.cam`) then
/// `MV01` (`cinedata1.cam`), each full screen until it ends or a key or
/// mouse press stops it, then the main menu again. No panel or text of
/// its own, and it asks no track. **Binary**: main menu case 7201,
/// video play.
pub const CREDITS: [FourCc; 2] = [FourCc(*b"MV06"), FourCc(*b"MV01")];

/// What the start-up intro plays: `MV02`, then
/// `MV04` (HD, hd flag), each full screen and stopped by a key or
/// press as CREDITS' are, then the `SPLH` still. From the cinematics'
/// archive ([`CINEMATICS`]); none open, no intro and no still. Skipped by
/// `-nointro` and `IntroVideo` 0.
pub const INTRO: [FourCc; 2] = [FourCc(*b"MV02"), FourCc(*b"MV04")];

/// What the original's CINEMATIC (5005) plays;
/// openregent lists films instead ([`film`]): the music stopped,
/// `MV04`, then `MV07` only once the expansion's ending was seen
/// (expansion ending seen, `Seen::ending`), then track 2. `MV07`
/// is that ending ([`end::ENDING`]), which the
/// end panels ask but, in HD, never find.
pub const CINEMATIC: [FourCc; 2] = [FourCc(*b"MV04"), FourCc(*b"MV07")];

/// The cinematics' archives under the HD install, by `WhichCinematic` (which cinematic,
/// default 1): 1 `cinedata3.dat` and `mx_cinedata3.dat`, 640×480; 0 the
/// `2` pair, stored at half size for Bink to double. They hold [`INTRO`]
/// and [`CINEMATIC`], each with a sound track. Opened as needed, not with the datasets.
pub const CINEMATICS: [&str; 2] = ["Data/cinedata3.dat", "DataMX/mx_cinedata3.dat"];

/// How many frames a video shows: all but its last. Video play
/// stops when Bink's frame number reaches the count less one.
/// **Inferred**: Bink counts frames from 1, so
/// the last is never drawn.
#[must_use]
pub const fn shown_frames(frames: usize) -> usize {
    frames.saturating_sub(1)
}

/// `AP29`'s control showing the version, `STRT` text 10 "Version #".
///
/// Its id differs by panel (19 base, 21 expansion, 22 `UIData`), so it is
/// found by its text index. **Inferred**: the code writes the version
/// there; `menu_main.png` shows "Version 1.5.2.24" in its rectangle.
pub const VERSION_LABEL: usize = 10;

/// The main menu's room: each button has a picture of a room object over
/// the room, lit with it, and a click on either is the button's.
///
/// ```text
///  button ──hover──> its picture plays slot 6's playback (forward, held)
///  picture ─hover──> its button drawn light blue (slot 6)        (Pairs)
///  trigger ─hover──> its picture plays, as from the button        (Triggers)
///  leave ──────────> the picture plays slot 0's (back to frame 0)
/// ```
///
/// **Binary**: main menu ends on the id clicked and
/// takes a button, its picture and its triggers alike. Which controls
/// hover depends on [`Rule`](room::Rule).
pub mod room {
    use super::Button;

    /// Each button's room picture, a control of `AP29` with an `INkm`
    /// (base) or `IX50` (expansion) set. **Binary**: main menu pairs,
    /// in its order.
    pub const PICTURES: [(Button, i32); 8] = [
        (Button::Load, 5103),
        (Button::Play, 5100),
        (Button::Multiplayer, 5102),
        (Button::HighScores, 5105),
        (Button::Cinematic, 5107),
        (Button::Credits, 5101),
        (Button::Exit, 5104),
        (Button::Settings, 5108),
    ];

    /// The expansion `AP29`'s hit regions over the room, no picture, by the
    /// button each stands for. **Binary**: main menu link.
    /// **Corpus**: each tooltip names its button ("play
    /// btn trigger 1"; "levels" for Adjust Settings).
    pub const TRIGGERS: [(i32, Button); 13] = [
        (7101, Button::Play),
        (7102, Button::Play),
        (7103, Button::Play),
        (7104, Button::Play),
        (7201, Button::Credits),
        (7202, Button::Credits),
        (7203, Button::Credits),
        (7301, Button::Settings),
        (7401, Button::Multiplayer),
        (7501, Button::Load),
        (7601, Button::Cinematic),
        (7701, Button::HighScores),
        (7801, Button::Exit),
    ];

    /// The picture played to its last frame once the quit dialog (`AP04`)
    /// says yes, before the game ends; hidden until then.
    pub const EXIT: i32 = 3000;

    /// How [`EXIT`] plays once shown: forward to the last frame of its
    /// slot at its set's period (widget mode set 4, 1 at
    /// main menu); the menu passes until it gets there, then the game
    /// ends (app state 9, main menu).
    pub const LEAVING: super::animate::Mode = super::animate::Mode::Forward;

    /// Whether [`EXIT`], at `frame` of `count`, has played out: the last
    /// frame shown, or none to show.
    #[must_use]
    pub const fn left(frame: usize, count: usize) -> bool {
        frame + 1 >= count
    }

    /// Controls main menu hides: [`EXIT`], the cheat replay and the
    /// GPL debugger.
    pub const HIDDEN: [i32; 3] = [EXIT, Button::Replay.id(), Button::Debugger.id()];

    /// The slot a control draws with its partner hovered: its state plus
    /// 6. `DEM2` 1006 slot 6 is light blue;
    /// a room picture's slot 6 is its lit run.
    pub const LIT: usize = 6;

    /// The slot a room picture rests in.
    pub const REST: usize = 0;

    /// The button control `id` stands for, and that button's picture: a
    /// button, its picture or a trigger.
    #[must_use]
    pub fn link(id: i32) -> Option<(Button, i32)> {
        let button = PICTURES
            .iter()
            .find(|(b, p)| b.id() == id || *p == id)
            .map(|(b, _)| *b)
            .or_else(|| TRIGGERS.iter().find(|(t, _)| *t == id).map(|(_, b)| *b))?;

        picture(button).map(|p| (button, p))
    }

    /// `button`'s room picture.
    #[must_use]
    pub fn picture(button: Button) -> Option<i32> {
        PICTURES.iter().find(|(b, _)| *b == button).map(|(_, p)| *p)
    }

    /// Which controls take the pointer over the room.
    ///
    /// **Binary**: main menu registers main menu hover pair
    /// on every button and picture when hd flag is set and the panel
    /// has no control 9000 — HD with a dataset's `AP29`, as the captures
    /// — else main menu hover list on the buttons and
    /// [`TRIGGERS`]: `MajX`, and HD's `UIData` panels. The expansion's
    /// pictures are [`smnu::PASSIVE`](crate::data::smnu::PASSIVE), so its
    /// triggers are what hover.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Rule {
        /// A button and its picture light each other.
        Pairs,
        /// A button or trigger lights the picture.
        Triggers,
    }

    /// What one hovered control lights.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct Lit {
        /// The room picture playing its lit run.
        pub picture: Option<i32>,
        /// The button drawn in slot [`LIT`] though not itself hovered.
        pub button: Option<Button>,
    }

    /// What `hovered` lights under `rule`.
    ///
    /// **Binary**: a hovered button starts its picture's slot-6 playback
    /// under either rule; under [`Rule::Pairs`] a hovered picture draws
    /// its button at slot 6. **Inferred**: a hovered picture plays its
    /// own lit run. The pair callback leaves its playback as it was and
    /// draws it at slot 6 plus its state; the user reports the room's
    /// objects animating under the pointer.
    #[must_use]
    pub fn lit(rule: Rule, hovered: i32) -> Lit {
        let part_of = |(button, picture): (Button, i32)| {
            let part = if button.id() == hovered {
                Part::Button
            } else if picture == hovered {
                Part::Picture
            } else {
                Part::Trigger
            };

            (part, button, picture)
        };

        match (rule, link(hovered).map(part_of)) {
            (_, Some((Part::Button, _, picture)))
            | (Rule::Triggers, Some((Part::Trigger, _, picture))) => Lit {
                picture: Some(picture),
                button: None,
            },
            (Rule::Pairs, Some((Part::Picture, button, picture))) => Lit {
                picture: Some(picture),
                button: Some(button),
            },
            // A trigger has no hover callback under `Pairs`; a picture
            // takes no pointer under `Triggers`.
            _ => Lit::default(),
        }
    }

    /// Which of a button's controls is hovered.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Part {
        Button,
        Picture,
        Trigger,
    }
}

/// The briefing's controls, both pages.
///
/// **Binary**: briefing fill fills them from the current quest (`CURR`);
/// quest briefing ends on 5000 or 5001.
pub mod briefing {
    /// Continue (`APc9`), Start Quest (`AP44`): hot key Enter.
    pub const NEXT: i32 = 5000;

    /// Cancel: hot key Escape.
    pub const CANCEL: i32 = 5001;

    /// The background text, the quest's `<Long>`; font
    /// `fn11` on `APc9`, `fnt4` on `AP44`. Scroll bar 9101 drives it.
    pub const BACKGROUND: i32 = 5002;

    /// The goals, the quest's `<Short>`; `AP44` only.
    /// Scroll bar 12 drives it.
    pub const GOALS: i32 = 5003;

    /// The title, the quest's name.
    pub const TITLE: i32 = 8885;

    /// The porthole: no picture of its own. Quest briefing plays the
    /// quest's [`super::video`] in its rectangle.
    pub const PORTHOLE: i32 = 8886;

    /// `AP44`'s round still over the porthole: [`super::zoom`].
    pub const ZOOM: i32 = 8720;

    /// The round quest icon in the title bar, [`super::icon`], is the
    /// pog ([`super::ids::POG`]); the background text's scroll bar
    /// [`super::ids::SCROLL_BAR`].
    ///
    /// The goals' scroll bar.
    pub const GOALS_SCROLL: i32 = 12;
}

/// The loading page, shown while a quest's or freestyle game's world is
/// made: an 800 by 600 frame, `INDg` 1000, centred.
///
/// **Binary**: four sites make it, after the game's dataset is applied
/// (loading quest from quest start, freestyle start cfg,
/// freestyle dialog, freestyle start net), into
/// loading panel. A save's load makes none.
pub const LOADING: FourCc = FourCc(*b"AP50");

/// [`LOADING`]'s controls; the round quest icon ([`icon`]) is
/// [`ids::POG`].
pub mod loading {
    /// The still, 750 by 375: [`super::segue`].
    pub const STILL: i32 = 5000;

    /// The title: `GMTX` 80 for freestyle.
    pub const TITLE: i32 = 8885;

    /// The progress bar, a scroll bar (`INDl` 1000, no picture) moved by
    /// message `0x29`; empty, it draws nothing.
    pub const BAR: i32 = 5020;
}

/// HD's custom-game dialog, from the pick page's Manual Settings:
/// 800 by 600, centred, in `mx_textdata.cam`.
pub const SETUP: FourCc = FourCc(*b"MX17");

/// [`SETUP`]'s controls. **Binary**: freestyle dialog and its
/// fillers.
/// **Game**: `ui_freestyle_setup.png`, the dialog at its own size on a
/// 1680 by 1050 screen.
pub mod setup {
    /// The title, "Single Player Freestyle Options", `fn11`.
    pub const TITLE: i32 = 10;

    /// ACCEPT, hot key A, the game; CANCEL, `0x1f43`, hot key Escape,
    /// the quest map.
    pub const ACCEPT: i32 = 5000;
    pub const CANCEL: i32 = 8003;

    /// The lists, each a combo box: Player Forces, Enemy Forces,
    /// Wandering Monsters (`0x1389` to `0x138b`), Landscape (`0x138c`),
    /// Victory Conditions (`0x138d`), Starting Gold, Victory Limits
    /// (`0x1390`), the two Special Events, Building Limitations.
    pub const SETTLEMENT: i32 = 5001;
    pub const SCENARIO: i32 = 5002;
    pub const MONSTERS: i32 = 5003;
    pub const LAND: i32 = 5004;
    pub const VICTORY: i32 = 5005;
    pub const GOLD: i32 = 5007;
    pub const LIMITS: i32 = 5008;
    pub const EVENT_1: i32 = 5200;
    pub const EVENT_2: i32 = 5300;
    pub const BUILDINGS: i32 = 5100;

    /// The text boxes beside the forces' and events' lists: each pick's
    /// blurb.
    pub const SETTLEMENT_TEXT: i32 = 7621;
    pub const SCENARIO_TEXT: i32 = 7622;
    pub const MONSTERS_TEXT: i32 = 7633;
    pub const EVENT_1_TEXT: i32 = 5201;
    pub const EVENT_2_TEXT: i32 = 5301;

    /// The line under the victory lists, "Descriptive Text for Each Goal".
    pub const GOAL_TEXT: i32 = 5009;

    /// The settings file's name, `GMTX` 411 "Default Settings", and its
    /// LOAD and SAVE (`FreestyleIO_Controller`).
    pub const SAVE_NAME: i32 = 6052;
    pub const LOAD: i32 = 6050;
    pub const SAVE: i32 = 6051;

    /// BUILDING EDIT MENU, ALLOW CHEATS.
    pub const EDIT_MENU: i32 = 5101;
    pub const CHEATS: i32 = 3036;

    /// The difficulty's word (`0x13ba`), from the GPL's
    /// `CalculateDifficulty` and `CalculateDifficultyString`,
    /// `GMTX` 115 on.
    pub const DIFFICULTY: i32 = 5050;

    /// The Map Number's box (`0x1a6e`), read at Accept by `sscanf %ld`.
    pub const MAP_NUMBER: i32 = 6766;

    /// The picture over the chat, `IX22` 1000. **Game**: the capture
    /// shows it and none of the chat's controls ([`CHAT`]).
    pub const PORTRAIT: i32 = 3887;

    /// The chat's controls, a network game's: CHAT ROOM, its box, scroll
    /// bar, entry, SEND CHAT and its hint.
    pub const CHAT_HEADING: i32 = 4;
    pub const CHAT_BOX: i32 = 6001;
    pub const CHAT_ENTRY: i32 = 6002;
    pub const SEND_CHAT: i32 = 6003;
    pub const CHAT_HINT: i32 = 1356;
    pub const CHAT: [i32; 6] = [
        CHAT_HEADING,
        CHAT_BOX,
        super::ids::SCROLL_BAR,
        CHAT_ENTRY,
        SEND_CHAT,
        CHAT_HINT,
    ];

    /// A combo box's parts are pictures of its own set plus these: the
    /// arrow button, the text box, the drop list and its scroll bar.
    /// Its own set draws
    /// nothing.
    pub const ARROW: i32 = 1;
    pub const FIELD: i32 = 2;
    pub const LIST: i32 = 3;
    pub const LIST_SCROLL: i32 = 4;
}

pub use animate::{Animator, Mode};
pub use colour::{Role, Style};
pub use control::{
    Able, CHECKED_SLOT, HOVER_SLOT, Hover, Press, State, Tick, drawn_slot, state, still, strip,
};
pub use layout::{Layout, ShownControl, hotkey_letter, layout, resolved};
pub use media::{BACKDROP, Release, World, end_voice, icon, segue, video, voice, zoom};

/// The briefing's large font, `fn11` "Majestica 12": the title and
/// `APc9`'s background text.
pub const LARGE_FONT: FourCc = FourCc(*b"fn11");

/// The small font, `fnt4` "tahona 7": buttons, labels, `AP44`'s texts.
pub const SMALL_FONT: FourCc = FourCc(*b"fnt4");

#[cfg(test)]
mod tests {
    use super::room::{Lit, Rule, link, lit};
    use super::*;
    use crate::data::smnu::{self, Rect};

    fn control(id: i32, rect: (i32, i32, i32, i32), flags: i32) -> ShownControl {
        let (x, y, width, height) = rect;

        ShownControl {
            id: Some(id),
            rect: Some(Rect {
                x,
                y,
                width,
                height,
            }),
            flags,
            ..ShownControl::default()
        }
    }

    /// The base `AP29`'s map (5100) under its chest (5103), over the
    /// passive room.
    fn room() -> Layout {
        Layout {
            id: MAIN,
            panel: ShownControl::default(),
            centred: true,
            controls: vec![
                control(1, (15, 10, 768, 520), smnu::PASSIVE),
                control(5100, (15, 308, 579, 222), smnu::CLICKS),
                control(5103, (250, 165, 258, 176), smnu::CLICKS),
                control(room::EXIT, (538, 325, 245, 199), smnu::PASSIVE),
            ],
            style: None,
        }
    }

    #[test]
    fn the_last_drawn_control_holding_the_point_is_under_it() {
        let layout = room();
        let under = |at| layout.under(at, &room::HIDDEN).and_then(|c| c.id);

        assert_eq!(under((300, 320)), Some(5103));
        assert_eq!(under((300, 400)), Some(5100));
        assert_eq!(under((20, 20)), None, "the room is passive");
        assert_eq!(under((593, 529)), Some(5100));
        assert_eq!(under((594, 529)), None, "the far edge is outside");
    }

    #[test]
    fn a_rectangle_past_the_widest_coordinate_does_not_overflow() {
        let layout = Layout {
            controls: vec![control(1, (i32::MAX, i32::MAX, 10, 10), smnu::CLICKS)],
            ..room()
        };

        assert_eq!(layout.under((0, 0), &[]).and_then(|c| c.id), None);
        assert_eq!(
            layout.under((i32::MAX, i32::MAX), &[]).and_then(|c| c.id),
            Some(1)
        );
    }

    #[test]
    fn a_button_its_picture_and_its_triggers_stand_for_the_button() {
        assert_eq!(link(5002), Some((Button::Play, 5100)));
        assert_eq!(link(5100), Some((Button::Play, 5100)));
        assert_eq!(link(7103), Some((Button::Play, 5100)));
        assert_eq!(link(7301), Some((Button::Settings, 5108)));
        assert_eq!(link(7801), Some((Button::Exit, 5104)));
        assert_eq!(link(5000), None);
    }

    #[test]
    fn a_picture_lights_its_button_only_in_pairs() {
        let both = Lit {
            picture: Some(5100),
            button: Some(Button::Play),
        };
        let picture = Lit {
            picture: Some(5100),
            button: None,
        };

        assert_eq!(lit(Rule::Pairs, 5100), both);
        assert_eq!(lit(Rule::Pairs, 5002), picture);
        assert_eq!(lit(Rule::Pairs, 7101), Lit::default());
        assert_eq!(lit(Rule::Triggers, 7101), picture);
        assert_eq!(lit(Rule::Triggers, 5002), picture);
        assert_eq!(lit(Rule::Triggers, 5100), Lit::default());
        assert_eq!(lit(Rule::Pairs, 1), Lit::default());
    }
}
