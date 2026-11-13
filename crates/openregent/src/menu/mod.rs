//! The menus before a game: main menu, quest map, a quest's background
//! and summary, in the original's art. Start Quest makes the game and
//! plays it, as `--quest` would have. A game the command line asks for
//! is made the same way, the app opening on its loading page.
//!
//! ```text
//!  Main ──Play Game──> Map ──quest──> Background ──Continue──> Summary ──Start Quest──> game
//!   │                  │ │ │  └─Main Menu─> Main  └──Cancel──┴──> Map
//!   │                  │ │ └─Erase Victories──> Erase ──Yes, No──> Map
//!   │                  │ └─jewel──> Extras ──quest──> Background
//!   ├─Exit Game──> Quit (AP04) ──Yes──> Leaving ──played out──> exit
//!   │                └─No──> Main
//!   ├─Adjust Settings──> Settings (`editor`) ──Save, Cancel──> Main
//!   ├─Profile──> Profiles ──Continue, Cancel──> Main
//!   ├─Change Mods──> Mods ──Done, Cancel──> Main
//!   ├─Book of Lore──> the lore window (`ui::lore`) over Main, opened or closed
//!   ├─High Scores──> Scores (`scores`) ──Main Menu──> Main, the table kept
//!   ├─Credits──> Credits(0) ──a key or press──> Credits(1) ──> Main
//!   ├─Load Game──> Loads (`save::dialog`) ──Load──> game
//!   │                └─Cancel──> Main
//!   └─Multiplayer──> Protocol (AP42) ──Accept──> Multiplayer (AP33, `lan`)
//!                       └─Cancel──> Main       └─Cancel──> Protocol
//!                      ├─Create Game──> Create (AP55) ──Accept──> Lobby (MX19), the host's
//!                      │                  └─Cancel──> Multiplayer
//!                      └─Join Game──> Lobby, a guest's
//!                    Lobby ──Start Game (host), the host's start (guest)──> game
//!                      ├─Adjust Settings──> LanSetup (MX17, chat) ──Accept, Cancel──> Lobby
//!                      └─Cancel──> Multiplayer
//!
//!  Map ──compass──> Freestyle (APd2) ──a preset──> game
//!                     │ └─Cancel──> Map
//!                     └─Manual Settings──> Setup (`freestyle`) ──Accept──> game
//!                                            │ └─Cancel──> Map
//!                                            ├─BUILDING EDIT MENU──> Trees (`tree`) ──Save, Delete, Cancel──> Setup
//!                                            └─LOAD, SAVE──> Presets (`preset_page`) ──Load, Save, Cancel──> Setup
//!  LanSetup opens Trees and Presets as Setup does, which close back to it.
//!
//!  Screen::Menu ──Start Quest, a freestyle start, a LAN game seated, the command line's──> AP50 drawn ──> begin: Maker on a thread, draw::install ──> Screen::Playing
//!  Screen::Playing, AP50 over the game (`drape`; not a LAN game) ──its drawing settled──> play
//!  a quest's AP50 over the game, settled ──its Start Quest──> play (`Hold`)
//!  Screen::Menu ──Load──> its page drawn, no AP50 ──> begin
//!  Screen::Playing ──the end panel's Main Menu──> draw::uninstall ──> Screen::Menu, Main
//!  Screen::Playing ──Options, LOAD GAME──> draw::uninstall ──> Screen::Menu, the save made
//!  Screen::Playing ──Options, RESTART──> draw::uninstall ──> Screen::Menu, Summary(q) or Freestyle
//! ```
//!
//! A game asks its way back as a [`Return`] in [`Returning`], taken as
//! `Screen::Menu` opens ([`returned`]): after the game's resources are
//! reset, so the Load Game dialog a LOAD GAME opens is not.
//!
//! Each flow's state is a resource, owned by its module; [`Menus`]
//! takes them all as a system's parameter, [`Flows`] lends each as a
//! plain borrow for a press that crosses flows:
//!
//! ```text
//!  Menu        the page, its view, the game asked for   (here)
//!  Art         the pictures and words, read once        art
//!  Quests      the quests offered                       (here)
//!  Saves       Load Game, the game's dialogs too        save::dialog
//!  Lan         the Multiplayer pages and session        lan
//!  Editor      the mods page                            mods
//!  Scores      the high scores and where kept           scores
//!  Freestyle   the freestyle config and where kept      freestyle
//!  Extras      the extra quests' list                   extras
//!  Picker      the profile page                         profiles
//!  Video       the page's video                         cinema
//!  Lore        BOOK OF LORE pressed                     row
//! ```
//!
//! The game's dialogs and end panels borrow `Art` and `Quests` alone.
//! [`Opened`] holds them all before an app does (`main`, the tests).
//!
//! `page` holds the pages and their moves, `paint` draws them from `art`
//! (read once), `font` sets text, `fit` scales each page to the window,
//! `profiles` is the Profile button's page (`profile`), `freestyle` the
//! freestyle config, `setup` its page, `tree` its Build Tree Editor
//! (the trees on disk in `files::trees`), `preset_page` its LOAD and SAVE
//! (the settings on disk in `files::presets`), `scores` the high scores' and the
//! credits' stand-in, `lan` the Multiplayer pages, `row` the main menu's
//! Change Mods, Profile and Book of Lore.

pub mod art;
mod cinema;
mod drape;
mod extras;
pub mod faces;
mod films;
pub mod fit;
pub mod font;
pub mod freestyle;
mod lan;
mod mods;
mod page;
pub(crate) mod paint;
mod preset_page;
mod profiles;
mod row;
pub(crate) mod save_page;
mod scores;
pub mod settings;
mod setup;
mod tree;

use bevy::ecs::system::{SystemParam, SystemState};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

use regent_core::data::fourcc::FourCc;
use regent_core::view::map::cursor;
use regent_core::view::menu::NAME_TEXT;
use regent_core::view::menu::film::Film;
use regent_core::view::menu::presets::Mode;
use regent_core::view::menu::quest_map::{HINTS, Progress};

use crate::audio::{ClickSounds, Narration};
use crate::draw::shelf::Shelf;
use crate::files::config::{MainMenu, Settings};
use crate::files::profile::{Player, Profiles};
use crate::game::begin::{GameSource, Maker};
use crate::game::load::Offered;
use crate::save::dialog::Saves;
use crate::ui::options::Play;
use freestyle::Custom;
pub(crate) use page::Page;
use page::{Button, Over, Press, Speech, Step};
use regent_core::sim::mapgen::preset;

pub use art::Art;
pub use cinema::{Layer, Video, full_screen};
pub use drape::{Drape, lift, undraped};
pub use freestyle::Freestyle;
use paint::Shows;
pub use paint::click;
pub use row::Lore;
pub use scores::{Scores, Writes};

/// What the window shows: the menus, or a game. A game's systems run in
/// [`Screen::Playing`] only.
#[derive(States, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Screen {
    #[default]
    Menu,
    Playing,
}

/// Whether a game is in play: a run condition.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn in_game(screen: Option<Res<State<Screen>>>) -> bool {
    screen.is_some_and(|s| *s.get() == Screen::Playing)
}

/// The command line's game, as the loading page tells games apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadKind {
    Quest(FourCc),
    Freestyle,
}

/// A game asked for: a quest by index on a map word, a save and its
/// name, a freestyle game, or the command line's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wanted {
    Quest {
        quest: usize,
        word: i32,
    },
    Save {
        path: std::path::PathBuf,
        name: String,
    },
    Freestyle(Box<Custom>),
    Command(LoadKind),
    /// The LAN game the lobby seated (`lan`), held there.
    Lan,
}

/// A custom game's quest key, the verdict's.
const FREESTYLE_KEY: FourCc = regent_core::sim::world::defeat::End::Won.verdict().1;

/// A game asked for: the loading page shows first, then it is made. The
/// page shown is worked out once ([`Quests::shows`]: a save's reads it).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Start {
    Asked(Wanted),
    Shown(Wanted, Option<Shows>, Hold),
}

/// Whether a made game waits on its loading page's Start Quest: a quest
/// or quest save picked in the menus does, so its still is seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hold {
    /// The page goes by itself, the game made and its drawing settled.
    Free,
    /// Start Quest not yet offered: the game being made, or settling
    /// ([`drape`]).
    Making,
    /// Start Quest offered.
    Made,
}

impl Hold {
    /// How `wanted`, showing `shows`, starts.
    const fn of(wanted: &Wanted, shows: Option<Shows>) -> Hold {
        match (wanted, shows) {
            (Wanted::Quest { .. } | Wanted::Save { .. }, Some(Shows::Quest(_))) => Hold::Making,
            _ => Hold::Free,
        }
    }
}

/// The page shown and how it stands, and the game asked for. Each
/// flow's state is a resource of its own ([`Menus`]).
#[derive(Resource)]
pub struct Menu {
    page: Page,
    view: paint::View,
    start: Option<Start>,
    /// Why the last game would not start.
    failed: Option<String>,
    /// The loading page shown with no game made (`--menu loading`).
    held: Option<Shows>,
}

/// The quests offered, shared with a game being made off the frame
/// ([`begin`]).
#[derive(Resource, Clone, Default)]
pub struct Quests(pub std::sync::Arc<[Offered]>);

/// The menus' resources, as their systems take them: [`Flows`] once
/// lent.
#[derive(SystemParam)]
pub struct Menus<'w> {
    pub menu: ResMut<'w, Menu>,
    pub art: ResMut<'w, Art>,
    pub quests: ResMut<'w, Quests>,
    /// Load Game's dialog, the game's save dialogs' too.
    pub saves: ResMut<'w, Saves>,
    pub lan: ResMut<'w, lan::Lan>,
    pub mods: ResMut<'w, mods::Editor>,
    pub scores: ResMut<'w, Scores>,
    pub freestyle: ResMut<'w, Freestyle>,
    pub extras: ResMut<'w, extras::Extras>,
    pub picker: ResMut<'w, profiles::Picker>,
    pub video: ResMut<'w, Video>,
    pub lore: ResMut<'w, Lore>,
}

/// [`Menus`] lent, each flow a plain borrow: what a page's press
/// touches across flows.
pub struct Flows<'a> {
    pub menu: &'a mut Menu,
    pub art: &'a mut Art,
    pub quests: &'a mut Quests,
    pub saves: &'a mut Saves,
    /// The Multiplayer pages (`lan`).
    pub lan: &'a mut lan::Lan,
    /// The mods page, and the mods found (`mods`).
    pub mods: &'a mut mods::Editor,
    /// The high scores (`scores`).
    pub scores: &'a mut Scores,
    /// The freestyle config (`freestyle`).
    pub freestyle: &'a mut Freestyle,
    /// The extra quests' list (`extras`).
    pub extras: &'a mut extras::Extras,
    /// The profile page (`profiles`).
    pub picker: &'a mut profiles::Picker,
    /// The page's video (`cinema`).
    pub video: &'a mut Video,
    /// BOOK OF LORE (`row`).
    pub lore: &'a mut Lore,
}

/// The menus as they open, before an app holds them ([`Opened::insert`]):
/// a world of their resources alone.
pub struct Opened(World);

/// Where a game left for the menus goes ([`Returning`]).
pub enum Return {
    /// The main menu, as the end panel's Main Menu goes.
    Main,
    /// The LAN lobby, on the game's socket.
    Lobby(crate::net::Regroup),
    /// The start page of the game's quest, or of a freestyle game.
    Restart(Option<FourCc>),
    /// A save to load, picked in the game's load dialog of `place`.
    Load {
        path: std::path::PathBuf,
        name: String,
        place: Option<Place>,
    },
}

/// A game's way back to the menus, taken as they open again
/// ([`returned`]): after the game's resources are reset, so Load Game's
/// dialog it opens stays.
#[derive(Resource, Default)]
pub struct Returning(pub Option<Return>);

impl Menus<'_> {
    /// `f` run on `world`'s menus.
    pub fn with<T>(world: &mut World, f: impl FnOnce(&mut Flows) -> T) -> T {
        let mut state = SystemState::<Menus>::new(world);
        let mut menus = state.get_mut(world).expect("the menus' resources");
        let out = f(&mut menus.flows());

        state.apply(world);
        out
    }

    /// Each flow lent.
    pub fn flows(&mut self) -> Flows<'_> {
        Flows {
            menu: &mut self.menu,
            art: &mut self.art,
            quests: &mut self.quests,
            saves: &mut self.saves,
            lan: &mut self.lan,
            mods: &mut self.mods,
            scores: &mut self.scores,
            freestyle: &mut self.freestyle,
            extras: &mut self.extras,
            picker: &mut self.picker,
            video: &mut self.video,
            lore: &mut self.lore,
        }
    }
}

impl Flows<'_> {
    /// The quests `install` offers read again: a mod's quest added or
    /// changed since start. The old list kept where they will not read.
    pub fn quests_on(&mut self, install: &crate::files::env::Install) {
        let read = crate::quests(install).and_then(|quests| {
            self.art
                .quests_on(&install.hd, &install.sdk, &quests)
                .map(|()| quests)
        });

        match read {
            Ok(quests) => self.quests.0 = quests.into(),
            Err(e) => warn!("quests not read again: {e}"),
        }
    }

    /// The extra quests' list on `install`: each mod folder's quest named
    /// by its mod, checked for its files.
    pub fn extras_on(&mut self, install: &crate::files::env::Install) {
        *self.extras = extras::Extras::of(&self.quests.0, &self.art.book.quests, install);
    }

    /// Load Game's dialog marking a save's mods not among `install`'s.
    pub fn saves_on(&mut self, install: &crate::files::env::Install) {
        self.saves.found_in(&install.mods());
    }

    /// The profile a game plays as, asked nothing ([`profiles::Picker::profile`]).
    #[must_use]
    pub fn profile(&self) -> Option<crate::files::profile::Profile> {
        self.picker.profile(self.art)
    }

    /// The save at `path` asked for from a game: made once the game is
    /// taken down; the saves page, of `place`, shows where it will not
    /// load.
    pub fn load(&mut self, (path, name): (std::path::PathBuf, String), place: Option<Place>) {
        self.menu.page = Page::Loads;
        self.menu.view = paint::View::default();
        self.menu.failed = None;
        self.saves.show(
            crate::save::dialog::Dialog::Load,
            place,
            String::new(),
            (None, regent_core::view::menu::saves::Players::Alone),
        );
        self.menu.start = Some(Start::Asked(Wanted::Save { path, name }));
    }

    /// The page `--menu` opened, on `player`'s profile: Load Game's
    /// saves.
    pub fn played_by(&mut self, player: &Player) {
        if self.menu.page == Page::Loads {
            let place = saves_place(player, Play::Alone);

            self.saves.show(
                crate::save::dialog::Dialog::Load,
                place,
                String::new(),
                (None, regent_core::view::menu::saves::Players::Alone),
            );
        }
    }

    /// Back from a finished game: the main menu, as the original's end
    /// panel's Main Menu goes.
    pub fn back(&mut self) {
        self.menu.reset();
        self.lan.cancel();
    }

    /// Back from a LAN game to its lobby, on the game's socket, as the
    /// original's end panel's "Return to Lobby" goes (menus return,
    /// a network session's branch): the host hosts again, a guest asks
    /// its host again, its LAN saves `player`'s.
    pub fn regroup(&mut self, regroup: crate::net::Regroup, player: &Player) {
        self.menu.reset();
        self.join_saves(player);
        self.lan.regroup(regroup, &self.freestyle.custom);
        self.menu.page = Page::Lobby;
    }

    /// Back from a restarted game: a quest's
    /// summary, `quest` the game's, as the original's menus resume on
    /// `AP44`; a freestyle game's pick page, a new Map Number rolled, as
    /// they resume on `AP63`. A quest not
    /// offered, the main menu (**Inferred** stand-in: none should be).
    pub fn restart(&mut self, quest: Option<FourCc>) {
        let offered = quest.map(|key| self.quests.index(key));

        self.back();
        self.menu.page = match offered {
            Some(Some(index)) => Page::Summary(index),
            Some(None) => Page::Main,
            None => {
                self.freestyle.custom.map_number = preset::map_number(freestyle::clock());
                Page::Freestyle
            }
        };
    }

    /// The lobby joined as `player`: a save the host loads is looked
    /// for, and one sent goes, in its LAN saves.
    fn join_saves(&mut self, player: &Player) {
        self.lan
            .set_saves(saves_place(player, Play::Lan).map(|(dir, _)| dir));
    }

    /// The freestyle config hosted on a port the system picks, the
    /// lobby page open, and the port: a test's.
    #[cfg(test)]
    pub fn host_lan(&mut self) -> u16 {
        let port = self.lan.host_any(&self.freestyle.custom).expect("hosted");

        self.menu.page = Page::Lobby;
        port
    }

    /// The host on this machine's `port` joined, the lobby open: a
    /// test's.
    #[cfg(test)]
    pub fn join_lan(&mut self, port: u16) {
        self.lan.join_local(port);
        self.menu.page = Page::Lobby;
    }

    /// On the page `spec` names (`--menu`): see [`opening`].
    ///
    /// # Errors
    ///
    /// A page or quest it does not know.
    pub fn open(&mut self, spec: &str) -> Result<(), String> {
        let keys: Vec<FourCc> = self.quests.0.iter().map(|q| q.quest.key).collect();

        // `PAGE@FRAME`: the page's video held at that frame.
        let spec = match spec.rsplit_once('@') {
            Some((page, frame)) => {
                let frame = frame
                    .parse()
                    .map_err(|_| format!("--menu {spec}: no frame {frame}"))?;

                self.video.at = Some(frame);
                page
            }
            None => spec,
        };

        if let Some(shows) = held(spec, &keys) {
            self.menu.held = Some(shows?);
        } else {
            let (page, view) = opening(spec, &keys)?;

            if page == Page::Profiles {
                self.picker.open(self.art.book.words.gmtx(NAME_TEXT));
            }

            if page == Page::Scores {
                self.scores.read();
            }

            // `extras:KEY` lights that quest's row.
            if let (Page::Extras, Some(quest)) = (page, view.hover) {
                self.extras.light(quest);
            }

            self.menu.view = view;

            // `lore:CODE` opens the Book of Lore on that entry.
            if let Some(("lore", code)) = spec.split_once(':') {
                let code = FourCc::from_prefix(code.as_bytes())
                    .ok()
                    .filter(|_| code.len() == 4)
                    .ok_or_else(|| format!("--menu {spec}: want a code, e.g. AVA1"))?;

                *self.lore = Lore(true, Some(code));
            }

            // `tree:NAME` opens the editor on that tree.
            if let Some(("tree", name)) = spec.split_once(':') {
                name.clone_into(&mut self.freestyle.custom.buildings);
            }

            // `mods:GUIDS` opens the mods page with those enabled; `off`
            // among them, with loading off.
            if page == Page::Mods {
                let loading = if spec.split([':', ',']).any(|k| k == "off") {
                    crate::files::mod_list::Loading::Off
                } else {
                    crate::files::mod_list::Loading::On
                };
                let enabled: Vec<crate::files::mod_list::Entry> = spec
                    .split_once(':')
                    .map(|(_, list)| {
                        list.split(',')
                            .filter_map(regent_core::data::guid::Guid::parse)
                            .map(|g| crate::files::mod_list::Entry::of(g, true))
                            .collect()
                    })
                    .unwrap_or_default();

                self.mods.open(&enabled, loading);
            }

            // `lobby:N` and `lan-setup:N[:ID]` host N players.
            if let Some(("lobby" | "lan-setup", rest)) = spec.split_once(':') {
                let n = rest.split(':').next().unwrap_or_default();
                let n = n
                    .parse()
                    .map_err(|_| format!("--menu {spec}: not 2 to 4"))?;

                self.lan.set_players(n)?;
            }

            if matches!(page, Page::Setup | Page::LanSetup | Page::Trees(_)) {
                self.freestyle.custom.resolve(&self.art.lists.trees);
            }

            if let Some(players) = setup_players(self.lan, page) {
                self.freestyle.custom.fit(self.art.lists.dialog(players));
            }

            if let Page::Trees(_) = page {
                self.menu.view.tree = tree::Editor::of(&self.freestyle.custom);
            }

            if let Page::Presets(mode, over) = page {
                self.menu.view.presets = io_of(self, mode, over);
            }

            // The session named as the profile; the lobby and its
            // settings shown hosting this config on a port the system
            // picks.
            if matches!(page, Page::Create | Page::Lobby | Page::LanSetup) {
                let nick = self
                    .profile()
                    .map(|p| p.nick().to_owned())
                    .unwrap_or_default();

                self.lan.name(&nick);

                if page != Page::Create {
                    self.lan.host_anywhere(&self.freestyle.custom, nick)?;
                }
            }

            self.menu.page = page;
        }

        Ok(())
    }
}

impl Opened {
    /// On the main menu, `quests` offered, drawn with `art`.
    #[must_use]
    pub fn new(quests: Vec<Offered>, art: Art) -> Opened {
        let mut world = World::new();

        world.insert_resource(Menu::new());
        world.insert_resource(art);
        world.insert_resource(Quests(quests.into()));
        world.insert_resource(Saves::default());
        world.insert_resource(lan::Lan::default());
        world.insert_resource(mods::Editor::default());
        world.insert_resource(Scores::default());
        world.insert_resource(Freestyle::default());
        world.insert_resource(extras::Extras::default());
        world.insert_resource(profiles::Picker::new(Profiles::default_place()));
        world.insert_resource(Video::default());
        world.insert_resource(Lore::default());
        Opened(world)
    }

    /// `f` run on the menus.
    pub fn with<T>(&mut self, f: impl FnOnce(&mut Flows) -> T) -> T {
        Menus::with(&mut self.0, f)
    }

    /// The menus' resources put in `world`, over any there.
    pub fn insert(mut self, world: &mut World) {
        fn moved<T: Resource>(from: &mut World, to: &mut World) {
            if let Some(resource) = from.remove_resource::<T>() {
                to.insert_resource(resource);
            }
        }

        let from = &mut self.0;

        moved::<Menu>(from, world);
        moved::<Art>(from, world);
        moved::<Quests>(from, world);
        moved::<Saves>(from, world);
        moved::<lan::Lan>(from, world);
        moved::<mods::Editor>(from, world);
        moved::<Scores>(from, world);
        moved::<Freestyle>(from, world);
        moved::<extras::Extras>(from, world);
        moved::<profiles::Picker>(from, world);
        moved::<Video>(from, world);
        moved::<Lore>(from, world);
    }

    /// The art service's sources the menus draw from (`art::Kept`).
    #[must_use]
    pub fn sources(&self) -> [regent_draw::art::ArtSource; 2] {
        self.0.resource::<Art>().ink.sources()
    }

    /// Whether the command line's game is still to make.
    #[must_use]
    pub fn commanded(&self) -> bool {
        self.0.resource::<Menu>().commanded()
    }

    /// On the pick page, `preset` just pressed on Map Number `map_number`;
    /// no art, so no event.
    #[cfg(test)]
    pub fn starting_freestyle(
        quests: Vec<Offered>,
        preset: preset::Preset,
        map_number: i32,
    ) -> Opened {
        let mut custom = Custom {
            map_number,
            ..Custom::default()
        };
        let mut opened = Opened::new(quests, Art::default());

        custom.preset(preset, 1, &[]);
        opened.with(|m| {
            m.menu.page = Page::Freestyle;
            m.menu.start = Some(Start::Asked(Wanted::Freestyle(Box::new(custom))));
        });
        opened
    }

    /// On `quest`'s summary, Start Quest just pressed on [`TEST_WORD`];
    /// no art.
    #[cfg(test)]
    pub fn starting(quests: Vec<Offered>, quest: usize) -> Opened {
        let mut opened = Opened::new(quests, Art::default());

        opened.with(|m| m.menu.ask(quest));
        opened
    }
}

impl Quests {
    /// The offered quest keyed `key`'s place in the list.
    fn index(&self, key: FourCc) -> Option<usize> {
        self.0.iter().position(|q| q.quest.key == key)
    }

    /// The loading page `wanted` shows: a quest's or freestyle's. A save
    /// shows its game's, a departure: the original's load makes no `AP50`.
    fn shows(&self, wanted: &Wanted) -> Option<Shows> {
        match wanted {
            Wanted::Quest { quest, .. } => Some(Shows::Quest(*quest)),
            Wanted::Freestyle(_) | Wanted::Lan | Wanted::Command(LoadKind::Freestyle) => {
                Some(Shows::Freestyle)
            }
            Wanted::Command(LoadKind::Quest(key)) => self.quest_shows(&key.to_string()),
            Wanted::Save { path, .. } => match crate::save::glance(path).map(|g| g.meta.game) {
                Ok(crate::save::SaveKind::Quest { key, .. }) => self.quest_shows(&key),
                _ => Some(Shows::Freestyle),
            },
        }
    }

    /// The loading page of the offered quest keyed `key`.
    fn quest_shows(&self, key: &str) -> Option<Shows> {
        self.0
            .iter()
            .position(|q| q.quest.key.to_string() == key)
            .map(Shows::Quest)
    }

    /// [`Quests::pog`] in the menus' `art`: its source and picture.
    #[must_use]
    pub fn pog_pic(&self, art: &Art, quest: Option<FourCc>) -> Option<(art::Pack, art::Pic)> {
        let book = &art.book;
        let shown = match quest {
            Some(key) => self.index(key).and_then(|i| book.quests.get(i)),
            None => book.freestyle_game.as_ref(),
        }?;

        Some((shown.source, shown.icon?))
    }

    /// The round pog of `quest`'s game, record and set, as its title bar
    /// shows it (`regent_core::view::menu::icon`); a custom game's, `FRE1`'s,
    /// the expansion's unplayable set (`the_save_dialogs_pog_is_the_games_quests`).
    #[must_use]
    pub fn pog(&self, art: &Art, quest: Option<FourCc>) -> Option<(FourCc, i32)> {
        match quest {
            Some(key) => self
                .index(key)
                .and_then(|i| art.book.quests.get(i)?.icon)
                .map(|p| (p.record, p.set)),
            None => regent_core::view::menu::icon(
                FREESTYLE_KEY,
                regent_core::view::menu::Release::Expansion,
                regent_core::view::menu::World::Freestyle,
            )
            .map(|(record, set)| (record, i32::from(set))),
        }
    }
}

impl Menu {
    /// On the main menu.
    #[must_use]
    fn new() -> Menu {
        Menu {
            page: Page::Main,
            view: paint::View::default(),
            start: None,
            failed: None,
            held: None,
        }
    }

    /// Whether the lore window may show on this page.
    pub fn is_main(&self) -> bool {
        self.page == Page::Main && self.start.is_none() && self.held.is_none()
    }

    /// Whether this start leads to the main menu, the intro first or not:
    /// no command line game or other page asked for.
    pub const fn opens_main(&self) -> bool {
        matches!(self.page, Page::Main | Page::Intro(_))
            && self.start.is_none()
            && self.held.is_none()
    }

    /// The start-up intro first, then the main menu.
    pub const fn intro(&mut self) {
        self.page = Page::Intro(0);
    }

    /// Whether the page is a full-screen video, which hides the cursor.
    #[must_use]
    pub const fn hides_cursor(&self) -> bool {
        matches!(self.page, Page::Credits(_) | Page::Intro(_) | Page::Film(_))
    }

    /// Whether the page stops the music: a film's video
    /// and the intro, which plays before any
    /// (prefs registry load starts it after, `main`).
    #[must_use]
    pub const fn hushes_music(&self) -> bool {
        matches!(self.page, Page::Intro(_) | Page::Film(_))
    }

    /// The command line's game, `game`, asked for: the app opens on its
    /// loading page, then plays it.
    pub fn command(&mut self, game: LoadKind) {
        self.start = Some(Start::Asked(Wanted::Command(game)));
    }

    /// Whether the command line's game is still to make.
    #[must_use]
    pub const fn commanded(&self) -> bool {
        matches!(
            self.start,
            Some(Start::Asked(Wanted::Command(_)) | Start::Shown(Wanted::Command(_), ..))
        )
    }

    fn reset(&mut self) {
        self.page = Page::Main;
        self.view = paint::View::default();
        self.start = None;
        self.failed = None;
    }

    /// The page shown.
    #[cfg(test)]
    pub const fn page(&self) -> Page {
        self.page
    }

    /// Start Quest pressed on the summary shown, as the player presses
    /// it; false on any other page.
    #[cfg(test)]
    pub fn start_quest(&mut self) -> bool {
        let step = self.page.after(Press::Button(Button::StartQuest));

        if let Some(Step::Start(quest)) = step {
            self.start = Some(Start::Asked(quest_on_clock(quest)));
        }

        step.is_some()
    }

    /// The map opened at `zoom`, window pixels a map pixel, about the
    /// hovered quest (`--zoom`, for screenshots).
    pub fn zoom(&mut self, zoom: f32) {
        self.view = std::mem::take(&mut self.view).zoomed(zoom);
    }

    /// The track the page's screen asks ([`Page::music`]); `None` keeps
    /// what plays, and none plays before the command line's game.
    #[must_use]
    pub fn music(&self) -> Option<i32> {
        if self.commanded() {
            None
        } else {
            self.page.music()
        }
    }

    /// The cursor shown, `CUR1` set and slot, and its picture in `art`:
    /// the busy one while a game is made, else
    /// `Dial`'s ([`art::dialog`]).
    #[must_use]
    pub fn cursor<'a>(
        &self,
        art: &'a Art,
    ) -> Option<((u32, usize), &'a regent_draw::art::cursor::Picture)> {
        let key = match self.start {
            Some(Start::Shown(_, _, Hold::Made)) | None => art::dialog(),
            Some(_) => Some(cursor::BUSY),
        }?;

        art.cursors
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(k, p)| (*k, p))
    }

    /// Whether Start Quest is offered: the game made and held, its
    /// drawing settled.
    #[cfg(test)]
    pub const fn offered(&self) -> bool {
        matches!(self.start, Some(Start::Shown(_, _, Hold::Made)))
    }

    /// The loading page's Start Quest pressed ahead, as the player
    /// presses it once offered.
    #[cfg(test)]
    pub const fn go(&mut self) {
        if let Some(Start::Shown(_, _, hold)) = &mut self.start {
            *hold = Hold::Free;
        }
    }

    /// Start Quest pressed on `quest`'s summary, on [`TEST_WORD`].
    #[cfg(test)]
    pub fn ask(&mut self, quest: usize) {
        self.page = Page::Summary(quest);
        self.start = Some(Start::Asked(Wanted::Quest {
            quest,
            word: TEST_WORD,
        }));
    }
}

/// The players whose lists the setup page `page` offers: one alone,
/// the lobby's for a LAN game's; none for another page.
fn setup_players(lan: &lan::Lan, page: Page) -> Option<u32> {
    match page {
        Page::Setup => Some(regent_core::sim::freestyle::ONE_PLAYER),
        Page::LanSetup => Some(lan.players()),
        _ => None,
    }
}

/// A game's way back taken, as the menus open again: the main menu, the
/// lobby, a restart's page, or the save its load dialog picked.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn returned(mut menus: Menus, mut returning: ResMut<Returning>, player: Res<Player>) {
    let mut flows = menus.flows();

    match returning.0.take() {
        Some(Return::Main) => flows.back(),
        Some(Return::Lobby(regroup)) => flows.regroup(regroup, &player),
        Some(Return::Restart(quest)) => flows.restart(quest),
        Some(Return::Load { path, name, place }) => flows.load((path, name), place),
        None => {}
    }
}

/// The tests' fixed map word: `--seed`'s default, the quiet day's.
#[cfg(test)]
const TEST_WORD: i32 = 0x0b60_9e2c;

/// `quest` asked for on a map word read off the clock, as the original's
/// quest start reads it: each start a new map.
fn quest_on_clock(quest: usize) -> Wanted {
    Wanted::Quest {
        quest,
        word: freestyle::clock(),
    }
}

/// Whether `--menu`'s `k` names quest `key`: its code, or a mod quest's
/// GUID (`quest::key_of`).
fn named(key: FourCc, k: &str) -> bool {
    key.to_string().trim() == k
        || regent_core::data::guid::Guid::parse(k)
            .is_some_and(|id| regent_core::content::quest::key_of(id) == key)
}

/// The loading page `spec` holds, if it names one: `loading` a freestyle
/// game's, `loading:KEY` the quest's.
fn held(spec: &str, keys: &[FourCc]) -> Option<Result<Shows, String>> {
    let quest = |k: &str| {
        keys.iter()
            .position(|q| named(*q, k))
            .map(Shows::Quest)
            .ok_or_else(|| format!("--menu {spec}: no quest {k}"))
    };

    match spec.split_once(':') {
        Some(("loading", key)) => Some(quest(key)),
        None if spec == "loading" => Some(Ok(Shows::Freestyle)),
        _ => None,
    }
}

/// The page `spec` names, and how the map stands on it: `main[:ID]`
/// (control `ID` hovered), `map[-top|-bottom][:KEY]` (the map scrolled to
/// an end, the quest or, a number, the control hovered), `extras`, `settings`, `mods[:GUIDS]` (those
/// enabled, by commas), `hint` (the first),
/// `background:KEY`, `summary:KEY`, `lore:CODE` (the Book of Lore on that entry), `tree[:NAME]` (the Build Tree Editor, on the tree NAME), `load-options`, `save-options`, `setup[:ID]` (combo box `ID`'s list
/// open), `scores`, `credits`, `cinematic` (every film listed), `ending` (`MV07`), `multiplayer`, `create`, `lobby[:N]`,
/// `lan-setup[:N[:ID]]` (the last two hosting the config for N players, 2
/// by default, on a port the system picks; ID as `setup`'s); `keys` the
/// offered quests'.
fn opening(spec: &str, keys: &[FourCc]) -> Result<(Page, paint::View), String> {
    let (name, key) = spec
        .split_once(':')
        .map_or((spec, None), |(n, k)| (n, Some(k)));

    if name == "main" {
        let pin = key
            .map(|k| {
                k.parse()
                    .map_err(|_| format!("--menu {spec}: no control {k}"))
            })
            .transpose()?;

        return Ok((Page::Main, paint::View::pointed(pin)));
    }

    // The editor's key is a tree's name, taken by `Flows::open`.
    if name == "tree" {
        return Ok((Page::Trees(Over::Alone), paint::View::default()));
    }

    // The lore's key is an entry, taken by `Flows::open`.
    if name == "lore" {
        return Ok((Page::Main, paint::View::default()));
    }

    // The mods page's key is its enabled GUIDs, taken by `Flows::open`.
    if name == "mods" {
        return Ok((Page::Mods, paint::View::default()));
    }

    // `lan-setup:N[:ID]`: N players, taken by `Flows::open`; combo box
    // `ID`'s list open.
    if name == "lan-setup" {
        let id = key
            .and_then(|k| k.split_once(':'))
            .map(|(_, id)| {
                id.parse()
                    .map_err(|_| format!("--menu {spec}: no control {id}"))
            })
            .transpose()?;
        let mut view = paint::View::default();

        view.setup = setup::Setup::listing_of(id);
        return Ok((Page::LanSetup, view));
    }

    // A number on the map is a control held hovered: the jewel, compass.
    let pin: Option<i32> = key.and_then(|k| k.parse().ok());
    let quest = key
        .filter(|_| pin.is_none())
        .map(|k| {
            keys.iter()
                .position(|q| named(*q, k))
                .ok_or_else(|| format!("--menu {spec}: no quest {k}"))
        })
        .transpose()?;
    let asked = |page: fn(usize) -> Page| {
        quest
            .map(page)
            .ok_or_else(|| format!("--menu {spec}: name a quest, as {name}:QU01"))
    };
    let map = |scroll| Ok((Page::Map, paint::View::pinned(scroll, quest, pin)));

    match name {
        "map" => map(None),
        "map-top" => map(Some(0.0)),
        "map-bottom" => map(Some(f32::NEG_INFINITY)),
        "extras" => Ok((Page::Extras, paint::View::pinned(None, quest, None))),
        "settings" => Ok((Page::Settings, paint::View::default())),
        "profiles" => Ok((Page::Profiles, paint::View::default())),
        "loads" => Ok((Page::Loads, paint::View::default())),
        "erase" => Ok((Page::Erase, paint::View::default())),
        "quit" => Ok((Page::Quit, paint::View::default())),
        "hint" => Ok((Page::Hint(HINTS[0]), paint::View::default())),
        "background" => asked(Page::Background).map(|p| (p, paint::View::default())),
        "summary" => asked(Page::Summary).map(|p| (p, paint::View::default())),
        "freestyle" => Ok((Page::Freestyle, paint::View::default())),
        "load-options" => Ok((
            Page::Presets(Mode::Load, Over::Alone),
            paint::View::default(),
        )),
        "save-options" => Ok((
            Page::Presets(Mode::Save, Over::Alone),
            paint::View::default(),
        )),
        "scores" => Ok((Page::Scores, paint::View::default())),
        "credits" => Ok((Page::Credits(0), paint::View::default())),
        "intro" => Ok((Page::Intro(0), paint::View::default())),
        "cinematic" => Ok((Page::Cinematics, paint::View::default())),
        "ending" => Ok((Page::Film(Film::North), paint::View::default())),
        "protocol" => Ok((Page::Protocol, paint::View::default())),
        "multiplayer" => Ok((Page::Multiplayer, paint::View::default())),
        "create" => Ok((Page::Create, paint::View::default())),
        "lobby" => Ok((Page::Lobby, paint::View::default())),
        "setup" => {
            let mut view = paint::View::default();

            view.setup = setup::Setup::listing_of(pin);
            Ok((Page::Setup, view))
        }
        _ => Err(format!(
            "--menu {spec}: want main[:ID], map, map-top, map-bottom, extras[:KEY], settings, lore:CODE, profiles, mods[:GUIDS], loads, erase, hint, background:KEY, summary:KEY, freestyle, setup[:ID], tree[:NAME], load-options, save-options, scores, credits, intro, cinematic, ending, protocol, multiplayer, create, lobby[:N], lan-setup[:N[:ID]] or loading[:KEY], any page then @FRAME to hold its video"
        )),
    }
}

/// The menus' systems, while [`Screen::Menu`]; the loading page over a
/// game just put in ([`drape`]).
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        use crate::game::scope::Scoped as _;

        app.init_resource::<Returning>()
            .per_game::<Drape>()
            .add_systems(OnEnter(Screen::Playing), drape::began)
            .add_systems(Last, lift.after(regent_draw::settle::Noted).run_if(in_game))
            .add_systems(
                EguiPrimaryContextPass,
                drape::shown
                    .after(crate::draw::frame::Ui::Over)
                    .run_if(in_game)
                    .run_if(not(undraped)),
            )
            .add_systems(OnEnter(Screen::Menu), returned)
            .add_systems(Update, (begin, rescan).run_if(in_state(Screen::Menu)))
            .add_systems(EguiPrimaryContextPass, show.run_if(in_state(Screen::Menu)))
            .add_systems(
                EguiPrimaryContextPass,
                crate::ui::lore::show
                    .after(show)
                    .run_if(in_state(Screen::Menu)),
            );
    }
}

/// The Mods page's Rescan ([`rescanned`]).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn rescan(mut menus: Menus, install: Option<Res<crate::files::env::Install>>) {
    if let (true, Some(install)) = (menus.mods.take_rescan(), install) {
        rescanned(&mut menus.flows(), &install);
    }
}

/// The install's mod folders scanned again for every clone (a menu
/// game's maker too), the pages listing mods told.
fn rescanned(f: &mut Flows, install: &crate::files::env::Install) {
    install.rescan();
    f.mods.on(install);
    f.lan.rehash();
    f.quests_on(install);
    f.extras_on(install);
    f.saves_on(install);
}

/// Draw the page and take its press, Escape as back; a game asked for,
/// the loading page.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(clippy::too_many_lines, reason = "every page's press, in one place")]
fn show(
    mut contexts: EguiContexts,
    (mut menus, mut shared): (Menus, ResMut<regent_draw::art::Art>),
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<Settings>,
    mut editor: ResMut<crate::ui::editor::Editor>,
    (mut player, mut narration, mut clicks): (
        ResMut<Player>,
        ResMut<Narration>,
        ResMut<ClickSounds>,
    ),
    (mut exit, time): (MessageWriter<AppExit>, Res<Time>),
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut f = menus.flows();

    // egui's pages at the interface's scale.
    fit::styled(ctx);

    // Last pass's press going down: its click.
    if let Some(wave) = paint::click(ctx).and_then(|w| f.art.clicks.get(&w)) {
        clicks.0.push(wave);
    }

    // A loading page held for a screenshot.
    if let (None, Some(shows)) = (&f.menu.start, f.menu.held) {
        let shown = (shows, paint::Offer::Nothing);

        paint::loading(
            ctx,
            (&mut *f.art, &mut shared),
            shown,
            &f.quests.0,
            Layer::Menus,
        );
        return Ok(());
    }

    // The lobby polled: a game seated starts as a freestyle start does.
    if f.menu.start.is_none() && lan::ours(f.menu.page) && f.lan.poll(std::time::Instant::now()) {
        f.menu.start = Some(Start::Asked(Wanted::Lan));
    }

    match f.menu.start.take() {
        Some(start @ (Start::Asked(_) | Start::Shown(..))) => {
            starting(ctx, (&mut f, &mut shared), &settings, start);
        }
        None => {
            let before = settings.ui.main_menu;
            let main = main_art(&settings);
            let none = Progress::default();
            let progress = player.profile.as_ref().map_or(&none, |p| &p.progress);
            let unlocked = progress.films();

            shared.set(settings.enhancements.ui_upscale);

            // The first start, no profile existing: ask for its name.
            if f.menu.is_main() && f.picker.ask() {
                f.picker.open(f.art.book.words.gmtx(NAME_TEXT));
                f.menu.page = Page::Profiles;
            }

            // The page's video, its frame due now.
            f.video.play(f.menu.page, f.art);

            if let Some(video) = f.video.playing_mut() {
                video.tick(ctx, time.delta(), settings.enhancements.video_upscale);
            }

            // The name prompt takes Escape as its Cancel, the setup page's
            // open list its close, a Multiplayer page's list or box its own.
            let naming = f.menu.page == Page::Profiles && f.picker.naming();
            let listing = (matches!(f.menu.page, Page::Setup | Page::LanSetup)
                && f.menu.view.setup.listing())
                || matches!(
                    f.menu.page,
                    Page::Trees(_) | Page::Presets(..) | Page::Loads
                )
                || (lan::ours(f.menu.page) && lan::busy(ctx));

            f.menu.view.video = f.video.playing().and_then(crate::video::Playing::frame);

            let drawn = paint::page(
                ctx,
                (&mut *f.art, &mut shared, &mut f.menu.view),
                f.menu.page,
                &f.quests.0,
                (main, f.menu.failed.as_deref(), progress),
            );
            let listed = (f.menu.page == Page::Extras)
                .then(|| {
                    extras::page(
                        ctx,
                        (&mut *f.art, &mut shared),
                        &mut *f.extras,
                        (&f.quests.0, progress),
                    )
                })
                .flatten();
            let chose = (f.menu.page == Page::Cinematics)
                .then(|| films::page(ctx, (&mut *f.art, &mut shared), &mut f.video.films))
                .flatten();
            let picked = (f.menu.page == Page::Profiles)
                .then(|| {
                    profiles::page(ctx, &mut *f.picker, &mut player, (&mut *f.art, &mut shared))
                })
                .flatten();
            let modded = (f.menu.page == Page::Mods)
                .then(|| mods::page(ctx, &mut *f.mods))
                .flatten();
            let loads = (f.menu.page == Page::Loads)
                .then(|| loads(ctx, &mut f, &mut shared))
                .flatten();
            // The lobby's LOAD GAME list over it takes the presses; so
            // does the mods page over it, the host's MODS.
            let lan_loading = f.menu.page == Page::Lobby && f.saves.open.is_some();

            if lan_loading {
                lan_loads(ctx, &mut f, &mut shared);
            }

            if f.menu.page == Page::Lobby && f.lan.take_mods_asked() {
                let (entries, loading) = f.lan.mod_list();

                f.mods.open_lobby(&entries, loading);
            }

            let lan_mods = f.menu.page == Page::Lobby && f.mods.over_lobby();

            if lan_mods {
                lobby_mods(ctx, &mut f);
            }

            let escape = (keys.just_pressed(KeyCode::Escape)
                && !naming
                && !listing
                && !lan_loading
                && !lan_mods)
                .then_some(Press::Back);
            let edited = (f.menu.page == Page::Settings)
                .then(|| {
                    crate::ui::editor::page(
                        ctx,
                        &mut editor,
                        (&mut *f.art, &mut shared),
                        &mut settings,
                    )
                })
                .flatten()
                .map(|_| Press::Button(Button::Cancel));
            let failed = f.menu.failed.as_deref();
            let set = matches!(f.menu.page, Page::Setup | Page::LanSetup)
                .then(|| {
                    let lan = (f.menu.page == Page::LanSetup).then_some(&mut *f.lan);

                    setup::page(
                        ctx,
                        (&mut *f.art, &mut shared, &mut f.menu.view.setup),
                        &mut f.freestyle.custom,
                        (failed, lan),
                    )
                })
                .flatten();
            let edited_tree = matches!(f.menu.page, Page::Trees(_))
                .then(|| {
                    tree::page(
                        ctx,
                        (&mut *f.art, &mut shared, &mut f.menu.view.tree),
                        &mut f.freestyle.custom,
                    )
                })
                .flatten();
            let preset = matches!(f.menu.page, Page::Presets(..))
                .then(|| {
                    preset_page::page(
                        ctx,
                        (&mut *f.art, &mut shared, &mut f.menu.view.presets),
                        &mut f.freestyle.custom,
                    )
                })
                .flatten();
            let scored = (f.menu.page == Page::Scores)
                .then(|| {
                    scores::page(
                        ctx,
                        (&mut *f.art, &mut shared, &mut f.scores.shown),
                        &f.scores.table,
                    )
                })
                .flatten();
            // A full-screen page with no video to play goes on at once.
            let credited = match (f.menu.page, f.video.shown()) {
                (Page::Credits(_) | Page::Intro(_) | Page::Film(_), Some(shown)) => {
                    cinema::full_screen(ctx, (&mut *f.art, &mut shared), shown, Layer::Menus)
                }
                (Page::Credits(_) | Page::Intro(_) | Page::Film(_), None) => Some(Press::Ended),
                _ => None,
            };
            let multiplayer = lan::page(
                ctx,
                (&mut *f.art, &mut shared),
                f.menu.page,
                &mut *f.lan,
                failed,
                // Only the games page edits the profile (the nick): lent on
                // every page, the player would read as changed every frame.
                (f.menu.page == Page::Multiplayer)
                    .then(|| player.profile.as_mut())
                    .flatten(),
            )
            .filter(|_| !lan_loading && !lan_mods);

            // Its art changed in the editor: the room starts over.
            if settings.ui.main_menu != before {
                f.menu.view = paint::View::default();
            }

            match drawn
                .or(edited)
                .or(modded)
                .or(listed)
                .or(chose)
                .or(set)
                .or(edited_tree)
                .or(preset)
                .or(scored)
                .or(credited)
                .or(multiplayer)
                .or(picked)
                .or(loads)
                .or(escape)
                .and_then(|press| f.menu.page.after(press))
            {
                Some(Step::To(page)) => {
                    // The map keeps its scroll behind the extras' list and
                    // the question, the room its motion behind the editor
                    // and the profile page.
                    let kept = matches!(
                        (f.menu.page, page),
                        (Page::Map, Page::Extras | Page::Erase | Page::Hint(_))
                            | (Page::Extras | Page::Erase | Page::Hint(_), Page::Map)
                            | (
                                Page::Main,
                                Page::Settings
                                    | Page::Profiles
                                    | Page::Mods
                                    | Page::Loads
                                    | Page::Quit
                            )
                            | (
                                Page::Settings
                                    | Page::Profiles
                                    | Page::Mods
                                    | Page::Loads
                                    | Page::Quit,
                                Page::Main
                            )
                            | (Page::Quit, Page::Leaving)
                    );

                    if !kept {
                        f.menu.view = paint::View::default();
                    }

                    // The mods page opens on the settings' list.
                    if page == Page::Mods {
                        f.mods.open(
                            &crate::files::mod_list::read(),
                            crate::files::mod_list::loading(),
                        );
                    }

                    // Back from a film the list keeps its row.
                    if f.menu.page == Page::Main && page == Page::Cinematics {
                        f.video.films = films::Films::of(unlocked);
                    }

                    if page == Page::Extras {
                        f.extras.open();
                    }

                    profiled(&mut f, page, &mut player);
                    configured(&mut f, page);
                    networked(&mut f, page, &mut player);

                    if f.menu.page == Page::Scores {
                        f.scores.write();
                    }

                    if page == Page::Scores {
                        f.scores.read();
                    }

                    f.menu.page = speak(&f, page, &mut player, &mut narration);
                    f.menu.failed = None;
                }
                Some(Step::Lore) => f.lore.0 = true,
                Some(Step::Erase) => {
                    erase(&mut player);
                    f.menu.page = Page::Map;
                }
                Some(Step::EraseScores) => {
                    let game = f.scores.shown.game;

                    f.scores.table.erase(game);
                    f.scores.shown.scroll = 0.0;
                }
                Some(Step::Start(quest)) => {
                    f.menu.start = Some(Start::Asked(quest_on_clock(quest)));
                }
                Some(Step::Preset(preset)) => freestyle_start(&mut f, Some(preset)),
                Some(Step::Custom) => freestyle_start(&mut f, None),
                // A port that will not bind is said on Multiplayer Setup.
                Some(Step::Host) => {
                    f.freestyle.keep();
                    f.menu.failed = None;
                    f.freestyle.custom.fit(f.art.lists.dialog(f.lan.players()));
                    f.freestyle.custom.cheats = lan::NEW_CHEATS;

                    match f.lan.host(&f.freestyle.custom, nick(&player)) {
                        Ok(()) => lobby(&mut f),
                        Err(why) => {
                            f.menu.failed = Some(why);
                            f.menu.page = Page::Multiplayer;
                        }
                    }
                }
                Some(Step::Join) => {
                    f.join_saves(&player);
                    f.lan.join(nick(&player));
                    lobby(&mut f);
                    f.menu.failed = None;
                }
                Some(Step::StartLan) => f.menu.failed = f.lan.start().err(),
                Some(Step::Ready) => f.lan.flip_ready(),
                Some(Step::LoadLan) => show_lan_loads(&mut f, &player),
                Some(Step::Share) => {
                    f.lan.share(&f.freestyle.custom);
                    f.freestyle.keep();
                    f.menu.page = Page::Lobby;
                }
                Some(Step::Exit) => {
                    exit.write(AppExit::Success);
                }
                None => {}
            }
        }
    }

    Ok(())
}

/// What shows while the game `start` asks for is made: its loading page,
/// worked out once, with Start Quest once offered; a game
/// with none loads under the page it was picked on, the command line's
/// under nothing.
fn starting(
    ctx: &bevy_egui::egui::Context,
    (f, shared): (&mut Flows, &mut regent_draw::art::Art),
    settings: &Settings,
    start: Start,
) {
    let (wanted, shows, hold) = match start {
        Start::Asked(wanted) => {
            let shows = f.quests.shows(&wanted);
            let hold = Hold::of(&wanted, shows);

            (wanted, shows, hold)
        }
        Start::Shown(wanted, shows, hold) => (wanted, shows, hold),
    };
    let offer = match hold {
        Hold::Made => paint::Offer::StartQuest,
        Hold::Free | Hold::Making => paint::Offer::Nothing,
    };

    let hold = match shows {
        Some(shows) => {
            let page = (shows, offer);
            let pressed =
                paint::loading(ctx, (&mut *f.art, shared), page, &f.quests.0, Layer::Menus);

            if pressed { Hold::Free } else { hold }
        }
        None if matches!(wanted, Wanted::Command(_)) => hold,
        None => {
            let under = (main_art(settings), None, &Progress::default());

            paint::page(
                ctx,
                (&mut *f.art, shared, &mut f.menu.view),
                f.menu.page,
                &f.quests.0,
                under,
            );

            hold
        }
    };

    f.menu.start = Some(Start::Shown(wanted, shows, hold));
}

/// The main menu's art the settings pick.
const fn main_art(settings: &Settings) -> art::Pack {
    match settings.ui.main_menu {
        MainMenu::Base => art::Pack::Base,
        MainMenu::Expansion => art::Pack::Expansion,
    }
}

/// The freestyle config's game asked for, `preset` written over it first
/// if given.
fn freestyle_start(f: &mut Flows, preset: Option<preset::Preset>) {
    if let Some(preset) = preset {
        f.freestyle
            .custom
            .preset(preset, freestyle::clock(), &f.art.lists.ratings);
    }

    f.freestyle.keep();

    let wanted = Wanted::Freestyle(Box::new(f.freestyle.custom.clone()));

    f.menu.start = Some(Start::Asked(wanted));
}

/// The freestyle config as `page` opens: kept, leaving the setup page;
/// its tree loaded, opening the setup page, as the original's dialog
/// fills Building Limitations; the editor
/// on that tree; a Map Number rolled, opening the pick page.
fn configured(f: &mut Flows, page: Page) {
    if f.menu.page == Page::Setup {
        f.freestyle.keep();
    }

    if matches!(page, Page::Setup | Page::LanSetup) {
        f.freestyle.custom.resolve(&f.art.lists.trees);
    }

    if let Some(players) = setup_players(f.lan, page) {
        f.freestyle.custom.fit(f.art.lists.dialog(players));
    }

    if let Page::Trees(_) = page {
        f.menu.view.tree = tree::Editor::of(&f.freestyle.custom);
    }

    if let Page::Presets(mode, over) = page {
        f.menu.view.presets = io_of(f, mode, over);
    }

    if (f.menu.page, page) == (Page::Map, Page::Freestyle) {
        f.freestyle.custom.map_number = preset::map_number(freestyle::clock());
    }
}

/// The LOAD or SAVE page opening as `mode` on the config, over the setup
/// page `over`: alone, or the lobby's players.
fn io_of(f: &Flows, mode: Mode, over: Over) -> preset_page::Io {
    let default_name = &f.art.lists.save_name;
    let players = match over {
        Over::Alone => crate::files::presets::SOLO,
        Over::Lan => f.lan.players(),
    };

    preset_page::Io::of(mode, &f.freestyle.custom, players, default_name)
}

/// The multiplayer pages as `page` opens: a profile to be named by, as
/// Play Game's, and the build hashed ahead for the lobby; the session
/// dialog named as the player, a new Map Number rolled as the pick page
/// rolls one; a lobby left, given up; the host's settings opened, kept
/// to put back, and cancelled, put back; LAN discovery stopped off
/// Multiplayer Setup.
fn networked(f: &mut Flows, page: Page, player: &mut Player) {
    if page == Page::Multiplayer {
        if player.profile.is_none() {
            player.profile = f.profile();
        }

        f.lan.warm();
        f.lan.nick = nick(player);
    }

    if page == Page::Create {
        let rolled = preset::map_number(freestyle::clock());

        f.freestyle.custom.map_number = rolled.max(freestyle::MAP_NUMBER_MIN);
        f.lan.name(&nick(player));
    }

    // LAN discovery's port let go once Multiplayer Setup is left.
    if !matches!(page, Page::Multiplayer | Page::Create) {
        f.lan.unbrowse();
    }

    match (f.menu.page, page) {
        (Page::Lobby, Page::Multiplayer) => f.lan.cancel(),
        (Page::Lobby, Page::LanSetup) => f.lan.adjust(&f.freestyle.custom),
        (Page::LanSetup, Page::Lobby) => f.lan.put_back(&mut f.freestyle.custom),
        _ => {}
    }
}

/// This player's profile nick, or empty with no profile.
fn nick(player: &Player) -> String {
    player
        .profile
        .as_ref()
        .map(|p| p.nick().to_owned())
        .unwrap_or_default()
}

/// The profile `page` opens on: the profile page's list; Play Game's
/// and Load Game's, the last used, else a new one, asked nothing; Load
/// Game's saves.
fn profiled(f: &mut Flows, page: Page, player: &mut Player) {
    if page == Page::Profiles {
        f.picker.open(f.art.book.words.gmtx(NAME_TEXT));
    }

    if matches!(page, Page::Map | Page::Loads) && player.profile.is_none() {
        player.profile = f.profile();
    }

    if page == Page::Loads {
        let place = saves_place(player, Play::Alone);

        f.saves.show(
            crate::save::dialog::Dialog::Load,
            place,
            String::new(),
            (None, regent_core::view::menu::saves::Players::Alone),
        );
    }
}

/// A profile's saves directory and its name.
pub type Place = (std::path::PathBuf, String);

/// The player's profile's saves directory for games of `play` and its
/// name, if a profile is open.
pub fn saves_place(player: &Player, play: Play) -> Option<Place> {
    player
        .profile
        .as_ref()
        .map(|p| (crate::save::dir(&p.dir, play), p.name.clone()))
}

/// Load Game's dialog over the main menu: a save picked starts it, Cancel
/// goes back; it takes Escape as its CANCEL.
fn loads(
    ctx: &bevy_egui::egui::Context,
    f: &mut Flows,
    shared: &mut regent_draw::art::Art,
) -> Option<Press> {
    if let Some(why) = f.menu.failed.take() {
        f.saves.said = Some(why);
    }

    let chose = save_page::page(ctx, (&mut *f.art, shared), f.saves)?;

    chosen(f, chose)
}

/// What Load Game's `chose` does: a save asked for, or back.
fn chosen(f: &mut Flows, chose: crate::save::dialog::Chose) -> Option<Press> {
    match chose {
        crate::save::dialog::Chose::Load(path) => {
            let name = crate::save::glance(&path)
                .map_or_else(|_| path.display().to_string(), |g| g.meta.name);

            f.menu.start = Some(Start::Asked(Wanted::Save { path, name }));
            None
        }
        crate::save::dialog::Chose::Cancel => {
            f.saves.close();
            Some(Press::Button(Button::Cancel))
        }
        crate::save::dialog::Chose::Save { .. } => None,
    }
}

/// The lobby's LOAD GAME list, the host's: a save picked is offered to
/// the guests ([`lan::Lan::choose`]), or why not said; Cancel closes it.
fn lan_loads(ctx: &bevy_egui::egui::Context, f: &mut Flows, shared: &mut regent_draw::art::Art) {
    let chose = save_page::page(ctx, (&mut *f.art, shared), f.saves);

    match chose {
        Some(crate::save::dialog::Chose::Load(path)) => match f.lan.choose(&path) {
            Ok(()) => f.saves.close(),
            Err(why) => f.saves.said = Some(why),
        },
        Some(crate::save::dialog::Chose::Cancel) => f.saves.close(),
        _ => {}
    }
}

/// The mods page over the host's lobby, its rows marked where a player
/// lacks the mod, as the lobby stands now: Done gives the lobby its
/// list, which every guest then sees.
fn lobby_mods(ctx: &bevy_egui::egui::Context, f: &mut Flows) {
    f.mods.set_problems(f.lan.mod_problems());

    if mods::page(ctx, f.mods).is_some()
        && let Some(list) = f.mods.take_chosen()
    {
        f.lan.set_mods(list, &f.freestyle.custom);
    }
}

/// The lobby's page, Load Game's list closed: a save picked there
/// leaves it open, and the lobby shows it as its own LOAD GAME list.
fn lobby(f: &mut Flows) {
    f.menu.page = Page::Lobby;
    f.saves.close();
}

/// The lobby's LOAD GAME list: `player`'s LAN saves of as many
/// players as the lobby's.
fn show_lan_loads(f: &mut Flows, player: &Player) {
    let place = saves_place(player, Play::Lan);
    let players = usize::try_from(f.lan.players()).unwrap_or_default();
    let lan = regent_core::view::menu::saves::Players::Lan(players);

    f.saves.show(
        crate::save::dialog::Dialog::Load,
        place,
        String::new(),
        (None, lan),
    );
}

/// Erase Victories, answered yes: the profile's records gone and saved.
fn erase(player: &mut Player) {
    if let Some(profile) = &mut player.profile {
        profile.progress.erase();

        if let Err(why) = profile.save() {
            warn!("{why}");
        }
    }
}

/// The page a move to `to` shows, its voice started or stopped
/// ([`Page::speech`]). The map opening runs its advisor and hint
/// (`Progress::open_map`) and saves what they marked; a hint shows over
/// it. Its lifting cloud never comes in HD, so none is drawn.
fn speak(f: &Flows, to: Page, player: &mut Player, narration: &mut Narration) -> Page {
    let said = |sound: Option<FourCc>| sound.and_then(|s| f.art.voices.get(&s).cloned());

    match f.menu.page.speech(to) {
        Speech::Keep => to,
        Speech::Hush => {
            narration.hush();
            to
        }
        Speech::Briefing(quest) => {
            let key = f.quests.0.get(quest).map(|q| q.quest.key);

            narration.say(said(key.and_then(regent_core::view::menu::voice)));
            to
        }
        Speech::Map => {
            let opening = player.profile.as_mut().map(|profile| {
                let before = (profile.progress.seen, profile.progress.last_won);
                let opening = profile.progress.open_map(&f.art.book.words.rules);

                if before != (profile.progress.seen, profile.progress.last_won)
                    && let Err(why) = profile.save()
                {
                    warn!("{why}");
                }

                opening
            });

            narration.say(said(opening.and_then(|o| o.voice)));
            opening.and_then(|o| o.hint).map_or(to, Page::Hint)
        }
    }
}

/// A game being made off the frame, the maker and shelf it took: the
/// loading page keeps drawing meanwhile.
#[derive(Resource)]
struct Making {
    job: std::thread::JoinHandle<Job>,
    began: std::time::Instant,
}

/// A making's maker and shelf, given back, and its game.
type Job = (Maker, Shelf, Result<crate::game::begin::Made, String>);

/// Once the loading page is drawn, make the game on a thread; once made,
/// install and play it on this one, under its loading page ([`drape`])
/// until its drawing settles: a held game's then waits for Start Quest.
/// A game that will not start is said on its page, the command line's
/// ends the app.
///
/// ```text
///  Shown, none making ──> Making: Maker, Shelf, Arc'd quests to a thread
///  Shown, Making ──finished──> Maker, Shelf back ──> draw::install ──> Screen::Playing,
///                              the page over it (not a LAN game's)
/// ```
fn begin(world: &mut World) {
    let shown = match &world.resource::<Menu>().start {
        Some(Start::Shown(wanted, ..)) => Some(wanted.clone()),
        _ => None,
    };

    if let Some(wanted) = shown {
        match world.remove_resource::<Making>() {
            None => making(world, wanted),
            Some(m) if m.job.is_finished() => {
                let (maker, shelf, made) = m
                    .job
                    .join()
                    .unwrap_or_else(|e| std::panic::resume_unwind(e));

                info!("game made in {:.2} s", m.began.elapsed().as_secs_f32());
                world.insert_resource(maker);
                world.insert_resource(shelf);
                played(world, &wanted, made);
            }
            Some(m) => world.insert_resource(m),
        }
    }
}

/// Wait for the game being made, if any: a test's, to step frames as
/// when the making ran on the frame.
#[cfg(test)]
pub fn settle(world: &World) {
    if let Some(m) = world.get_resource::<Making>() {
        while !m.job.is_finished() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

/// `wanted`'s game made on a thread, the maker and shelf with it.
fn making(world: &mut World, wanted: Wanted) {
    let mut maker = world.remove_resource::<Maker>().expect("a maker");

    // A new game loads the mods enabled now, none with loading off.
    maker.set_mods(&crate::files::mod_list::chosen(
        &crate::files::mod_list::read(),
        crate::files::mod_list::loading(),
    ));

    let mut shelf = world.remove_resource::<Shelf>().unwrap_or_default();
    let seated = world.resource_mut::<lan::Lan>().take();
    let quests = world.resource::<Quests>().0.clone();
    let job = std::thread::spawn(move || {
        let made = made(&mut maker, &mut shelf, (&wanted, &quests), seated);

        (maker, shelf, made)
    });

    world.insert_resource(Making {
        job,
        began: std::time::Instant::now(),
    });
}

/// The game `wanted` asks for, `quests` the offered, `seated` the LAN
/// game the lobby seated.
fn made(
    maker: &mut Maker,
    shelf: &mut Shelf,
    (wanted, quests): (&Wanted, &[Offered]),
    seated: Option<crate::game::begin::Command>,
) -> Result<crate::game::begin::Made, String> {
    match wanted {
        Wanted::Quest { quest, word } => quests
            .get(*quest)
            .ok_or_else(|| format!("no quest {quest}"))
            .and_then(|offered| {
                let source = GameSource::Quest {
                    offered,
                    word: *word,
                };

                maker.make(source, shelf)
            }),
        Wanted::Save { path, .. } => maker.make(GameSource::Save { path, quests }, shelf),
        Wanted::Freestyle(custom) => maker.make(GameSource::Freestyle(custom), shelf),
        Wanted::Command(_) => maker.make(GameSource::Command, shelf),
        Wanted::Lan => seated.map_or_else(
            || Err("no LAN game seated".to_owned()),
            |lan| maker.make(GameSource::Lan(Box::new(lan)), shelf),
        ),
    }
}

/// `made` installed and played, under its loading page but a LAN game's;
/// a game that will not start is said on its page, the command line's
/// ends the app.
fn played(world: &mut World, wanted: &Wanted, made: Result<crate::game::begin::Made, String>) {
    match made.and_then(|made| crate::draw::install(world, made)) {
        Ok(()) => {
            world.resource_mut::<Player>().recorded = false;

            // The game's save dialogs open on their own, not on what the
            // menus' Load Game left.
            world.resource_mut::<Saves>().fresh();

            if let Some(mut kept) = world.get_resource_mut::<crate::files::scores::Scoring>() {
                kept.scored = false;
            }

            // The page stays until the game's first frames' drawing work
            // is done. A LAN game's peers start as they always did.
            if !matches!(wanted, Wanted::Lan) {
                world.insert_resource(Drape::up());
            }

            world
                .resource_mut::<NextState<Screen>>()
                .set(Screen::Playing);
        }
        Err(why) if matches!(wanted, Wanted::Command(_)) => {
            warn!("{why}");
            world.write_message(AppExit::error());
        }
        Err(why) => {
            let mut menu = world.resource_mut::<Menu>();

            warn!("{why}");
            menu.start = None;
            menu.failed = Some(why);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mod's quest put in a mod folder is offered after a Rescan, in
    /// the extra quests' list too: no restart.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_rescan_offers_a_new_mods_quest() {
        const WILD: &str = "2892764983";

        let scratch = crate::testdata::scratch("rescan");
        let dir = scratch.path().to_path_buf();

        let install = crate::files::env::Install::from_env()
            .expect("MAJESTY_HD")
            .with_mod_folders(vec![dir.clone()]);
        let mut opened = Opened::new(crate::quests(&install).expect("quests"), Art::default());
        let before = opened.with(|m| m.quests.0.len());

        opened.with(|m| m.extras_on(&install));

        let workshop =
            crate::files::env::path(crate::files::env::WORKSHOP).expect("MAJESTY_WORKSHOP");
        std::os::unix::fs::symlink(workshop.join(WILD), dir.join(WILD)).expect("linked");
        opened.with(|m| {
            rescanned(m, &install);

            assert_eq!(m.quests.0.len(), before + 1);
            assert_eq!(m.art.book.quests.len(), m.quests.0.len());
            assert!(m.extras.lists(before), "listed with the extras");
        });
    }

    /// Load Game's list, open as its page opens it.
    fn listing() -> Opened {
        let mut opened = Opened::new(Vec::new(), Art::default());

        opened.with(|m| {
            m.menu.page = Page::Loads;
            m.saves.show(
                crate::save::dialog::Dialog::Load,
                None,
                String::new(),
                (None, regent_core::view::menu::saves::Players::Alone),
            );
        });
        opened
    }

    /// Load Game's list closes on CANCEL, and the lobby opens without
    /// it: else it comes back over the lobby as its LOAD GAME list.
    #[test]
    fn the_load_game_list_does_not_come_back_over_the_lobby() {
        listing().with(|m| {
            assert_eq!(
                chosen(m, crate::save::dialog::Chose::Cancel),
                Some(Press::Button(Button::Cancel))
            );
            assert_eq!(m.saves.open, None);
        });

        // A save picked keeps it: a load that fails says why in it.
        listing().with(|m| {
            chosen(m, crate::save::dialog::Chose::Load("a.tar.zst".into()));
            lobby(m);

            assert_eq!((m.menu.page, m.saves.open), (Page::Lobby, None));
        });
    }

    /// A custom game started keeps its config, and the next session's
    /// menus open on it, a new Map Number to roll.
    #[test]
    fn a_started_freestyle_config_is_kept_for_the_next_session() {
        let scratch = crate::testdata::scratch("menu_kept");
        let path = scratch.path().join("menu_kept.toml");

        Opened::new(Vec::new(), Art::default()).with(|m| {
            m.freestyle.keep_at(path.clone());
            m.freestyle.custom.gold = 12_000;
            m.freestyle.custom.map_number = 42;
            freestyle_start(m, None);
        });

        Opened::new(Vec::new(), Art::default()).with(|m| {
            m.freestyle.keep_at(path);

            assert_eq!(m.freestyle.custom.gold, 12_000);
            assert_eq!(m.freestyle.custom.map_number, Custom::default().map_number);
        });
    }
    #[test]
    fn a_menu_page_is_named_with_its_quest() {
        let keys = [FourCc(*b"QU01"), FourCc(*b"XQ10")];
        let page = |spec| opening(spec, &keys).map(|(page, _)| page);

        assert_eq!(page("main"), Ok(Page::Main));
        assert_eq!(page("main:5100"), Ok(Page::Main));
        assert!(page("main:map").is_err());
        assert_eq!(page("map-top:XQ10"), Ok(Page::Map));
        assert_eq!(page("summary:XQ10"), Ok(Page::Summary(1)));
        assert_eq!(page("background:QU01"), Ok(Page::Background(0)));
        assert!(page("summary").is_err());
        assert!(page("summary:QU99").is_err());
        assert_eq!(page("credits"), Ok(Page::Credits(0)));
        assert_eq!(page("scores"), Ok(Page::Scores));
        assert_eq!(page("profiles"), Ok(Page::Profiles));
        assert_eq!(page("loads"), Ok(Page::Loads));
        assert_eq!(page("erase"), Ok(Page::Erase));
        assert_eq!(page("quit"), Ok(Page::Quit));
        assert_eq!(page("hint"), Ok(Page::Hint(143)));
    }

    #[test]
    fn a_loading_page_is_held_by_its_quest_or_freestyle() {
        let keys = [FourCc(*b"QU01"), FourCc(*b"XQ10")];

        assert_eq!(held("loading:XQ10", &keys), Some(Ok(Shows::Quest(1))));
        assert_eq!(held("loading", &keys), Some(Ok(Shows::Freestyle)));
        assert!(matches!(held("loading:QU99", &keys), Some(Err(_))));
        assert_eq!(held("summary:QU01", &keys), None);
    }

    #[test]
    fn a_quest_picked_in_the_menus_waits_for_start_quest() {
        let quest = Wanted::Quest { quest: 3, word: 0 };
        let save = Wanted::Save {
            path: std::path::PathBuf::from("a.gmp"),
            name: String::new(),
        };
        let command = Wanted::Command(LoadKind::Quest(FourCc(*b"QU01")));

        assert_eq!(Hold::of(&quest, Some(Shows::Quest(3))), Hold::Making);
        assert_eq!(Hold::of(&save, Some(Shows::Quest(3))), Hold::Making);
        assert_eq!(Hold::of(&save, Some(Shows::Freestyle)), Hold::Free);
        assert_eq!(Hold::of(&command, Some(Shows::Quest(0))), Hold::Free);
        assert_eq!(Hold::of(&Wanted::Lan, Some(Shows::Freestyle)), Hold::Free);
        assert_eq!(Hold::of(&quest, None), Hold::Free);
    }

    #[test]
    fn every_start_and_load_shows_a_loading_page() {
        let menu = Quests::default();
        let save = Wanted::Save {
            path: std::path::PathBuf::from("a.gmp"),
            name: String::new(),
        };
        let quest = Wanted::Quest { quest: 3, word: 0 };
        let freestyle = Wanted::Freestyle(Box::default());

        assert_eq!(menu.shows(&quest), Some(Shows::Quest(3)));
        assert_eq!(menu.shows(&freestyle), Some(Shows::Freestyle));
        // An unreadable save shows freestyle's page.
        assert_eq!(menu.shows(&save), Some(Shows::Freestyle));
        assert_eq!(
            menu.shows(&Wanted::Command(LoadKind::Freestyle)),
            Some(Shows::Freestyle)
        );

        // A quest the menus do not offer shows none.
        assert_eq!(
            menu.shows(&Wanted::Command(LoadKind::Quest(FourCc(*b"QU01")))),
            None
        );
    }
}

#[cfg(test)]
mod video_tests {
    use super::*;

    /// Menus offering one quest, on map slot `slot`.
    fn with_quest(slot: Option<usize>) -> Opened {
        let mut art = Art::default();

        art.book.quests = vec![art::Quest {
            source: art::Pack::Base,
            slot,
            icon: None,
            zoom: None,
            segue: None,
        }];

        Opened::new(Vec::new(), art)
    }

    #[test]
    fn credits_play_mv06_then_mv01() {
        with_quest(None).with(|m| {
            assert_eq!(Video::of(m.art, Page::Credits(0)), Some(FourCc(*b"MV06")));
            assert_eq!(Video::of(m.art, Page::Credits(1)), Some(FourCc(*b"MV01")));
            assert_eq!(Video::of(m.art, Page::Credits(2)), None);
        });
    }

    /// The briefing's video starts as its page opens, and is its quest
    /// slot's; the summary plays none.
    #[test]
    fn a_briefing_plays_its_quest_slots_video() {
        with_quest(Some(13)).with(|m| {
            assert_eq!(
                Video::of(m.art, Page::Background(0)),
                Some(FourCc(*b"QM13"))
            );
            assert_eq!(Video::of(m.art, Page::Summary(0)), None);
        });
        with_quest(None).with(|m| {
            assert_eq!(Video::of(m.art, Page::Background(0)), None);
        });
    }

    /// A page with a video missing keeps it as missing, and leaving the
    /// page drops it.
    #[test]
    fn a_missing_video_is_kept_as_missing_until_the_page_closes() {
        with_quest(Some(0)).with(|m| {
            m.menu.page = Page::Background(0);
            m.video.play(m.menu.page, m.art);
            assert!(matches!(m.video.shown(), Some((id, None)) if id == FourCc(*b"QM00")));

            m.menu.page = Page::Main;
            m.video.play(m.menu.page, m.art);
            assert!(m.video.shown().is_none());
        });
    }

    #[test]
    fn a_page_at_a_frame_holds_its_video_there() {
        with_quest(None).with(|m| {
            m.open("credits@640").unwrap();
            assert_eq!(m.menu.page, Page::Credits(0));
            assert_eq!(m.video.at, Some(640));
            assert!(m.open("credits@x").is_err());
        });
    }

    /// A film plays its own video; the intro `MV02`, `MV04`.
    #[test]
    fn a_film_plays_its_video() {
        with_quest(None).with(|m| {
            let video = |m: &Flows, page| Video::of(m.art, page);

            assert_eq!(video(m, Page::Film(Film::Opening)), Some(FourCc(*b"MV04")));
            assert_eq!(video(m, Page::Film(Film::North)), Some(FourCc(*b"MV07")));
            assert_eq!(video(m, Page::Cinematics), None);

            assert_eq!(video(m, Page::Intro(0)), Some(FourCc(*b"MV02")));
            assert_eq!(video(m, Page::Intro(1)), Some(FourCc(*b"MV04")));
        });

        // `--menu ending` opens on `MV07`.
        with_quest(None).with(|m| {
            m.open("ending").unwrap();
            assert_eq!(m.menu.page, Page::Film(Film::North));
            assert_eq!(Video::of(m.art, m.menu.page), Some(FourCc(*b"MV07")));
        });
    }

    /// The intro's and a film's videos come from the cinematics'
    /// archives, `MV07` from the expansion's; the rest from the datasets'.
    #[test]
    fn each_video_is_read_from_its_archive() {
        let mut art = Art::default();
        let archive = |source: Option<crate::video::VideoSource>| match source {
            Some(crate::video::VideoSource::Archive(path)) => Some(path),
            _ => None,
        };

        art.cinematics = vec!["base.dat".into(), "mx.dat".into()];
        art.videos
            .insert(FourCc(*b"MV06"), crate::video::held(b"BIK"));

        let mv02 = cinema::source_of(&art, Page::Intro(0), FourCc(*b"MV02"));
        let mv04 = cinema::source_of(&art, Page::Film(Film::Opening), FourCc(*b"MV04"));
        let mv07 = cinema::source_of(&art, Page::Film(Film::North), FourCc(*b"MV07"));
        let mv06 = cinema::source_of(&art, Page::Credits(0), FourCc(*b"MV06"));

        assert_eq!(archive(mv02), Some("base.dat".into()));
        assert_eq!(archive(mv04), Some("base.dat".into()));
        assert_eq!(archive(mv07), Some("mx.dat".into()));
        assert!(matches!(
            mv06,
            Some(crate::video::VideoSource::Record(r)) if r.data == b"BIK"
        ));
        assert!(cinema::source_of(&art, Page::Credits(1), FourCc(*b"MV01")).is_none());
    }

    /// The intro plays before any music; a film stops it.
    #[test]
    fn the_intro_and_a_film_stop_the_music() {
        let mut menu = Menu::new();

        for (page, hushed) in [
            (Page::Intro(1), true),
            (Page::Film(Film::Opening), true),
            (Page::Cinematics, false),
            (Page::Credits(0), false),
            (Page::Main, false),
        ] {
            menu.page = page;
            assert_eq!(menu.hushes_music(), hushed, "{page:?}");
        }
    }

    /// The main menu's Load Game lists the single-player folder alone,
    /// the lobby's LOAD GAME the LAN one; a joined lobby's guest takes
    /// a save sent into the LAN one.
    #[test]
    fn each_load_list_and_the_guests_saves_are_its_kinds_folder() {
        let (_dir, player) = crate::save::dialog::two_folders("menus");
        let lan = crate::save::dir(&player.profile.as_ref().expect("profile").dir, Play::Lan);

        with_quest(None).with(|m| {
            m.menu.page = Page::Loads;
            m.played_by(&player);
            assert_eq!(m.saves.names(), ["solo"]);

            show_lan_loads(m, &player);
            assert_eq!(m.saves.names(), ["lan"]);

            m.join_saves(&player);
            assert_eq!(m.lan.saves(), Some(lan.as_path()));
        });
    }

    #[test]
    fn only_a_full_screen_video_hides_the_cursor() {
        let mut menu = Menu::new();

        for page in [Page::Credits(1), Page::Intro(0), Page::Film(Film::North)] {
            menu.page = page;
            assert!(menu.hides_cursor(), "{page:?}");
        }

        menu.page = Page::Background(0);
        assert!(!menu.hides_cursor());
    }
}
