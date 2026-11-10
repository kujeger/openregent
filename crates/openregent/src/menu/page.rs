//! The menus' pages and the moves between them: no drawing, no Bevy.

use regent_core::data::fourcc::FourCc;
use regent_core::sim::mapgen::preset::Preset;
use regent_core::view::menu::film::Film;
use regent_core::view::menu::presets::Mode;
use regent_core::view::menu::{self, multiplayer, music};
use regent_core::view::menu::{quest_map, scores};

/// The options screen, `AP37`. **Inferred** from its
/// sound and voice tests and its read of the music volume.
const OPTIONS: FourCc = FourCc(*b"AP37");

/// The question dialog, `AP11`: Erase Victories' and the map's hint.
const QUESTION: FourCc = FourCc(*b"AP11");

/// The load dialog, `AP27` ([panels.md](docs/formats/panels.md#the-main-screen)).
const LOAD: FourCc = FourCc(*b"AP27");

/// HD's freestyle setup dialog, `MX17`.
const SETUP: FourCc = FourCc(*b"MX17");

/// HD's mods dialog, `APdc`, main menu control 9000.
const MODS: FourCc = FourCc(*b"APdc");

/// A page of the menus. A quest is its index in the offered list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Map,
    /// The quests the map has no place for (`MD01`, `VAMP`, the mod
    /// folders'), from its jewel; the original's `APd8`.
    Extras,
    /// The settings editor over the main menu (`editor`).
    Settings,
    /// The profile page over the main menu (`profiles`), from its
    /// Profile button: the original asks a name (`AP35`) at Play Game.
    Profiles,
    /// The mods page over the main menu (`mods`), from its Mods button:
    /// a placeholder for HD's `APdc`.
    Mods,
    /// Load Game's saves over the main menu (`save::dialog`), for `AP27`.
    Loads,
    /// Erase Victories' question over the map, the original's `AP11`.
    Erase,
    /// Exit's question over the main menu, `AP04`.
    Quit,
    /// The main menu as the game ends: its exit picture played out.
    Leaving,
    /// The map's hint over it, `AP11` with OK alone: its `GMTX` text.
    Hint(usize),
    Background(usize),
    Summary(usize),
    /// The freestyle pick page, `APd2`, from the map's compass: three
    /// presets, Manual Settings, Cancel.
    Freestyle,
    /// The freestyle setup page (`freestyle`), standing in for `MX17`.
    Setup,
    /// The Build Tree Editor (`tree`), `MX18`, from a setup page.
    Trees(Over),
    /// Load or Save Freestyle Options (`preset_page`), `MX20` or `MX21`,
    /// from a setup page's LOAD and SAVE.
    Presets(Mode, Over),
    /// HIGH SCORES, `AP64` (`scores`).
    Scores,
    /// CREDITS: video `n` of `menu::CREDITS` playing (`cinema`).
    Credits(usize),
    /// The start-up intro: video `n` of `menu::INTRO` playing, before
    /// the main menu.
    Intro(usize),
    /// Cinematics (`films`): the films a profile has unlocked, over the
    /// main menu. openregent's list for the original's CINEMATIC.
    Cinematics,
    /// A film of the list playing, then the list again.
    Film(Film),
    /// Multiplayer Protocol, `AP42`: LAN or internet.
    Protocol,
    /// Multiplayer Setup, `AP33` (`lan`): the host's address to join, the
    /// port to host on.
    Multiplayer,
    /// The session dialog, `AP55`: the session's name and players.
    Create,
    /// The lobby, `MX19`: the host's, or a guest's.
    Lobby,
    /// The host's settings, `MX17` with its chat, from the lobby's ADJUST
    /// SETTINGS.
    LanSetup,
}

/// Which setup page a page opens over, and closes back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Over {
    /// The freestyle setup page.
    Alone,
    /// The host's settings of a LAN game.
    Lan,
}

impl Over {
    /// The setup page it is.
    #[must_use]
    pub const fn page(self) -> Page {
        match self {
            Over::Alone => Page::Setup,
            Over::Lan => Page::LanSetup,
        }
    }
}

/// What the player did on a page: a button, a quest on the map, or
/// Escape ([`Press::Back`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Button(Button),
    Quest(usize),
    /// The main menu's Profile button: openregent's own, in HD's mods row.
    Profiles,
    /// The main menu's Mods button: HD's control 9000.
    Mods,
    /// The main menu's Book of Lore button: openregent's own, in HD's mods
    /// row.
    Lore,
    Back,
    /// The page's video played to its end.
    Ended,
    /// A film of the Cinematics page picked.
    Film(Film),
}

/// Where a press leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    To(Page),
    /// Make the quest's game and play it.
    Start(usize),
    /// Write the preset over the freestyle config, make its game and
    /// play it.
    Preset(Preset),
    /// Make the freestyle config's game and play it.
    Custom,
    /// Erase the profile's victories, then back to the map.
    Erase,
    /// The Book of Lore opened or closed over the main menu.
    Lore,
    /// ERASE SCORES: the shown game's high scores emptied, asked nothing.
    EraseScores,
    /// Bind the host's port and wait for its guests in the lobby.
    Host,
    /// Ask the host typed for a seat, in the lobby.
    Join,
    /// The host's LAN game started: every guest welcomed.
    StartLan,
    /// A guest's ready flipped.
    Ready,
    /// The host's LOAD GAME: its LAN saves listed, one to load.
    LoadLan,
    /// The host's settings kept and shown to the guests, back to the
    /// lobby.
    Share,
    Exit,
}

impl Page {
    /// Where `press` leads from here; `None` where it does nothing.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm per page transition")]
    pub const fn after(self, press: Press) -> Option<Step> {
        match (self, press) {
            (Page::Main, Press::Button(Button::LoadGame)) => Some(Step::To(Page::Loads)),
            (Page::Main, Press::Button(Button::HighScores)) => Some(Step::To(Page::Scores)),
            (Page::Main, Press::Button(Button::Credits)) => Some(Step::To(Page::Credits(0))),
            (Page::Scores, Press::Button(Button::EraseScores)) => Some(Step::EraseScores),
            // A video ends, or a key or press stops it; the last back to
            // the main menu.
            (Page::Credits(n), Press::Back | Press::Ended) if n + 1 < menu::CREDITS.len() => {
                Some(Step::To(Page::Credits(n + 1)))
            }
            (Page::Intro(n), Press::Back | Press::Ended) if n + 1 < menu::INTRO.len() => {
                Some(Step::To(Page::Intro(n + 1)))
            }
            (Page::Main, Press::Button(Button::Cinematic))
            | (Page::Film(_), Press::Back | Press::Ended) => Some(Step::To(Page::Cinematics)),
            (Page::Cinematics, Press::Film(film)) => Some(Step::To(Page::Film(film))),
            (Page::Main, Press::Profiles) => Some(Step::To(Page::Profiles)),
            (Page::Main, Press::Mods) => Some(Step::To(Page::Mods)),
            (Page::Main, Press::Lore) => Some(Step::Lore),
            // Play Game; Cancel or Escape on a quest's pages, the extras
            // and the question.
            (Page::Main, Press::Button(Button::PlayGame))
            | (
                Page::Extras | Page::Background(_) | Page::Summary(_),
                Press::Button(Button::Cancel) | Press::Back,
            )
            | (Page::Erase, Press::Button(Button::No) | Press::Back)
            // **Inferred**: Escape closes the hint as OK does.
            | (Page::Hint(_), Press::Button(Button::Ok) | Press::Back) => Some(Step::To(Page::Map)),
            (Page::Map, Press::Button(Button::EraseVictories)) => Some(Step::To(Page::Erase)),
            (Page::Erase, Press::Button(Button::Yes)) => Some(Step::Erase),
            (Page::Main, Press::Button(Button::ExitGame)) => Some(Step::To(Page::Quit)),
            (Page::Quit, Press::Button(Button::Yes)) => Some(Step::To(Page::Leaving)),
            (Page::Leaving, Press::Ended) => Some(Step::Exit),
            (Page::Main, Press::Button(Button::AdjustSettings)) => Some(Step::To(Page::Settings)),
            // The editor's Escape is its own: it puts the settings back.
            (Page::Settings, Press::Button(Button::Cancel)) => Some(Step::To(Page::Main)),
            // A profile picked, or none.
            (Page::Map | Page::Scores, Press::Button(Button::MainMenu) | Press::Back)
            | (Page::Profiles, Press::Button(Button::Continue))
            | (
                Page::Profiles | Page::Mods | Page::Loads | Page::Protocol | Page::Cinematics,
                Press::Button(Button::Cancel) | Press::Back,
            )
            // **Inferred**: Escape is NO, as on the map's question.
            | (Page::Quit, Press::Button(Button::No) | Press::Back)
            | (Page::Credits(_) | Page::Intro(_), Press::Back | Press::Ended) => {
                Some(Step::To(Page::Main))
            }
            (Page::Map, Press::Button(Button::Downloadable)) => Some(Step::To(Page::Extras)),
            (Page::Map | Page::Extras, Press::Quest(quest)) => {
                Some(Step::To(Page::Background(quest)))
            }
            (Page::Background(quest), Press::Button(Button::Continue)) => {
                Some(Step::To(Page::Summary(quest)))
            }
            (Page::Summary(quest), Press::Button(Button::StartQuest)) => Some(Step::Start(quest)),
            (Page::Map, Press::Button(Button::FreestyleQuests)) => Some(Step::To(Page::Freestyle)),
            (Page::Freestyle, Press::Button(Button::BeginnerRandom)) => {
                Some(Step::Preset(Preset::Beginner))
            }
            (Page::Freestyle, Press::Button(Button::AdvancedRandom)) => {
                Some(Step::Preset(Preset::Advanced))
            }
            (Page::Freestyle, Press::Button(Button::ExpertRandom)) => {
                Some(Step::Preset(Preset::Expert))
            }
            (Page::Freestyle, Press::Button(Button::ManualSettings)) => Some(Step::To(Page::Setup)),
            (Page::Setup, Press::Button(Button::Accept)) => Some(Step::Custom),
            (Page::Setup, Press::Button(button)) if button.over_setup() => {
                Some(Step::To(button.opened(Over::Alone)))
            }
            (Page::LanSetup, Press::Button(button)) if button.over_setup() => {
                Some(Step::To(button.opened(Over::Lan)))
            }
            // The editor closes back to the setup (freestyle dialog,
            // case 5101), LOAD's and SAVE's pages too.
            (
                Page::Trees(over) | Page::Presets(_, over),
                Press::Button(Button::Cancel) | Press::Back,
            ) => Some(Step::To(over.page())),
            // Both go back to the quest map, AP30 (map number roll,
            // freestyle dialog).
            (Page::Freestyle | Page::Setup, Press::Button(Button::Cancel) | Press::Back) => {
                Some(Step::To(Page::Map))
            }
            (Page::Main, Press::Button(Button::Multiplayer))
                => Some(Step::To(Page::Protocol)),
            (Page::Protocol, Press::Button(Button::Accept))
            | (
                Page::Create | Page::Lobby,
                Press::Button(Button::Cancel) | Press::Back,
            ) => Some(Step::To(Page::Multiplayer)),
            (Page::Multiplayer, Press::Button(Button::Cancel) | Press::Back) => {
                Some(Step::To(Page::Protocol))
            }
            (Page::Multiplayer, Press::Button(Button::CreateGame)) => Some(Step::To(Page::Create)),
            (Page::Multiplayer, Press::Button(Button::JoinGame)) => Some(Step::Join),
            (Page::Create, Press::Button(Button::Accept)) => Some(Step::Host),
            (Page::Lobby, Press::Button(Button::StartGame)) => Some(Step::StartLan),
            (Page::Lobby, Press::Button(Button::Ready)) => Some(Step::Ready),
            (Page::Lobby, Press::Button(Button::LoadGame)) => Some(Step::LoadLan),
            (Page::Lobby, Press::Button(Button::AdjustSettings)) => {
                Some(Step::To(Page::LanSetup))
            }
            (Page::LanSetup, Press::Button(Button::Accept)) => Some(Step::Share),
            (Page::LanSetup, Press::Button(Button::Cancel) | Press::Back) => {
                Some(Step::To(Page::Lobby))
            }
            _ => None,
        }
    }

    /// Whether the original's map screen is open on it: the map, and
    /// what it asks over itself (quest map screen's loop).
    const fn over_map(self) -> bool {
        matches!(self, Page::Map | Page::Extras | Page::Erase | Page::Hint(_))
    }

    /// What moving from here to `to` does to the menus' voice. The map
    /// screen's advisor stops as it closes; the briefing's
    /// voice plays on `APc9` alone, stopped at Continue or Cancel.
    ///
    /// ```text
    ///  any ──> Background(q)            its voice, the last stopped
    ///  not the map ──> the map          the map opens: its advisor, if any
    ///  the map ──> its own pages        plays on
    ///  the map or Background ──> other  stopped
    /// ```
    #[must_use]
    pub const fn speech(self, to: Page) -> Speech {
        match (self, to) {
            (_, Page::Background(quest)) => Speech::Briefing(quest),
            _ if to.over_map() && !self.over_map() => Speech::Map,
            _ if to.over_map() => Speech::Keep,
            (Page::Background(_), _) => Speech::Hush,
            _ if self.over_map() => Speech::Hush,
            _ => Speech::Keep,
        }
    }

    /// The original's screen it stands for, whose track it plays
    /// ([`Page::music`]).
    #[must_use]
    pub const fn panel(self) -> FourCc {
        match self {
            Page::Main | Page::Leaving => menu::MAIN,
            Page::Quit => menu::quit::PANEL,
            Page::Protocol => multiplayer::PROTOCOL,
            Page::Multiplayer => multiplayer::GAMES,
            Page::Create => multiplayer::SESSION,
            Page::Lobby => multiplayer::LOBBY,
            Page::Map => quest_map::PANEL,
            Page::Extras | Page::Cinematics => menu::extras::PANEL,
            Page::Settings => OPTIONS,
            Page::Profiles => menu::NAME_PROMPT,
            Page::Mods => MODS,
            Page::Loads => LOAD,
            Page::Erase | Page::Hint(_) => QUESTION,
            Page::Background(_) => menu::BRIEFING,
            Page::Summary(_) => menu::SUMMARY,
            Page::Freestyle => menu::FREESTYLE,
            Page::Setup | Page::LanSetup => SETUP,
            Page::Trees(_) => regent_core::view::menu::tree::EDITOR,
            Page::Presets(mode, _) => mode.panel(),
            Page::Scores => scores::PANEL,
            // No screen: the video plays inside main menu.
            Page::Credits(n) => menu::CREDITS[n % menu::CREDITS.len()],
            Page::Intro(n) => menu::INTRO[n % menu::INTRO.len()],
            Page::Film(film) => film.id(),
        }
    }

    /// The track its screen asks as it opens; `None` keeps what plays
    /// (`menu::music::track`).
    ///
    /// ```text
    ///  Main, Map ──> 2   Background, Summary ──> 6   the rest ──> plays on
    /// ```
    ///
    /// The high scores and the credits ask none: screen switch's
    /// `AP64` and main menu's videos call no music.
    #[must_use]
    pub fn music(self) -> Option<i32> {
        match self {
            // Its films stop the music: it comes back with the list.
            Page::Cinematics => Some(music::THEME),
            _ => music::track(self.panel()),
        }
    }

    /// Its buttons, in reading order.
    #[must_use]
    pub const fn buttons(self) -> &'static [Button] {
        match self {
            Page::Main => &Button::MAIN,
            Page::Map => &Button::MAP,
            Page::Extras
            | Page::Cinematics
            | Page::Settings
            | Page::Mods
            | Page::Loads
            | Page::Trees(_)
            | Page::Presets(..) => &[Button::Cancel],
            Page::Erase | Page::Quit => &[Button::Yes, Button::No],
            Page::Hint(_) => &[Button::Ok],
            Page::Profiles | Page::Background(_) => &[Button::Continue, Button::Cancel],
            Page::Summary(_) => &[Button::StartQuest, Button::Cancel],
            Page::Freestyle => &Button::FREESTYLE,
            Page::Setup | Page::LanSetup => &[
                Button::Accept,
                Button::EditMenu,
                Button::LoadOptions,
                Button::SaveOptions,
                Button::Cancel,
            ],
            Page::Scores => &[Button::EraseScores, Button::MainMenu],
            Page::Credits(_) | Page::Intro(_) | Page::Film(_) | Page::Leaving => &[],
            Page::Multiplayer => &[Button::CreateGame, Button::JoinGame, Button::Cancel],
            Page::Protocol | Page::Create => &[Button::Accept, Button::Cancel],
            Page::Lobby => &[
                Button::StartGame,
                Button::Ready,
                Button::AdjustSettings,
                Button::Cancel,
            ],
        }
    }
}

/// What a move does to the menus' voice ([`Page::speech`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speech {
    /// What plays plays on.
    Keep,
    /// Stopped.
    Hush,
    /// The map opens: what played stops, its advisor may speak
    /// (`quest_map::Progress::open_map`).
    Map,
    /// The quest's briefing voice (`menu::voice`), what played stopped.
    Briefing(usize),
}

/// Every button of the menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    PlayGame,
    Multiplayer,
    LoadGame,
    AdjustSettings,
    Credits,
    HighScores,
    Cinematic,
    ExitGame,
    EraseVictories,
    FreestyleQuests,
    MainMenu,
    /// The map's jewel: Load Downloadable Quest.
    Downloadable,
    Continue,
    Cancel,
    StartQuest,
    Yes,
    No,
    Ok,
    BeginnerRandom,
    AdvancedRandom,
    ExpertRandom,
    ManualSettings,
    /// The setup page's start: `MX17`'s 5000.
    Accept,
    /// The setup page's BUILDING EDIT MENU, 5101.
    EditMenu,
    /// The setup page's LOAD and SAVE, 6050 and 6051.
    LoadOptions,
    SaveOptions,
    /// `AP64`'s 9001.
    EraseScores,
    /// Multiplayer Setup's (`lan`): to the session dialog.
    CreateGame,
    /// Multiplayer Setup's: ask the host typed for a seat.
    JoinGame,
    /// The lobby's, the host's: the game started.
    StartGame,
    /// The lobby's, a guest's: ready, or not.
    Ready,
}

impl Button {
    /// The pick page's, top to bottom as `APd2` stacks them.
    pub const FREESTYLE: [Button; 5] = [
        Button::BeginnerRandom,
        Button::AdvancedRandom,
        Button::ExpertRandom,
        Button::ManualSettings,
        Button::Cancel,
    ];

    /// The main menu's, row by row as the capture has them
    /// (`local/shots/menu_main.png`): four a row.
    pub const MAIN: [Button; 8] = [
        Button::PlayGame,
        Button::Multiplayer,
        Button::Credits,
        Button::HighScores,
        Button::LoadGame,
        Button::AdjustSettings,
        Button::Cinematic,
        Button::ExitGame,
    ];

    /// The quest map's, left to right, then its jewel.
    pub const MAP: [Button; 4] = [
        Button::EraseVictories,
        Button::FreestyleQuests,
        Button::MainMenu,
        Button::Downloadable,
    ];

    /// Its label, as the captures write it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Button::PlayGame => "Play Game",
            Button::Multiplayer => "Multiplayer",
            Button::LoadGame => "Load Game",
            Button::AdjustSettings => "Adjust Settings",
            Button::Credits => "Credits",
            Button::HighScores => "High Scores",
            Button::Cinematic => "Cinematics",
            Button::ExitGame => "Exit Game",
            Button::EraseVictories => "Erase Victories",
            Button::FreestyleQuests => "Freestyle Quests",
            Button::MainMenu => "Main Menu",
            Button::Downloadable => "Load Downloadable Quest",
            Button::Continue => "Continue",
            Button::Cancel => "Cancel",
            Button::StartQuest => "Start Quest",
            Button::Yes => "Yes",
            Button::No => "No",
            Button::Ok => "OK",
            Button::BeginnerRandom => "Beginner Random",
            Button::AdvancedRandom => "Advanced Random",
            Button::ExpertRandom => "Expert Random",
            Button::ManualSettings => "Manual Settings",
            Button::Accept => "Accept",
            Button::EditMenu => "Building Edit Menu",
            Button::LoadOptions => "Load",
            Button::SaveOptions => "Save",
            Button::EraseScores => "Erase Scores",
            Button::CreateGame => "Create Game",
            Button::JoinGame => "Join Game",
            Button::StartGame => "Start Game",
            Button::Ready => "Ready",
        }
    }

    /// Whether it opens a page over a setup page.
    const fn over_setup(self) -> bool {
        matches!(
            self,
            Button::EditMenu | Button::LoadOptions | Button::SaveOptions
        )
    }

    /// The page it opens over the setup page `over`.
    const fn opened(self, over: Over) -> Page {
        match self {
            Button::LoadOptions => Page::Presets(Mode::Load, over),
            Button::SaveOptions => Page::Presets(Mode::Save, over),
            _ => Page::Trees(over),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_game_leads_through_a_quest_to_its_start() {
        let steps = [
            (Page::Main, Press::Button(Button::PlayGame)),
            (Page::Map, Press::Quest(4)),
            (Page::Background(4), Press::Button(Button::Continue)),
        ];
        let reached = steps.map(|(page, press)| page.after(press));

        assert_eq!(
            reached,
            [
                Some(Step::To(Page::Map)),
                Some(Step::To(Page::Background(4))),
                Some(Step::To(Page::Summary(4))),
            ]
        );
        assert_eq!(
            Page::Summary(4).after(Press::Button(Button::StartQuest)),
            Some(Step::Start(4))
        );
    }

    #[test]
    fn cancel_and_escape_return_to_the_map_and_then_the_main_menu() {
        let cancel = Press::Button(Button::Cancel);

        for page in [Page::Background(1), Page::Summary(1)] {
            assert_eq!(page.after(cancel), Some(Step::To(Page::Map)));
            assert_eq!(page.after(Press::Back), Some(Step::To(Page::Map)));
        }

        assert_eq!(Page::Map.after(Press::Back), Some(Step::To(Page::Main)));
        assert_eq!(
            Page::Map.after(Press::Button(Button::MainMenu)),
            Some(Step::To(Page::Main))
        );
        assert_eq!(Page::Main.after(Press::Back), None);
    }

    /// Exit asks `AP04`; YES plays the exit picture out, then the game
    /// ends; NO or Escape goes back.
    #[test]
    fn exit_asks_then_plays_out_then_ends() {
        let yes = Press::Button(Button::Yes);
        let no = Press::Button(Button::No);

        assert_eq!(
            Page::Main.after(Press::Button(Button::ExitGame)),
            Some(Step::To(Page::Quit))
        );
        assert_eq!(Page::Quit.after(yes), Some(Step::To(Page::Leaving)));
        assert_eq!(Page::Quit.after(no), Some(Step::To(Page::Main)));
        assert_eq!(Page::Quit.after(Press::Back), Some(Step::To(Page::Main)));
        assert_eq!(Page::Leaving.after(Press::Back), None);
        assert_eq!(Page::Leaving.after(Press::Ended), Some(Step::Exit));
    }

    #[test]
    fn every_button_leads_somewhere() {
        let pages = [
            Page::Main,
            Page::Map,
            Page::Extras,
            Page::Settings,
            Page::Profiles,
            Page::Mods,
            Page::Loads,
            Page::Erase,
            Page::Quit,
            Page::Hint(143),
            Page::Background(0),
            Page::Summary(0),
            Page::Freestyle,
            Page::Setup,
            Page::Trees(Over::Alone),
            Page::Presets(Mode::Load, Over::Alone),
            Page::Presets(Mode::Save, Over::Lan),
            Page::Scores,
            Page::Credits(0),
            Page::Intro(0),
            Page::Cinematics,
            Page::Film(Film::North),
            Page::Multiplayer,
            Page::Create,
            Page::Lobby,
            Page::LanSetup,
        ];

        for page in pages {
            for &button in page.buttons() {
                let leads = page.after(Press::Button(button)).is_some();

                assert!(leads, "{button:?} on {page:?}");
            }
        }
    }

    #[test]
    fn the_jewel_lists_the_extra_quests_which_lead_to_their_briefing() {
        assert_eq!(
            Page::Map.after(Press::Button(Button::Downloadable)),
            Some(Step::To(Page::Extras))
        );
        assert_eq!(
            Page::Extras.after(Press::Quest(33)),
            Some(Step::To(Page::Background(33)))
        );
        assert_eq!(Page::Extras.after(Press::Back), Some(Step::To(Page::Map)));
        assert_eq!(
            Page::Extras.after(Press::Button(Button::Cancel)),
            Some(Step::To(Page::Map))
        );
    }

    #[test]
    fn the_main_menu_shows_all_eight_buttons() {
        assert_eq!(Page::Main.buttons(), Button::MAIN);
    }

    #[test]
    fn the_intro_plays_mv02_then_mv04_each_skipped_by_a_press() {
        for press in [Press::Back, Press::Ended] {
            assert_eq!(Page::Intro(0).after(press), Some(Step::To(Page::Intro(1))));
            assert_eq!(Page::Intro(1).after(press), Some(Step::To(Page::Main)));
        }
    }

    #[test]
    fn cinematics_lists_its_films_each_playing_back_to_the_list() {
        assert_eq!(
            Page::Main.after(Press::Button(Button::Cinematic)),
            Some(Step::To(Page::Cinematics))
        );
        assert_eq!(
            Page::Cinematics.after(Press::Film(Film::North)),
            Some(Step::To(Page::Film(Film::North)))
        );

        for press in [Press::Back, Press::Ended] {
            assert_eq!(
                Page::Film(Film::North).after(press),
                Some(Step::To(Page::Cinematics))
            );
        }

        for back in [Press::Back, Press::Button(Button::Cancel)] {
            assert_eq!(Page::Cinematics.after(back), Some(Step::To(Page::Main)));
        }
    }

    #[test]
    fn high_scores_open_from_the_main_menu_and_go_back_to_it() {
        assert_eq!(
            Page::Main.after(Press::Button(Button::HighScores)),
            Some(Step::To(Page::Scores))
        );
        assert_eq!(
            Page::Scores.after(Press::Button(Button::EraseScores)),
            Some(Step::EraseScores)
        );

        for back in [Press::Back, Press::Button(Button::MainMenu)] {
            assert_eq!(Page::Scores.after(back), Some(Step::To(Page::Main)));
        }
    }

    #[test]
    fn credits_play_both_videos_each_stopped_by_a_press() {
        assert_eq!(
            Page::Main.after(Press::Button(Button::Credits)),
            Some(Step::To(Page::Credits(0)))
        );
        assert_eq!(
            Page::Credits(0).after(Press::Back),
            Some(Step::To(Page::Credits(1)))
        );
        assert_eq!(
            Page::Credits(1).after(Press::Back),
            Some(Step::To(Page::Main))
        );
    }

    #[test]
    fn credits_play_each_video_to_its_end_then_the_main_menu() {
        assert_eq!(
            Page::Credits(0).after(Press::Ended),
            Some(Step::To(Page::Credits(1)))
        );
        assert_eq!(
            Page::Credits(1).after(Press::Ended),
            Some(Step::To(Page::Main))
        );
    }

    #[test]
    fn load_game_lists_the_profiles_saves() {
        assert_eq!(
            Page::Main.after(Press::Button(Button::LoadGame)),
            Some(Step::To(Page::Loads))
        );

        for back in [Press::Back, Press::Button(Button::Cancel)] {
            assert_eq!(Page::Loads.after(back), Some(Step::To(Page::Main)));
        }
    }

    #[test]
    fn adjust_settings_opens_the_editor_which_closes_to_the_main_menu() {
        assert_eq!(
            Page::Main.after(Press::Button(Button::AdjustSettings)),
            Some(Step::To(Page::Settings))
        );
        assert_eq!(
            Page::Settings.after(Press::Button(Button::Cancel)),
            Some(Step::To(Page::Main))
        );
        assert_eq!(Page::Settings.after(Press::Back), None);
    }

    #[test]
    fn the_profile_page_opens_from_the_main_menu_and_goes_back_to_it() {
        assert_eq!(
            Page::Main.after(Press::Profiles),
            Some(Step::To(Page::Profiles))
        );

        for back in [
            Press::Back,
            Press::Button(Button::Cancel),
            Press::Button(Button::Continue),
        ] {
            assert_eq!(Page::Profiles.after(back), Some(Step::To(Page::Main)));
        }
    }

    #[test]
    fn the_mods_page_opens_from_the_main_menu_and_goes_back_to_it() {
        assert_eq!(Page::Main.after(Press::Mods), Some(Step::To(Page::Mods)));

        for back in [Press::Back, Press::Button(Button::Cancel)] {
            assert_eq!(Page::Mods.after(back), Some(Step::To(Page::Main)));
        }
    }

    #[test]
    fn the_book_of_lore_opens_over_the_main_menu_only() {
        assert_eq!(Page::Main.after(Press::Lore), Some(Step::Lore));
        assert_eq!(Page::Map.after(Press::Lore), None);
    }

    #[test]
    fn the_hint_closes_to_the_map() {
        for ok in [Press::Back, Press::Button(Button::Ok)] {
            assert_eq!(Page::Hint(143).after(ok), Some(Step::To(Page::Map)));
        }
    }

    #[test]
    fn the_briefing_speaks_on_its_page_and_the_map_advisor_while_the_map_is_open() {
        let moves = [
            (Page::Main, Page::Map, Speech::Map),
            (Page::Map, Page::Hint(143), Speech::Keep),
            (Page::Hint(143), Page::Map, Speech::Keep),
            (Page::Map, Page::Erase, Speech::Keep),
            (Page::Map, Page::Extras, Speech::Keep),
            (Page::Map, Page::Background(4), Speech::Briefing(4)),
            (Page::Extras, Page::Background(33), Speech::Briefing(33)),
            (Page::Background(4), Page::Summary(4), Speech::Hush),
            (Page::Background(4), Page::Map, Speech::Map),
            (Page::Summary(4), Page::Map, Speech::Map),
            (Page::Map, Page::Main, Speech::Hush),
            (Page::Main, Page::Settings, Speech::Keep),
        ];

        for (from, to, speech) in moves {
            assert_eq!(from.speech(to), speech, "{from:?} to {to:?}");
        }
    }

    #[test]
    fn the_main_menu_and_map_play_the_theme_a_quests_pages_the_epic() {
        let pages = [
            (Page::Main, Some(music::THEME)),
            (Page::Map, Some(music::THEME)),
            (Page::Background(4), Some(music::EPIC)),
            (Page::Summary(4), Some(music::EPIC)),
            (Page::Profiles, None),
            (Page::Loads, None),
            (Page::Settings, None),
            (Page::Extras, None),
            (Page::Erase, None),
            (Page::Hint(143), None),
            (Page::Scores, None),
            (Page::Credits(0), None),
            (Page::Credits(1), None),
            (Page::Intro(0), None),
            (Page::Film(Film::North), None),
            (Page::Cinematics, Some(music::THEME)),
        ];

        for (page, track) in pages {
            assert_eq!(page.music(), track, "{page:?}");
        }
    }

    #[test]
    fn the_compass_opens_the_pick_page_whose_presets_start_a_game() {
        let presets = [
            (Button::BeginnerRandom, Preset::Beginner),
            (Button::AdvancedRandom, Preset::Advanced),
            (Button::ExpertRandom, Preset::Expert),
        ];

        assert_eq!(
            Page::Map.after(Press::Button(Button::FreestyleQuests)),
            Some(Step::To(Page::Freestyle))
        );

        for (button, preset) in presets {
            assert_eq!(
                Page::Freestyle.after(Press::Button(button)),
                Some(Step::Preset(preset))
            );
        }
    }

    #[test]
    fn manual_settings_opens_the_setup_page_whose_accept_starts_a_game() {
        assert_eq!(
            Page::Freestyle.after(Press::Button(Button::ManualSettings)),
            Some(Step::To(Page::Setup))
        );
        assert_eq!(
            Page::Setup.after(Press::Button(Button::Accept)),
            Some(Step::Custom)
        );
    }

    #[test]
    fn cancel_and_escape_leave_the_freestyle_pages_for_the_map() {
        for page in [Page::Freestyle, Page::Setup] {
            for back in [Press::Back, Press::Button(Button::Cancel)] {
                assert_eq!(page.after(back), Some(Step::To(Page::Map)), "{page:?}");
            }
        }
    }

    #[test]
    fn the_map_closes_for_the_freestyle_pages_and_opens_again_after() {
        let moves = [
            (Page::Map, Page::Freestyle, Speech::Hush),
            (Page::Freestyle, Page::Setup, Speech::Keep),
            (Page::Freestyle, Page::Map, Speech::Map),
            (Page::Setup, Page::Map, Speech::Map),
        ];

        for (from, to, speech) in moves {
            assert_eq!(from.speech(to), speech, "{from:?} to {to:?}");
        }

        assert_eq!(Page::Freestyle.music(), None);
        assert_eq!(Page::Setup.music(), None);
    }

    #[test]
    fn multiplayer_leads_through_the_lobby_and_cancel_goes_back_a_page() {
        let button = |page: Page, b| page.after(Press::Button(b));
        let back = [Press::Back, Press::Button(Button::Cancel)];

        assert_eq!(
            button(Page::Main, Button::Multiplayer),
            Some(Step::To(Page::Protocol))
        );
        assert_eq!(
            button(Page::Protocol, Button::Accept),
            Some(Step::To(Page::Multiplayer))
        );
        assert_eq!(
            button(Page::Multiplayer, Button::CreateGame),
            Some(Step::To(Page::Create))
        );
        assert_eq!(
            button(Page::Multiplayer, Button::JoinGame),
            Some(Step::Join)
        );
        assert_eq!(button(Page::Create, Button::Accept), Some(Step::Host));
        assert_eq!(button(Page::Lobby, Button::StartGame), Some(Step::StartLan));
        assert_eq!(button(Page::Lobby, Button::Ready), Some(Step::Ready));
        assert_eq!(button(Page::Lobby, Button::LoadGame), Some(Step::LoadLan));
        assert_eq!(
            button(Page::Lobby, Button::AdjustSettings),
            Some(Step::To(Page::LanSetup))
        );
        assert_eq!(button(Page::LanSetup, Button::Accept), Some(Step::Share));

        let backs = [
            (Page::Protocol, Page::Main),
            (Page::Multiplayer, Page::Protocol),
            (Page::Create, Page::Multiplayer),
            (Page::Lobby, Page::Multiplayer),
            (Page::LanSetup, Page::Lobby),
        ];

        for (from, to) in backs {
            for press in back {
                assert_eq!(from.after(press), Some(Step::To(to)), "{from:?}");
            }

            // The main menu's music plays on.
            assert_eq!(from.music(), None, "{from:?}");
        }
    }

    /// The editor and LOAD and SAVE open over either setup page and
    /// close back to it.
    #[test]
    fn a_setup_pages_editor_and_presets_close_back_to_it() {
        for over in [Over::Alone, Over::Lan] {
            let setup = over.page();
            let opened = [
                (Button::EditMenu, Page::Trees(over)),
                (Button::LoadOptions, Page::Presets(Mode::Load, over)),
                (Button::SaveOptions, Page::Presets(Mode::Save, over)),
            ];

            for (button, page) in opened {
                assert_eq!(setup.after(Press::Button(button)), Some(Step::To(page)));
                assert_eq!(page.after(Press::Back), Some(Step::To(setup)));
            }
        }
    }

    #[test]
    fn erase_victories_asks_first() {
        assert_eq!(
            Page::Map.after(Press::Button(Button::EraseVictories)),
            Some(Step::To(Page::Erase))
        );
        assert_eq!(
            Page::Erase.after(Press::Button(Button::Yes)),
            Some(Step::Erase)
        );

        for no in [Press::Back, Press::Button(Button::No)] {
            assert_eq!(Page::Erase.after(no), Some(Step::To(Page::Map)));
        }
    }
}
