//! The multiplayer pages (`net`) in the original's art: Multiplayer
//! Setup `AP33`, the session dialog `AP55`, the lobby `MX19`, and the
//! host's settings, the setup page `MX17` with its chat (`setup`).
//!
//! ```text
//!  Main ──Multiplayer──> Protocol (AP42): LAN or internet
//!          └──ACCEPT──> Multiplayer (AP33): the open games LAN discovery hears (a
//!                        row pressed fills HOST:PORT), HOST[:PORT] to join, the port to host on
//!   ├─CREATE GAME──> Create (AP55): name, players ──ACCEPT: port bound──> Lobby, the host's
//!   ├─JOIN GAME (an address that reads)──> Lobby, a guest's: host looked up, seat asked
//!   └─CANCEL──> Main
//!  Lobby (MX19): settings, seats by nick and build, ready, chat
//!   ├─host: ADJUST SETTINGS──> LanSetup (MX17, chat) ──ACCEPT: the guests see it──> Lobby
//!   │       LOAD GAME──> a LAN save picked: offered, sent to who lacks it;
//!   │                    a seat's line pressed moves it to the next saved seat
//!   │       START GAME, every seat taken and every guest ready──> game
//!   ├─guest: Ready flips; the host's start──> game
//!   └─CANCEL──> Multiplayer, the lobby let go
//!  a LAN game's end, Return to Lobby──> Lobby again on the game's socket
//!   (`Lan::regroup`): the host's with its guests seated first, a guest's
//!   waiting while the host plays on
//! ```
//!
//! AP42 offers LAN and internet over iroh. AP33's open games list holds the games LAN discovery hears
//! (`net::Finder`), each checked against this build, and under them three
//! boxes, the host to join, the port to host on and the player's nick.
//!
//! The window never waits on the network: the lobby's client
//! ([`net::Session`]) and LAN discovery are polled each pass
//! ([`Lan::poll`]); the client runs what may block on a thread aside.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy_egui::egui::{self, Color32, Rect, pos2, vec2};
use regent_core::data::fourcc::FourCc;
use regent_core::sim::world::cheat::Cheats;
use regent_core::view::menu::hotkey;
use regent_core::view::menu::multiplayer::{games, lobby, protocol, session};
use regent_core::view::menu::{self, Layout, ShownControl, State, Tick};

use super::art::Words;
use super::art::{Art, Pack};
use super::fit::{self, Fit};
use super::freestyle::{Choice, Custom, Dialog, Lists};
use super::page::{Button, Page, Press};
use super::paint::{self, Able, Align, Canvas, Inset};
use super::save_page;
use super::setup::{boxed, ground, line};
use crate::files::env::Install;
use crate::files::profile::Profile;
use crate::game::begin::Command;
use crate::net::{self, Service};
use crate::setup::Setup;
use regent_draw::art as service;

/// The fewest players of a LAN game.
const LEAST_PLAYERS: u32 = 2;

/// How often a waiting page is drawn again, to poll.
const POLL_EVERY: Duration = Duration::from_millis(50);
const RADIO_LABEL_INSET: i32 = 24;

/// The `GMTX` texts the lobby shows: its seats' lines.
pub const TEXTS: [usize; 8] = [280, 281, 282, 283, 284, 285, 286, 287];

/// The chat's line as the host changes the settings, `GMTX` 314 "The
/// Host has changed the game options.".
pub const CHANGED_TEXT: usize = 314;

/// The most characters the port box takes: 65535's.
const PORT_DIGITS: usize = 5;

/// The most characters the address box takes: a host name's 253 and a
/// port, or an invite (64) with spaces pasted around it, trimmed where
/// read.
const ADDRESS_CHARS: usize = 260;

/// What the open games list says under its games: this game's.
const JOIN_HINT: &str = "Or type the host's address: HOST or HOST:PORT";
const PORT_HINT: &str = "Host port";

/// The list's line while LAN discovery hears no game, and before why
/// it hears none (placeholders).
const SEARCHING: &str = "Looking for games on the local network...";
const DEAF: &str = "No LAN discovery:";

/// A found game's row: its players' word, `GMTX` 318 "Players", as HD's
/// row `"-%d %s"`; placeholders
/// where HD has no word.
pub const PLAYERS_TEXT: usize = 318;
const PLAYERS: &str = "Players";
const MAP_WORD: &str = "Map";
const FULL: &str = "Full";

/// Where in the open games list's place its games and the boxes go,
/// from its corner, and how tall each line is.
const GAMES_AT: f32 = 8.0;
const GAMES_SHOWN: usize = 8;
const JOIN_LABEL_AT: f32 = 216.0;
const JOIN_BOX_AT: f32 = 240.0;
const PORT_LABEL_AT: f32 = 276.0;
const PORT_BOX_AT: f32 = 300.0;
const SAID_AT: f32 = 330.0;
const ROW: f32 = 24.0;
const INSET: f32 = 40.0;
const PORT_WIDTH: f32 = 120.0;
const NICK_INSET: f32 = 180.0;
const NICK_HINT: &str = "Your name";

/// A box typed in: its ground, and its edge. **Inferred**: the list's
/// own ground is near black in the capture (`ui_mp_list.png`).
const BOX_GROUND: Color32 = Color32::from_rgb(0x10, 0x14, 0x24);
const BOX_EDGE: Color32 = Color32::from_rgb(0x60, 0x58, 0x48);

/// A failure, said in red.
const FAILED: Color32 = Color32::from_rgb(0xdb, 0x21, 0x24);

/// The session dialog's veil over the faces. **Inferred** from
/// `ui_mp_create.png`, whose faces are darker than behind the other
/// pages.
const VEIL: u8 = 160;

/// The lobby's settings box, its headings as `AP73`'s `STRT` names
/// them (**Inferred**: `MX19`'s box is filled by code not read).
const SETTINGS: [(&str, Shows); 11] = [
    ("PLAYER FORCES", Shows::Choice(Choice::Settlement)),
    ("ENEMY FORCES", Shows::Choice(Choice::Scenario)),
    ("WANDERING MONSTERS", Shows::Choice(Choice::Monsters)),
    ("SPECIAL EVENTS", Shows::Events),
    ("LANDSCAPE", Shows::Choice(Choice::Land)),
    ("STARTING GOLD", Shows::Choice(Choice::Gold)),
    ("VICTORY CONDITIONS", Shows::Choice(Choice::Victory)),
    ("VICTORY LIMITS", Shows::Choice(Choice::Limits)),
    ("MAP NUMBER", Shows::MapNumber),
    ("DIFFICULTY", Shows::Difficulty),
    ("CHEATS", Shows::Cheats),
];

/// What a settings line shows.
#[derive(Clone, Copy)]
enum Shows {
    Choice(Choice),
    Events,
    MapNumber,
    Difficulty,
    Cheats,
}

/// The settings box's heading for the save the host loads: a
/// placeholder in `AP73`'s headings' manner.
const LOADED: &str = "LOADED GAME";

/// The words before the lockstep delay a start would have, in the own
/// seat's link cell. openregent's own line, as the original has none.
const DELAY_HEADING: &str = "Network delay";

/// A victory with no limit's line, as the capture shows it.
const NO_LIMIT: &str = "--------";

/// The multiplayer pages' state: the lobby's client, and what the pages
/// show and take typed.
#[derive(bevy::prelude::Resource)]
pub struct Lan {
    protocol: Service,
    /// The lobby hosted or joined.
    client: net::Session,
    /// The host to join, as typed: `HOST[:PORT]`. Kept for the session.
    address: String,
    /// The port to host on, as typed.
    port: String,
    /// The nick being edited on Multiplayer Setup.
    pub(super) nick: String,
    /// The session's name, as typed.
    session: String,
    players: u32,
    /// The game started, to make.
    ready: Option<Command>,
    /// Why the last try stopped.
    said: Option<String>,
    /// The chat line being typed.
    typed: String,
    /// The chat's lines last drawn, and how far its box is scrolled.
    chat_lines: usize,
    chat_scroll: f32,
    /// How far the lobby's settings box is scrolled.
    settings_scroll: f32,
    /// How far Multiplayer Setup's open games list is scrolled, in rows.
    games_scroll: f32,
    /// The host's config as its settings page opened: Cancel puts it
    /// back.
    before: Option<Custom>,
    /// The lobby's setup read back as a config, by its revision.
    read: Option<(u32, Custom)>,
    /// LAN discovery's games, heard while Multiplayer Setup shows; why
    /// not, if its port will not bind.
    finder: Option<Result<net::Finder, String>>,
}

impl Default for Lan {
    fn default() -> Lan {
        Lan {
            protocol: Service::Lan,
            client: net::Session::new(crate::files::log::dir),
            address: String::new(),
            port: net::PORT.to_string(),
            nick: String::new(),
            session: String::new(),
            players: LEAST_PLAYERS,
            ready: None,
            said: None,
            typed: String::new(),
            chat_lines: 0,
            chat_scroll: 0.0,
            settings_scroll: 0.0,
            games_scroll: 0.0,
            before: None,
            read: None,
            finder: None,
        }
    }
}

impl Lan {
    /// Games hosted and joined on `install`: its build is what the
    /// lobby checks.
    pub fn on(&mut self, install: Install) {
        self.client.on(install);
    }

    /// The profile's LAN saves directory, where a LAN save is looked for and
    /// one sent goes.
    pub fn set_saves(&mut self, dir: Option<std::path::PathBuf>) {
        let kept = |at| Arc::new(crate::save::lan::Shelf::new(at)) as Arc<dyn net::Saves>;

        self.client.set_saves(dir.map(kept));
    }

    /// The saves directory [`Lan::set_saves`] set.
    #[cfg(test)]
    pub fn saves(&self) -> Option<&std::path::Path> {
        self.client.saves()
    }

    /// The host's LAN save at `path` loaded: offered to the guests.
    ///
    /// # Errors
    ///
    /// Not the host, or a save that will not load into this lobby
    /// ([`net::Session::choose`]).
    pub fn choose(&mut self, path: &std::path::Path) -> Result<(), String> {
        match self.client.phase() {
            net::Phase::Hosting(_) => self.client.choose(crate::save::lan::offered(path)?),
            _ => Err("only the host loads a game".to_owned()),
        }
    }

    /// Lobby seat `at` moved to the next saved seat: the host's.
    fn reseat(&mut self, at: usize) {
        self.client.reseat(at);
    }

    /// The build hashed ahead, as a page that needs it opens.
    pub fn warm(&mut self) {
        self.client.warm();
    }

    /// LAN discovery heard at `now`: its port bound on the first call.
    fn browse(&mut self, now: Instant) {
        if self.protocol == Service::Lan {
            if let Ok(finder) = self.finder.get_or_insert_with(net::Finder::new) {
                finder.poll(now);
            }
        } else {
            self.unbrowse();
        }
    }

    /// LAN discovery's port let go: Multiplayer Setup left.
    pub fn unbrowse(&mut self) {
        self.finder = None;
    }

    /// The games LAN discovery heard, or why it hears none.
    fn found(&self) -> Result<&[net::Found], &str> {
        match &self.finder {
            Some(Ok(finder)) => Ok(finder.found()),
            Some(Err(why)) => Err(why),
            None => Ok(&[]),
        }
    }

    /// The session dialog opening: named as the player, where nothing is
    /// typed yet (**Inferred**: the original's keeps the last name typed,
    /// mp session name).
    pub fn name(&mut self, nick: &str) {
        if self.session.trim().is_empty() {
            nick.clone_into(&mut self.session);
        }
    }

    /// Host `custom`'s game for the players picked, named `nick`: the
    /// port bound, the guests waited for from here on.
    ///
    /// # Errors
    ///
    /// A port that is not a number from 1 to 65535, or will not bind.
    pub fn host(&mut self, custom: &Custom, nick: String) -> Result<(), String> {
        let typed = self.port.trim();
        let port = match self.protocol {
            Service::Lan => typed
                .parse::<u16>()
                .ok()
                .filter(|&p| p != 0)
                .ok_or_else(|| format!("port {typed}: not a number from 1 to 65535"))?,
            Service::Internet => 0,
        };

        self.bound(custom, nick, port)
    }

    /// [`Lan::host`] on `port`, 0 a port the system picks.
    ///
    /// # Errors
    ///
    /// A port that will not bind.
    fn bound(&mut self, custom: &Custom, nick: String, port: u16) -> Result<(), String> {
        let setup = Setup::hosted(custom, self.players);
        let title = self.session.trim().to_owned();

        self.client
            .host((self.protocol, port), setup, (nick, title))?;
        self.forget();
        self.unbrowse();
        Ok(())
    }

    /// The host typed, as it reads; why not, if it does not.
    ///
    /// # Errors
    ///
    /// [`net::parse_address`]'s.
    fn joinable(&self) -> Result<(String, u16), String> {
        match self.protocol {
            Service::Lan => net::parse_address(&self.address),
            Service::Internet => {
                let invite = self.address.trim();
                net::iroh::Bridge::check_invite(invite)?;
                Ok((invite.to_owned(), 0))
            }
        }
    }

    /// Join the host typed, named `nick`: its name resolved aside, then
    /// asked for a seat from here on.
    pub fn join(&mut self, nick: String) {
        self.forget();
        self.unbrowse();

        if let Err(why) = self.client.join(self.protocol, &self.address, nick) {
            self.said = Some(why);
        }
    }

    /// The last lobby's leftovers gone: its failure, chat, scroll, setup.
    fn forget(&mut self) {
        self.said = None;
        self.typed.clear();
        self.chat_lines = 0;
        self.chat_scroll = 0.0;
        self.settings_scroll = 0.0;
        self.before = None;
        self.read = None;
    }

    /// Back from a LAN game: its lobby again on the game's socket, the
    /// host's showing `custom`'s setup.
    pub fn regroup(&mut self, regroup: net::Regroup, custom: &Custom) {
        let setup = Setup::hosted(custom, regroup.players());
        let title = self.session.trim().to_owned();

        self.forget();
        self.players = regroup.players();
        self.client.regroup(regroup, setup, title);
    }

    /// Host `custom`'s game on a port the system picks, starting once
    /// every guest is ready, and the port: a test's.
    #[cfg(test)]
    pub fn host_any(&mut self, custom: &Custom) -> Result<u16, String> {
        self.client.start_at_once();
        self.bound(custom, String::new(), 0)?;
        self.client
            .bound_port()
            .ok_or_else(|| "not bound".to_owned())
    }

    /// Host `custom`'s game on a port the system picks, named `nick`:
    /// `--menu lobby`'s, for screenshots.
    ///
    /// # Errors
    ///
    /// A socket that will not bind.
    pub fn host_anywhere(&mut self, custom: &Custom, nick: String) -> Result<(), String> {
        self.bound(custom, nick, 0)
    }

    /// Join the host on this machine's `port`, as JOIN GAME does, ready
    /// from the first: a test's.
    #[cfg(test)]
    pub fn join_local(&mut self, port: u16) {
        self.address = format!("127.0.0.1:{port}");
        self.client.start_at_once();
        self.join(String::new());
    }

    /// The lobby given up: its socket let go, the guests told.
    pub fn cancel(&mut self) {
        self.client.cancel();
        self.forget();
    }

    /// The host's settings page opening on `custom`, kept to put back.
    pub fn adjust(&mut self, custom: &Custom) {
        self.before = Some(custom.clone());
    }

    /// The host's settings page cancelled: `custom` as it opened.
    pub fn put_back(&mut self, custom: &mut Custom) {
        if let Some(before) = self.before.take() {
            *custom = before;
        }
    }

    /// The host's settings now `custom`: the setup every guest sees, a
    /// change lapsing their ready.
    pub fn share(&mut self, custom: &Custom) {
        let players = self.players();

        self.before = None;
        self.client.share(Setup::hosted(custom, players));
    }

    /// The host's game started: every guest welcomed, the game to make.
    ///
    /// # Errors
    ///
    /// Not the host, a seat free, a guest not ready, a socket that fails.
    pub fn start(&mut self) -> Result<(), String> {
        let command = command(self.client.start()?)?;

        self.ready = Some(command);
        Ok(())
    }

    /// Whether START GAME would start the game.
    fn startable(&self) -> bool {
        self.client.startable()
    }

    /// A guest's ready flipped: for the host's setup as it stands.
    pub const fn flip_ready(&mut self) {
        self.client.flip_ready();
    }

    /// This player's chat `line`, to the lobby.
    fn say(&mut self, line: &str) {
        self.client.say(line);
    }

    /// `players` picked, as the session dialog picks them: `--menu
    /// lobby:N`'s.
    ///
    /// # Errors
    ///
    /// Not 2 to 4.
    pub fn set_players(&mut self, players: u32) -> Result<(), String> {
        if session::PLAYERS.iter().any(|(_, n)| *n == players) {
            self.players = players;
            Ok(())
        } else {
            Err(format!("{players} players: not 2 to 4"))
        }
    }

    /// The lobby's players: the session's, else those picked.
    pub fn players(&self) -> u32 {
        self.client.seated_players().unwrap_or(self.players)
    }

    /// Whether this machine hosts the lobby.
    const fn hosts(&self) -> bool {
        self.client.hosts()
    }

    /// The lobby as last heard: the host's own, or a guest's of it.
    fn room(&self) -> Option<net::Room> {
        self.client.room()
    }

    fn invite(&self) -> Option<String> {
        self.client.invite()
    }

    fn health(&self, seat: usize) -> Option<net::iroh::Health> {
        self.client.health(seat)
    }

    /// The session's name: the lobby's, else as typed.
    pub(super) fn title(&self) -> String {
        self.client
            .title()
            .unwrap_or_else(|| self.session.trim().to_owned())
    }

    /// The lobby's setup `room` holds, as a config: read once a revision.
    fn read_back(&mut self, room: &net::Room) -> Option<&Custom> {
        let fresh = self.read.as_ref().is_none_or(|(r, _)| *r != room.revision);

        if fresh {
            self.read = Some((room.revision, room.setup.custom()));
        }

        self.read.as_ref().map(|(_, read)| read)
    }

    /// What the lobby is doing, while it has no room to show.
    fn status(&self) -> Option<String> {
        let internet = self.protocol == Service::Internet;

        match self.client.phase() {
            net::Phase::Bound { .. } if internet => Some("Preparing internet lobby...".to_owned()),
            net::Phase::Bound { port } => {
                Some(format!("Hosting on port {port}. Checking this build..."))
            }
            net::Phase::Hosting(_) if internet => {
                let shown = if self.invite().is_some() {
                    "Share the invite code to add players."
                } else {
                    "Preparing internet lobby..."
                };

                Some(shown.to_owned())
            }
            net::Phase::Hosting(hosting) => Some(format!("Hosting on port {}.", hosting.port())),
            net::Phase::Resolving if internet => Some("Connecting to internet host...".to_owned()),
            net::Phase::Resolving => Some(format!("Looking up {}...", self.address.trim())),
            net::Phase::Back => Some("Back to the lobby...".to_owned()),
            net::Phase::Joining(joining) if internet && joining.held() => {
                Some("The host is still playing. Waiting...".to_owned())
            }
            net::Phase::Joining(joining) if joining.held() => Some(format!(
                "{} still plays: waiting for the host.",
                joining.host()
            )),
            net::Phase::Joining(joining) if internet && joining.room().is_none() => {
                Some("Waiting for the internet host...".to_owned())
            }
            net::Phase::Joining(joining) if joining.room().is_none() => {
                Some(format!("Asking {} for a seat...", joining.host()))
            }
            net::Phase::Joining(joining) => joining.failed().map(str::to_owned).or_else(|| {
                joining
                    .receiving()
                    .map(|(came, all)| format!("The host's save: {came} of {all} parts here."))
            }),
            net::Phase::Idle => None,
        }
    }

    /// Why the lobby stopped, if it did: for a page it shows under.
    pub(super) fn stopped(&self) -> Option<&str> {
        self.said.as_deref()
    }

    /// The lobby polled at `now`: whether its game is started, for
    /// [`Lan::take`]. A failure stops it, said on its page.
    pub fn poll(&mut self, now: Instant) -> bool {
        match self.client.poll(now) {
            Some(net::Polled::Started(started)) => match command(*started) {
                Ok(command) => self.ready = Some(command),
                Err(why) => self.said = Some(why),
            },
            Some(net::Polled::Failed(why)) => self.said = Some(why),
            None => {}
        }

        self.ready.is_some()
    }

    /// The game started, once.
    pub const fn take(&mut self) -> Option<Command> {
        self.ready.take()
    }
}

/// The LAN game `started` makes: the host's of its setup, a guest's as
/// its welcome asks.
///
/// # Errors
///
/// A welcome whose setup will not make a game here.
fn command(started: net::Started) -> Result<Command, String> {
    let ((setup, local), peer, saved) = match started {
        net::Started::Hosted { peer, setup, saved } => ((setup, peer.seat()), peer, saved),
        net::Started::Welcomed {
            peer,
            welcome,
            host,
            saved,
        } => {
            let setup = crate::cli::joined(&welcome, &host)?;

            ((setup, welcome.seat), peer, saved)
        }
    };

    Ok(Command {
        setup,
        local,
        warm: None,
        events: Vec::new(),
        offered: None,
        peer: Some(peer),
        saved,
    })
}

/// Whether `page` is one of these, its lobby polled: the host's
/// settings and what they open too.
pub const fn ours(page: Page) -> bool {
    use super::page::Over;

    matches!(
        page,
        Page::Protocol
            | Page::Multiplayer
            | Page::Create
            | Page::Lobby
            | Page::LanSetup
            | Page::Trees(Over::Lan)
            | Page::Presets(_, Over::Lan)
    )
}

/// Whether a box has the keys or a list is open: Escape is theirs.
pub fn busy(ctx: &egui::Context) -> bool {
    egui::Popup::is_any_open(ctx) || ctx.memory(|m| m.focused().is_some())
}

/// `page`'s drawing: `lists` name the setup's picks, `failed` why the
/// last game would not start; what was pressed.
pub fn page(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    page: Page,
    lan: &mut Lan,
    failed: Option<&str>,
    profile: Option<&mut Profile>,
) -> Option<Press> {
    // The lobby, and LAN discovery, are polled each pass, though
    // nothing moves.
    if page == Page::Lobby || page == Page::Multiplayer {
        ctx.request_repaint_after(POLL_EVERY);
    }

    if page == Page::Multiplayer {
        lan.browse(Instant::now());
    }

    match page {
        Page::Protocol => protocol_page(ctx, (art, shared), lan),
        Page::Multiplayer => games_page(ctx, (art, shared), lan, failed, profile),
        Page::Create => session_page(ctx, (art, shared), lan),
        Page::Lobby => lobby_page(ctx, (art, shared), lan, failed),
        _ => None,
    }
}

/// The original protocol page with the two services openregent offers.
fn protocol_page(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    lan: &mut Lan,
) -> Option<Press> {
    let Art { book, ink, .. } = art;
    let (layout, source) = book.protocol.as_ref()?;
    let mut canvas = Canvas::filling(ctx, (ink, shared), paint::size(layout));

    canvas.backdrop();
    canvas.panel_art(*source, layout);

    for c in &layout.controls {
        match c.id {
            Some(protocol::LAN | protocol::INTERNET) => {
                let choice = if c.id == Some(protocol::LAN) {
                    Service::Lan
                } else {
                    Service::Internet
                };
                // Use the middle rows after hiding the unused services.
                let row = if choice == Service::Lan {
                    protocol::SERIAL
                } else {
                    protocol::LAN
                };
                let mut placed = c.clone();
                if let (Some(rect), Some(slot)) = (
                    &mut placed.rect,
                    layout.control(row).and_then(|control| control.rect),
                ) {
                    rect.y = slot.y;
                }
                let tick = if lan.protocol == choice {
                    Tick::Checked
                } else {
                    Tick::Clear
                };
                let label = if choice == Service::Lan {
                    "Play over LAN (UDP)"
                } else {
                    "Play over the Internet (iroh)"
                };

                let icon = ShownControl {
                    text: None,
                    ..placed.clone()
                };

                if canvas.ticked(*source, &icon, (Able::Enabled, tick)) {
                    lan.protocol = choice;
                }
                line(
                    &mut canvas,
                    &radio_label(&placed, label),
                    (label, paint::PALE),
                );
            }
            Some(protocol::MODEM | protocol::SERIAL | protocol::ACCEPT | protocol::CANCEL) => {}
            _ => boxed(&mut canvas, c, c.text.as_deref()),
        }
    }

    let offered = [
        (Button::Accept, Able::Enabled),
        (Button::Cancel, Able::Enabled),
    ];
    let id = |button| {
        if button == Button::Accept {
            protocol::ACCEPT
        } else {
            protocol::CANCEL
        }
    };

    buttons(&mut canvas, layout, (&offered, id)).or_else(|| keyed(ctx, layout, (&offered, id)))
}

/// The keys typed this pass a page's buttons take: while a box has the
/// keys, Enter and Escape alone.
fn keys(ctx: &egui::Context) -> Vec<i32> {
    let typing = ctx.memory(|m| m.focused().is_some());

    paint::keys(ctx)
        .into_iter()
        .filter(|&k| !typing || k == hotkey::ENTER || k == hotkey::ESCAPE)
        .collect()
}

/// Buttons offered, each able or not, and the control each is.
type Offered<'a> = (&'a [(Button, Able)], fn(Button) -> i32);

/// The first of `buttons` whose hot key was typed, each at the control
/// `id` names; those disabled left out.
fn keyed(ctx: &egui::Context, layout: &Layout, (buttons, id): Offered<'_>) -> Option<Press> {
    let enabled: Vec<Button> = buttons
        .iter()
        .filter(|(_, able)| *able == Able::Enabled)
        .map(|(b, _)| *b)
        .collect();

    paint::hot(layout, (&enabled, id), &keys(ctx)).map(Press::Button)
}

/// `buttons` drawn, each at the control `id` names; the one pressed.
fn buttons(canvas: &mut Canvas<'_>, layout: &Layout, (buttons, id): Offered<'_>) -> Option<Press> {
    buttons.iter().fold(None, |pressed, &(button, able)| {
        let now = layout
            .control(id(button))
            .is_some_and(|c| canvas.button(Pack::Expansion, c, able));

        pressed.or(now.then_some(Press::Button(button)))
    })
}

/// `text` typed in layout rectangle `rect`, in `font` and `colour`, at
/// most `most` characters, set by `align`; its response.
fn typed(
    canvas: &mut Canvas<'_>,
    (rect, font, colour): (Rect, Option<FourCc>, Color32),
    (text, most, align): (&mut String, usize, egui::Align),
    salt: &str,
) -> Option<egui::Response> {
    canvas.typed(
        (rect, font.or(Some(menu::SMALL_FONT)), colour),
        (text, most, align),
        (salt, Inset::Sides),
    )
}

/// A box drawn for typing in, in layout rectangle `rect`.
fn typing_box(canvas: &Canvas<'_>, rect: Rect) {
    let at = canvas.fit.rect(rect);
    let painter = canvas.painter();

    painter.rect_filled(at, 0.0, BOX_GROUND);
    painter.rect_stroke(at, 0.0, (1.0, BOX_EDGE), egui::StrokeKind::Inside);
}

/// Control `c` with `text`.
fn with_text(c: &ShownControl, text: &str) -> ShownControl {
    ShownControl {
        text: Some(text.to_owned()),
        ..c.clone()
    }
}

fn radio_label(c: &ShownControl, text: &str) -> ShownControl {
    let rect = c.rect.map(|mut r| {
        r.x += RADIO_LABEL_INSET;
        r.width -= RADIO_LABEL_INSET;
        r
    });

    ShownControl {
        rect,
        art: None,
        font: c.font.or(Some(menu::SMALL_FONT)),
        ..with_text(c, text)
    }
}

fn session_radio_parts(c: &ShownControl) -> (ShownControl, ShownControl) {
    let icon = ShownControl {
        text: None,
        ..c.clone()
    };
    let label = radio_label(c, c.text.as_deref().unwrap_or_default());

    (icon, label)
}

/// The control's id that is `button` on Multiplayer Setup.
const fn games_control(button: Button) -> i32 {
    match button {
        Button::CreateGame => games::CREATE,
        Button::JoinGame => games::JOIN,
        _ => games::CANCEL,
    }
}

/// Multiplayer Setup, `AP33`, in its own art as the setup page: the open
/// games LAN discovery hears, a row each, then the host, port and nick;
/// CREATE GAME, JOIN GAME (enabled while the address
/// reads), CANCEL. A game's row pressed puts its address in the box;
/// double clicked, joins it.
///
/// ```text
///  ┌(pog)──────────── Multiplayer Setup ─────────────────────┐
///  │                  [ OPEN GAMES ]                          │
///  │ Ann's game (Ann)  1/2 Players  Rivers, Map 7  v0.1.0 1a2b^│
///  │ Bob's game (Bob)  2/2 Players  Hills, Map 3          Full#│
///  │ Or type the host's address: HOST or HOST:PORT           #│
///  │ [192.168.1.5:47101                                ]     #│
///  │ Host port                 Your name                      #│
///  │ [47101 ]                  [Ann                  ]        v│
///  │ (CREATE GAME)      (JOIN GAME)        (CANCEL)            │
///  └──────────────────────────────────────────────────────────┘
/// ```
fn games_page(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    lan: &mut Lan,
    failed: Option<&str>,
    mut profile: Option<&mut Profile>,
) -> Option<Press> {
    let Art {
        book, ink, lists, ..
    } = art;
    let layout = book.games.as_ref()?;
    let mut canvas = Canvas::filling(ctx, (ink, shared), paint::size(layout));
    let joinable = lan.joinable();
    let mut doubled = None;

    canvas.backdrop();
    canvas.panel_art(Pack::Expansion, layout);

    for c in &layout.controls {
        match c.id {
            Some(games::LIST) => {
                let said = lan
                    .said
                    .clone()
                    .or_else(|| failed.map(str::to_owned))
                    .map(|s| (s, FAILED));
                let unread = joinable
                    .as_ref()
                    .err()
                    .filter(|_| !lan.address.trim().is_empty())
                    .map(|why| (why.clone(), paint::PALE));
                let note = said.or(unread);

                boxes(
                    &mut canvas,
                    c,
                    lan,
                    note.as_ref().map(|(s, colour)| (s.as_str(), *colour)),
                    profile.as_deref_mut(),
                );
                doubled = found_rows(&mut canvas, c, lan, (&book.words, lists));
            }
            Some(menu::ids::SCROLL_BAR) => {
                let list = layout.control(games::LIST).and_then(|l| l.rect);
                let rows = lan.found().map_or(0, <[net::Found]>::len);

                canvas.list_bar(
                    Pack::Expansion,
                    (c, list),
                    (&mut lan.games_scroll, rows, GAMES_SHOWN),
                    "lan games",
                );
            }
            Some(games::CREATE | games::JOIN | games::CANCEL) => {}
            _ => boxed(&mut canvas, c, c.text.as_deref()),
        }
    }

    let join = if joinable.is_ok() {
        Able::Enabled
    } else {
        Able::Disabled
    };
    let offered = [
        (Button::CreateGame, Able::Enabled),
        (Button::JoinGame, join),
        (Button::Cancel, Able::Enabled),
    ];

    buttons(&mut canvas, layout, (&offered, games_control))
        .or(doubled)
        .or_else(|| keyed(ctx, layout, (&offered, games_control)))
}

/// The games LAN discovery heard, a row each down list `c` from its
/// top, `words` and `lists` naming each one's players and map: the one
/// whose address is typed lit, the build check flush right. A row
/// pressed puts its address in the box; JOIN GAME if one was double
/// clicked. With none, what the search says.
fn found_rows(
    canvas: &mut Canvas<'_>,
    c: &ShownControl,
    lan: &mut Lan,
    (words, lists): (&Words, &Lists),
) -> Option<Press> {
    let list = fit::rect(c.rect?);
    let ours = lan.client.build().ok().flatten();
    let found = lan.found().map(<[net::Found]>::to_vec);
    let colour = paint::pick(c, State::Normal);
    let players = words
        .gmtx(PLAYERS_TEXT)
        .map_or_else(|| PLAYERS.to_owned(), |t| t.trim().to_owned());
    let band = |n: usize| {
        #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
        let top = GAMES_AT + n as f32 * ROW;

        Rect::from_min_size(
            pos2(list.min.x + INSET, list.min.y + top),
            vec2(list.width() - 2.0 * INSET, ROW),
        )
    };
    let shown = |rect: Rect| ShownControl {
        rect: Some(super::setup::whole(rect)),
        font: Some(menu::SMALL_FONT),
        ..ShownControl::default()
    };
    let mut doubled = false;

    let games = match found {
        Ok(games) if games.is_empty() => {
            let message = if lan.protocol == Service::Lan {
                SEARCHING
            } else {
                "Internet games are joined by invite code"
            };
            line(canvas, &shown(band(0)), (message, paint::PALE));
            Vec::new()
        }
        Ok(games) => games,
        Err(why) => {
            line(canvas, &shown(band(0)), (&format!("{DEAF} {why}"), FAILED));
            Vec::new()
        }
    };

    // The rows scrolled to: the rest below, by the bar or the wheel.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a list's rows"
    )]
    let first = (lan.games_scroll.round() as usize).min(games.len().saturating_sub(GAMES_SHOWN));

    for (n, game) in games.iter().skip(first).take(GAMES_SHOWN).enumerate() {
        let at = band(n);
        let address = game.host.to_string();
        let response = canvas.ui.interact(
            canvas.fit.rect(at),
            canvas.ui.id().with(("lan game", game.open.id)),
            egui::Sense::click(),
        );

        if response.clicked() {
            address.clone_into(&mut lan.address);
        }

        doubled |= response.double_clicked();

        if lan.address.trim() == address {
            canvas
                .painter()
                .rect_filled(canvas.fit.rect(at), 0.0, save_page::LIT);
        }

        // Why it may not be joined in red, else whether full, else its
        // build.
        let (state, tint) = match (ours.as_ref().and_then(|b| game.misfit(b)), game.full()) {
            (Some(why), _) => (why, FAILED),
            (None, true) => (FULL.to_owned(), paint::PALE),
            (None, false) => (game.open.build.shown(), paint::PALE),
        };
        let open = &game.open;
        let map = map_text(open, lists.dialog(open.players));
        let text = format!(
            "{}  {}/{} {players}  {map}",
            named(open, &address),
            open.seated,
            open.players
        );
        let wide = canvas
            .ink
            .font(menu::SMALL_FONT)
            .map_or(0.0, |f| f.width(canvas.ui.ctx(), canvas.fit.scale, &state));
        let room = at.width() - save_page::SIDE_MARGIN * 2.0 - wide;
        let left = Rect::from_min_size(at.min, vec2(room.max(0.0), ROW));

        line(canvas, &shown(left), (&text, colour));
        save_page::flush_right(
            canvas,
            (&shown(at), super::setup::whole(at)),
            (&state, tint),
            at.max.x - save_page::SIDE_MARGIN,
        );
    }

    doubled.then_some(Press::Button(Button::JoinGame))
}

/// A found game's name: its session's and its host's nick, those it has;
/// with neither (the command line's host), its `address`.
///
/// ```text
///  Ann's game (Ann)    Ann's game    (Ann)    192.168.1.5:47101
/// ```
fn named(open: &net::Open, address: &str) -> String {
    match (open.title.trim(), open.nick.trim()) {
        ("", "") => address.to_owned(),
        (title, "") => title.to_owned(),
        (title, nick) => format!("{title} ({nick})").trim().to_owned(),
    }
}

/// What a found game's row says of its map: the save it loads, or its
/// landscape and Map Number as the lobby's settings name them.
fn map_text(open: &net::Open, dialog: Dialog<'_>) -> String {
    if let Some(name) = &open.save {
        format!("{LOADED}: {name}")
    } else {
        let custom = open.setup.custom();
        let land = Choice::Land.shown(&custom, dialog);

        format!("{}, {MAP_WORD} {}", bare(&land), custom.map_number)
    }
}

/// In list `c`'s place: the address and port boxes, each under its
/// line, and `said` under them.
fn boxes(
    canvas: &mut Canvas<'_>,
    c: &ShownControl,
    lan: &mut Lan,
    said: Option<(&str, Color32)>,
    profile: Option<&mut Profile>,
) {
    let Some(r) = c.rect else { return };
    let list = fit::rect(r);
    let font = Some(menu::SMALL_FONT);
    let row = |top: f32, width: f32| {
        Rect::from_min_size(pos2(list.min.x + INSET, list.min.y + top), vec2(width, ROW))
    };
    let wide = list.width() - 2.0 * INSET;
    let text_line = |canvas: &mut Canvas<'_>, top: f32, (text, colour): (&str, Color32)| {
        let shown = ShownControl {
            rect: Some(super::setup::whole(row(top, wide))),
            font,
            ..ShownControl::default()
        };

        line(canvas, &shown, (text, colour));
    };

    canvas
        .painter()
        .rect_filled(canvas.fit.rect(list), 0.0, Color32::BLACK);

    let hint = if lan.protocol == Service::Lan {
        JOIN_HINT
    } else {
        "Paste the host's internet invite code"
    };
    text_line(canvas, JOIN_LABEL_AT, (hint, paint::PALE));
    typing_box(canvas, row(JOIN_BOX_AT, wide));
    typed(
        canvas,
        (row(JOIN_BOX_AT, wide), font, Color32::WHITE),
        (&mut lan.address, ADDRESS_CHARS, egui::Align::Min),
        "multiplayer address",
    );

    if lan.protocol == Service::Lan {
        text_line(canvas, PORT_LABEL_AT, (PORT_HINT, paint::PALE));
        typing_box(canvas, row(PORT_BOX_AT, PORT_WIDTH));

        let port = typed(
            canvas,
            (row(PORT_BOX_AT, PORT_WIDTH), font, Color32::WHITE),
            (&mut lan.port, PORT_DIGITS, egui::Align::Min),
            "lan port",
        );

        if port.is_some_and(|r| r.changed()) {
            lan.port.retain(|ch| ch.is_ascii_digit());
        }
    }

    // The open space beside the port holds the local player's nick.
    let nick_rect = Rect::from_min_size(
        pos2(list.min.x + NICK_INSET, list.min.y + PORT_BOX_AT),
        vec2(wide - NICK_INSET + INSET, ROW),
    );
    let nick_label = ShownControl {
        rect: Some(super::setup::whole(Rect::from_min_size(
            pos2(list.min.x + NICK_INSET, list.min.y + PORT_LABEL_AT),
            nick_rect.size(),
        ))),
        font,
        ..ShownControl::default()
    };
    line(canvas, &nick_label, (NICK_HINT, paint::PALE));
    typing_box(canvas, nick_rect);
    // Written once editing ends (Enter, or a press elsewhere, which
    // comes before any button's), not at every key.
    let edited = typed(
        canvas,
        (nick_rect, font, Color32::WHITE),
        (&mut lan.nick, net::NICK_MOST, egui::Align::Min),
        "multiplayer nick",
    )
    .is_some_and(|r| r.lost_focus());

    if edited {
        let saved = profile
            .ok_or_else(|| "no profile is open".to_owned())
            .and_then(|profile| profile.save_nick(&lan.nick));

        if let Err(why) = saved {
            lan.said = Some(why);
        }
    }

    if let Some(said) = said {
        text_line(canvas, SAID_AT, said);
    }
}

/// The control's id that is `button` on the session dialog.
const fn session_control(button: Button) -> i32 {
    match button {
        Button::Accept => session::ACCEPT,
        _ => session::CANCEL,
    }
}

/// The session dialog, `AP55`, centred over the faces, veiled: the
/// session's name typed, 2, 3 or 4 players, ACCEPT (with a name) and
/// CANCEL. The original's GAME VISIBILITY, HD's own, means nothing on a
/// LAN: not drawn.
fn session_page(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    lan: &mut Lan,
) -> Option<Press> {
    let Art { book, ink, .. } = art;
    let layout = book.session.as_ref()?;
    let mut canvas = Canvas::new(ctx, (ink, shared), paint::SCREEN);
    let window = canvas.ui.max_rect();

    canvas.backdrop();
    canvas
        .ui
        .painter()
        .rect_filled(window, 0.0, Color32::from_black_alpha(VEIL));
    canvas.fit = Fit::centred(window, canvas.fit.scale, paint::size(layout));
    canvas.block(window);
    canvas.panel_art(Pack::Expansion, layout);

    for c in &layout.controls {
        let players = session::PLAYERS.iter().find(|(id, _)| c.id == Some(*id));

        match (c.id, players) {
            (_, Some(&(_, n))) => {
                let tick = if lan.players == n {
                    Tick::Checked
                } else {
                    Tick::Clear
                };
                let (icon, label) = session_radio_parts(c);

                if canvas.ticked(Pack::Expansion, &icon, (Able::Enabled, tick)) {
                    lan.players = n;
                }
                if let Some(text) = label.text.as_deref() {
                    line(&mut canvas, &label, (text, Color32::WHITE));
                }
            }
            (Some(session::NAME), _) => {
                ground(&mut canvas, c);

                let colour = paint::pick(c, State::Normal);
                let shown = c.rect.map(fit::rect).and_then(|rect| {
                    typed(
                        &mut canvas,
                        (rect, c.font, colour),
                        (&mut lan.session, session::LENGTH, egui::Align::Center),
                        "lan session",
                    )
                });

                if let Some(response) = shown.filter(|r| !r.has_focus()) {
                    response.request_focus();
                }
            }
            (Some(session::ACCEPT | session::CANCEL), _) => {}
            _ => boxed(&mut canvas, c, c.text.as_deref()),
        }
    }

    let named = if lan.session.trim().is_empty() {
        Able::Disabled
    } else {
        Able::Enabled
    };
    let offered = [(Button::Accept, named), (Button::Cancel, Able::Enabled)];

    buttons(&mut canvas, layout, (&offered, session_control))
        .or_else(|| keyed(ctx, layout, (&offered, session_control)))
}

/// The control's id that is `button` on the lobby.
const fn lobby_control(button: Button) -> i32 {
    match button {
        Button::StartGame => lobby::START,
        Button::Ready => lobby::READY,
        Button::AdjustSettings => lobby::ADJUST,
        _ => lobby::CANCEL,
    }
}

/// A chat's controls on a page.
pub(super) struct Chat {
    pub heading: i32,
    pub lines: i32,
    pub bar: i32,
    pub hint: i32,
    pub entry: i32,
    pub send: i32,
}

/// The lobby's chat.
const LOBBY_CHAT: Chat = Chat {
    heading: lobby::CHAT_HEADING,
    lines: lobby::CHAT,
    bar: menu::ids::SCROLL_BAR,
    hint: lobby::CHAT_HINT,
    entry: lobby::CHAT_ENTRY,
    send: lobby::SEND_CHAT,
};

/// The setup page's chat, a LAN game's.
pub(super) const SETUP_CHAT: Chat = Chat {
    heading: menu::setup::CHAT_HEADING,
    lines: menu::setup::CHAT_BOX,
    bar: menu::ids::SCROLL_BAR,
    hint: menu::setup::CHAT_HINT,
    entry: menu::setup::CHAT_ENTRY,
    send: menu::setup::SEND_CHAT,
};

/// The lobby, `MX19`, in its own art as the setup page: the session's
/// name; its settings, scrolling; each seat's status, nick and build;
/// the chat; ADJUST SETTINGS and LOAD GAME (the host's: a LAN save),
/// START GAME (the host's, every seat taken, every guest ready, and with
/// a save every guest holding it) or Ready (a guest's), CANCEL.
///
/// ```text
///  ┌(pog)──────────────────── foobar ─────────────────────────┐
///  │ [SETTINGS ]  ┌[x] PLAYER ONE - HOST ┐   [ CHAT ROOM ]     │
///  │ PLAYER FORCES│ player1 (v0.1.0 1a2b3)│  │ player1: hi  │^│ │
///  │ Random (...) ├[ ] PLAYER TWO - NOT ..┤  │              │#│ │
///  │ ...         ^│                       │  Press ENTER to ... │
///  │ (ADJUST SET.)├───────────────────────┤  [typed        ]    │
///  │ (LOAD GAME ) │                       │   (SEND CHAT)       │
///  │  GAME NAME   └───────────────────────┘                     │
///  │ (START GAME)                (CANCEL)                       │
///  └────────────────────────────────────────────────────────────┘
/// ```
fn lobby_page(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    lan: &mut Lan,
    failed: Option<&str>,
) -> Option<Press> {
    let Art {
        book, ink, lists, ..
    } = art;
    let layout = book.lobby.as_ref()?;
    let mut canvas = Canvas::filling(ctx, (ink, shared), paint::size(layout));
    let room = lan.room();
    let hosts = lan.hosts();
    let title = lan.title();
    let dialog = lists.dialog(lan.players());
    let loaded = room
        .as_ref()
        .and_then(|r| r.save.as_ref())
        .map(|o| o.name.clone());
    let settings = room
        .as_ref()
        .and_then(|r| lan.read_back(r).cloned())
        .map(|custom| settings_text(&custom, dialog))
        .map(|text| match &loaded {
            Some(name) => format!("{LOADED}\n{name}\n\n{text}"),
            None => text,
        });
    // This machine's seat: a guest's from the host, the host's is 0.
    let own = room
        .as_ref()
        .map(|r| if hosts { 0 } else { r.seat as usize });
    let delay = room.as_ref().map_or(0, |r| r.delay);
    let words = &book.words;

    let mut load = false;

    canvas.backdrop();
    canvas.panel_art(Pack::Expansion, layout);

    for c in &layout.controls {
        let id = c.id.unwrap_or_default();
        let seat = lobby::STATUS
            .iter()
            .chain(&lobby::NAMES)
            .chain(&lobby::BORDERS)
            .chain(&lobby::PLATES)
            .chain(&lobby::PICKS)
            .position(|s| *s == id)
            .map(|n| n % lobby::STATUS.len());

        match id {
            lobby::TITLE | lobby::GAME_NAME => boxed(&mut canvas, c, Some(&title)),
            lobby::SETTINGS => {
                ground(&mut canvas, c);

                let bar = layout.control(lobby::SETTINGS_BAR);
                let text = settings.clone().unwrap_or_default();

                canvas.text_box(
                    (Pack::Expansion, c, bar),
                    (&text, Align::Centre, 1.0),
                    &mut lan.settings_scroll,
                );
            }
            _ if lobby::STATUS.contains(&id) || lobby::NAMES.contains(&id) => {
                let pressed = seat_line(&mut canvas, c, (seat, room.as_ref()), words);

                if let (true, true, Some(seat)) = (hosts, pressed, seat) {
                    lan.reseat(seat);
                }
            }
            _ if lobby::BORDERS.contains(&id) || lobby::PLATES.contains(&id) => {
                boxed(&mut canvas, c, None);
            }
            _ if lobby::PICKS.contains(&id) => {
                link_cell(&mut canvas, c, (seat, own, delay), lan);
            }
            lobby::SETTINGS_STRIP
            | lobby::SETTINGS_HEADING
            | lobby::GAME_NAME_HEADING
            | lobby::POG => boxed(&mut canvas, c, c.text.as_deref()),
            lobby::LOAD => {
                let able = if hosts { Able::Enabled } else { Able::Disabled };

                if canvas.button(Pack::Expansion, c, able) {
                    load = true;
                }
            }
            _ => {}
        }
    }

    copy_invite(ctx, &mut canvas, layout, lan);

    let notes = lan
        .status()
        .into_iter()
        .chain(lan.said.clone())
        .chain(failed.map(str::to_owned))
        .collect::<Vec<_>>();

    chat(&mut canvas, layout, &LOBBY_CHAT, (lan, &notes), words);

    let able = |yes: bool| if yes { Able::Enabled } else { Able::Disabled };
    let joined = room.is_some();
    let first = if hosts {
        (Button::StartGame, able(lan.startable()))
    } else {
        (Button::Ready, able(joined))
    };
    let offered = [
        first,
        (Button::AdjustSettings, able(hosts && joined)),
        (Button::Cancel, Able::Enabled),
    ];

    let pressed = buttons(&mut canvas, layout, (&offered, lobby_control))
        .or_else(|| keyed(ctx, layout, (&offered, lobby_control)));

    pressed.or(load.then_some(Press::Button(Button::LoadGame)))
}

/// The invite's font: the map's line, smaller than the page's.
const INVITE_FONT: FourCc = FourCc(*b"fnt7");

/// The invite's line height, layout pixels.
const INVITE_ROW: f32 = 14.0;

/// Copy the host's iroh ID from the lobby, and show it under the button.
fn copy_invite(ctx: &egui::Context, canvas: &mut Canvas<'_>, layout: &Layout, lan: &Lan) {
    if let Some(invite) = lan.invite() {
        let size = paint::size(layout);
        let rect = Rect::from_min_size(pos2(size.x - 190.0, 30.0), vec2(165.0, ROW));
        let shown = ShownControl {
            rect: Some(super::setup::whole(rect)),
            font: Some(menu::SMALL_FONT),
            ..ShownControl::default()
        };

        line(canvas, &shown, ("[COPY INVITE]", paint::PALE));
        if pressed_on(canvas, &shown) {
            ctx.copy_text(invite.clone());
        }

        // The invite itself under it, to read and type without the
        // clipboard: one line across the title panel, the smaller font
        // shrunk to the panel's width.
        let title = layout.control(lobby::TITLE).and_then(|c| c.rect);
        let (left, right) = title
            .map(fit::rect)
            .map_or((rect.min.x, rect.max.x), |t| (t.min.x, t.max.x));
        let below =
            Rect::from_min_max(pos2(left, rect.max.y), pos2(right, rect.max.y + INVITE_ROW));
        let text = ShownControl {
            rect: Some(super::setup::whole(below)),
            font: Some(INVITE_FONT),
            ..ShownControl::default()
        };

        canvas.fitted(&text, &invite, paint::PALE);
    }
}

/// Seat `seat`'s status box or name box `c`: its line in `room`, ticked
/// when ready; the name box, its nick and build, and with a save loaded
/// the saved seat it sits in. A seat past the players shows nothing but
/// its frame. Whether its line was pressed: the host's reseating.
fn seat_line(
    canvas: &mut Canvas<'_>,
    c: &ShownControl,
    (seat, room): (Option<usize>, Option<&net::Room>),
    words: &Words,
) -> bool {
    let players = room.map_or(0, |r| r.players as usize);
    let taken = seat.zip(room).and_then(|(s, r)| r.seats.get(s));
    let sits = seat
        .zip(room.and_then(|r| r.save.as_ref()))
        .and_then(|(s, offer)| offer.seats.get(*offer.sits.get(s)? as usize));
    let id = c.id.unwrap_or_default();

    match seat.filter(|s| *s < players) {
        Some(seat) if lobby::STATUS.contains(&id) => {
            let ready = taken.is_some_and(|t| t.ready);
            let text = words
                .gmtx(lobby::status_text(seat, ready))
                .unwrap_or_default();
            let tick = if ready { Tick::Checked } else { Tick::Clear };

            canvas.ticked(
                Pack::Expansion,
                &with_text(c, text.trim()),
                (Able::Enabled, tick),
            );
            false
        }
        Some(_) => {
            ground(canvas, c);

            if let Some(t) = taken {
                let named = if t.nick.is_empty() {
                    format!("({})", t.build)
                } else {
                    format!("{} ({})", t.nick, t.build)
                };
                let shown = match sits {
                    Some(saved) => format!("{named} {AS_SAVED} {saved}"),
                    None => named,
                };
                line(canvas, c, (&shown, paint::pick(c, State::Normal)));
            }

            sits.is_some() && pressed_on(canvas, c)
        }
        None if lobby::NAMES.contains(&id) => {
            ground(canvas, c);
            false
        }
        None => false,
    }
}

/// The unused combobox row under a seat's name shows its host link.
fn health_line(health: net::iroh::Health) -> (String, Color32) {
    let route = match health.route {
        Some(net::iroh::Route::Direct) => " · Direct",
        Some(net::iroh::Route::Relayed) => " · Relayed",
        None => "",
    };

    if !health.connected {
        ("Disconnected".to_owned(), FAILED)
    } else if health.unstable {
        (format!("Unstable · no ping{route}"), Color32::YELLOW)
    } else if let Some(ping) = health.ping_ms {
        (format!("Connected · {ping} ms{route}"), paint::PALE)
    } else {
        (format!("Checking ping{route}"), Color32::YELLOW)
    }
}

/// How a seat's line names the saved seat it sits in: a placeholder.
const AS_SAVED: &str = "as";

/// Whether `c`'s rectangle was clicked this pass.
fn pressed_on(canvas: &mut Canvas<'_>, c: &ShownControl) -> bool {
    c.rect.is_some_and(|r| {
        let at = canvas.fit.rect(fit::rect(r));

        canvas
            .ui
            .interact(
                at,
                canvas.ui.id().with(("seat", c.id)),
                egui::Sense::click(),
            )
            .clicked()
    })
}

/// A newly hosted session's cheat rule: barred, where the original
/// allows cheats in every new game. The host may tick ALLOW CHEATS.
pub const NEW_CHEATS: Cheats = Cheats::Barred;

/// The lobby's settings box: a heading, the pick under it, a line
/// between each (**Game**: `ui_mp_game_lobby.png`).
fn settings_text(custom: &Custom, dialog: Dialog<'_>) -> String {
    let shown = |shows: Shows| match shows {
        Shows::Choice(Choice::Limits) if !Choice::Limits.offered(custom, dialog) => {
            NO_LIMIT.to_owned()
        }
        Shows::Choice(choice) => bare(&choice.shown(custom, dialog)).to_owned(),
        Shows::Events => [Choice::Event(0), Choice::Event(1)]
            .map(|c| bare(&c.shown(custom, dialog)).to_owned())
            .join(", "),
        Shows::MapNumber => custom.map_number.to_string(),
        Shows::Difficulty => custom
            .difficulty(dialog.lists)
            .unwrap_or_default()
            .to_owned(),
        Shows::Cheats => if custom.cheats == Cheats::Allowed {
            "Yes"
        } else {
            "No"
        }
        .to_owned(),
    };

    SETTINGS
        .iter()
        .map(|&(heading, shows)| format!("{heading}\n{}", shown(shows)))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Seat `seat`'s link cell `c`: its host link, or on the `own` seat the
/// lockstep `delay` (none: no link to oneself).
fn link_cell(
    canvas: &mut Canvas<'_>,
    c: &ShownControl,
    (seat, own, delay): (Option<usize>, Option<usize>, u32),
    lan: &Lan,
) {
    if seat.is_some() && seat == own {
        if delay > 0 {
            line(canvas, c, (&delay_line(delay), paint::PALE));
        }
    } else if let Some(health) = seat.and_then(|seat| lan.health(seat)) {
        let (text, colour) = health_line(health);

        line(canvas, c, (&text, colour));
    }
}

/// The lockstep `delay` as the own seat's link cell says it: "Network
/// delay 3 frames, 150 ms".
fn delay_line(delay: u32) -> String {
    format!("{DELAY_HEADING} {}", net::Delay::shown(delay))
}

/// `name` without the mark a greyed or shipped entry carries.
fn bare(name: &str) -> &str {
    name.strip_prefix(regent_core::sim::world::offer::UNAVAILABLE_MARK)
        .or_else(|| name.strip_prefix(crate::files::trees::SHIPPED_MARK))
        .unwrap_or(name)
}

/// The chat `ids` names on `layout`: its heading and hint, its lines
/// after `notes` in its box, kept at the newest, the line typed in its
/// entry, sent by Enter or SEND CHAT; `words` say a settings change.
pub(super) fn chat(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    ids: &Chat,
    (lan, notes): (&mut Lan, &[String]),
    words: &Words,
) {
    let control = |id| layout.control(id);
    let lines = lan.room().map(|r| r.chat.lines).unwrap_or_default();
    let text = notes
        .iter()
        .cloned()
        .chain(lines.iter().map(|talk| talk_line(talk, words)))
        .collect::<Vec<_>>()
        .join("\n");
    let mut sent = false;

    // A line come: the box at its newest.
    if lines.len() + notes.len() != lan.chat_lines {
        lan.chat_lines = lines.len() + notes.len();
        lan.chat_scroll = f32::MAX;
    }

    for id in [ids.heading, ids.hint] {
        if let Some(c) = control(id) {
            let font = c.font.or(Some(menu::SMALL_FONT));

            boxed(
                canvas,
                &ShownControl { font, ..c.clone() },
                c.text.as_deref(),
            );
        }
    }

    if let Some(c) = control(ids.lines) {
        ground(canvas, c);
        canvas.text_box(
            (Pack::Expansion, c, control(ids.bar)),
            (&text, Align::Left, 1.0),
            &mut lan.chat_scroll,
        );
    }

    if let Some(c) = control(ids.entry) {
        ground(canvas, c);

        let colour = paint::pick(c, State::Normal);
        let response = c.rect.map(fit::rect).and_then(|rect| {
            typed(
                canvas,
                (rect, c.font, colour),
                (&mut lan.typed, net::CHAT_MOST, egui::Align::Min),
                "lan chat",
            )
        });
        let entered = canvas.ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));

        if let Some(response) = response.filter(|r| r.lost_focus() && entered) {
            sent = true;
            response.request_focus();
        }
    }

    if let Some(c) = control(ids.send) {
        sent |= canvas.button(Pack::Expansion, c, Able::Enabled);
    }

    if sent {
        let line = std::mem::take(&mut lan.typed);

        lan.say(&line);
    }
}

/// A chat line as the box shows it: `nick: line`; the settings
/// changed, [`CHANGED_TEXT`].
fn talk_line(talk: &net::Talk, words: &Words) -> String {
    match talk {
        net::Talk::Said(who, line) => format!("{who}: {line}"),
        net::Talk::Changed => words
            .gmtx(CHANGED_TEXT)
            .map(|t| t.trim().to_owned())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_label_clears_radio_art() {
        let radio = ShownControl {
            rect: Some(regent_core::data::smnu::Rect {
                x: 76,
                y: 189,
                width: 87,
                height: 30,
            }),
            art: Some((FourCc(*b"INDk"), 1003)),
            text: Some("2 Players".to_owned()),
            ..ShownControl::default()
        };

        let (icon, label) = session_radio_parts(&radio);

        assert!(icon.text.is_none());
        assert_eq!(label.rect.unwrap().x, 100);
        assert!(label.art.is_none());
    }

    #[test]
    fn protocol_label_starts_beside_radio() {
        let radio = ShownControl {
            rect: Some(regent_core::data::smnu::Rect {
                x: 214,
                y: 241,
                width: 298,
                height: 23,
            }),
            ..ShownControl::default()
        };

        let label = radio_label(&radio, "Play over LAN (UDP)");

        assert_eq!(label.rect.unwrap().x, 238);
        assert_eq!(label.text.as_deref(), Some("Play over LAN (UDP)"));
        assert!(label.art.is_none());
    }

    /// How long a lobby's aside work may take to fail: generous, as a
    /// loaded machine may not run its thread for a while.
    const SETTLE: Duration = Duration::from_secs(30);

    /// Whether `lan` stops waiting, no game made, within [`SETTLE`]:
    /// polled each millisecond, so true as soon as it does.
    fn settles(lan: &mut Lan) -> bool {
        let deadline = Instant::now() + SETTLE;

        loop {
            std::thread::sleep(Duration::from_millis(1));
            let stopped =
                !lan.poll(Instant::now()) && matches!(lan.client.phase(), net::Phase::Idle);

            if stopped || Instant::now() >= deadline {
                break stopped;
            }
        }
    }

    /// A port that is not a number is said, no lobby begun.
    #[test]
    fn a_bad_port_is_said_and_hosts_nothing() {
        let mut lan = Lan {
            port: "nope".to_owned(),
            ..Lan::default()
        };

        assert!(
            lan.host(&Custom::default(), String::new())
                .is_err_and(|why| why.contains("nope"))
        );
        assert!(matches!(lan.client.phase(), net::Phase::Idle));
    }

    /// Port 0 is refused, as the message says: 1 to 65535.
    #[test]
    fn port_zero_is_refused() {
        let mut lan = Lan {
            port: "0".to_owned(),
            ..Lan::default()
        };

        assert!(
            lan.host(&Custom::default(), String::new())
                .is_err_and(|why| why.contains("1 to 65535"))
        );
        assert!(matches!(lan.client.phase(), net::Phase::Idle));
    }

    /// An invite pasted with spaces around it fits the box, and joins.
    #[test]
    fn a_padded_invite_fits_and_reads() {
        let invite = format!(" {} ", ::iroh::SecretKey::from_bytes(&[1; 32]).public());
        let lan = Lan {
            protocol: Service::Internet,
            address: invite.clone(),
            ..Lan::default()
        };

        assert!(invite.chars().count() <= ADDRESS_CHARS);
        assert!(lan.joinable().is_ok());
    }

    /// JOIN GAME is offered only for an address that reads.
    #[test]
    fn only_an_address_that_reads_is_joinable() {
        let joinable = |address: &str| {
            Lan {
                address: address.to_owned(),
                ..Lan::default()
            }
            .joinable()
        };

        assert_eq!(joinable("10.0.0.2"), Ok(("10.0.0.2".to_owned(), net::PORT)));
        assert_eq!(joinable("pc:4000"), Ok(("pc".to_owned(), 4000)));
        assert!(joinable("").is_err() && joinable("pc:x").is_err());
    }

    /// A name that does not resolve fails aside, and the failure is
    /// said; no game.
    #[test]
    fn an_unknown_host_is_said_in_the_lobby() {
        let mut lan = Lan {
            address: "no such host.invalid".to_owned(),
            ..Lan::default()
        };

        // A build at hand: with none, its own failure ("no install to
        // check") could come before the address's.
        lan.client.built(net::Build {
            version: "test".to_owned(),
            profile: net::Profile::this(),
            commit: "test".to_owned(),
            data: 2,
        });
        lan.join(String::new());
        lan.poll(Instant::now());

        let stopped = settles(&mut lan);

        assert!(stopped);
        assert!(
            lan.said.as_deref().is_some_and(|s| s.contains("no spaces")),
            "{:?}",
            lan.said
        );
        assert!(lan.take().is_none());
    }

    /// The host's port bound while the build hashes, which fails with
    /// no install: said, the port let go.
    #[test]
    fn a_host_with_no_build_says_why_and_lets_the_port_go() {
        let mut lan = Lan {
            session: "foobar".to_owned(),
            ..Lan::default()
        };

        lan.host_anywhere(&Custom::default(), "Ann".to_owned())
            .expect("bound");
        assert!(
            lan.status()
                .is_some_and(|s| s.starts_with("Hosting on port"))
        );
        assert_eq!(lan.title(), "foobar");

        let stopped = settles(&mut lan);

        assert!(stopped);
        assert_eq!(lan.said.as_deref(), Some("no install to check"));
    }

    /// Only a host with every seat taken and every guest ready starts;
    /// a guest never does.
    #[test]
    fn start_is_refused_but_to_a_ready_host() {
        let mut lan = Lan::default();

        assert!(lan.start().is_err_and(|why| why.contains("host")));
        assert!(!lan.startable());
    }

    /// The host's settings page cancelled puts the config back; accepted,
    /// keeps it.
    #[test]
    fn cancelled_settings_are_put_back() {
        let mut lan = Lan::default();
        let mut custom = Custom::default();

        lan.adjust(&custom);
        custom.gold = 10_000;
        lan.put_back(&mut custom);
        assert_eq!(custom, Custom::default());

        lan.adjust(&custom);
        custom.gold = 10_000;
        lan.share(&custom);
        lan.put_back(&mut custom);
        assert_eq!(custom.gold, 10_000);
    }

    /// The chat says a settings change in the game's words, `GMTX` 314;
    /// a player's line as `nick: line`. A found game's row says `GMTX` 318.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_settings_change_is_said_in_the_games_words() {
        let text = regent_core::data::text::Text::build(crate::testdata::hd());
        let said = net::Talk::Said("Ann".to_owned(), "hi".to_owned());

        assert_eq!(
            text.gmtx(CHANGED_TEXT).map(|t| t.trim().to_owned()),
            Some("The Host has changed the game options.".to_owned())
        );
        assert_eq!(talk_line(&said, &Words::default()), "Ann: hi");
        assert_eq!(
            text.gmtx(PLAYERS_TEXT).map(|t| t.trim().to_owned()),
            Some(PLAYERS.to_owned())
        );
    }

    /// The settings box lists each heading with its pick under it.
    #[test]
    fn the_settings_box_lists_each_heading_and_pick() {
        let text = settings_text(
            &Custom {
                map_number: 4242,
                ..Custom::default()
            },
            super::super::freestyle::Lists::default().dialog(LEAST_PLAYERS),
        );
        let lines: Vec<&str> = text.lines().collect();

        assert_eq!(lines[0], "PLAYER FORCES");
        assert!(text.contains("MAP NUMBER\n4242"));
        assert!(text.contains("VICTORY LIMITS\n--------"));
        assert!(text.ends_with("CHEATS\nYes"));

        let barred = settings_text(
            &Custom {
                cheats: Cheats::Barred,
                ..Custom::default()
            },
            super::super::freestyle::Lists::default().dialog(LEAST_PLAYERS),
        );
        assert!(barred.ends_with("CHEATS\nNo"));
    }

    /// A found game's row names it by session and nick, else by its
    /// address, and its map by Map Number, or the save it loads.
    #[test]
    fn a_found_game_is_named_by_session_nick_or_address() {
        let open = |title: &str, nick: &str, save: Option<&str>| net::Open {
            id: 1,
            port: net::PORT,
            title: title.to_owned(),
            nick: nick.to_owned(),
            seated: 1,
            players: 2,
            setup: Setup {
                map_seed: Some(7),
                ..Setup::default()
            },
            save: save.map(str::to_owned),
            build: net::Build {
                version: "0.1.0".to_owned(),
                profile: net::Profile::this(),
                commit: "c0ffee".to_owned(),
                data: 2,
            },
            builds: net::Builds::Same,
        };
        let at = "10.0.0.2:47101";
        let lists = super::super::freestyle::Lists::default();

        assert_eq!(
            named(&open("Ann's game", "Ann", None), at),
            "Ann's game (Ann)"
        );
        assert_eq!(named(&open("Ann's game", "", None), at), "Ann's game");
        assert_eq!(named(&open("", "Ann", None), at), "(Ann)");
        assert_eq!(named(&open("", "", None), at), at);

        let map = map_text(&open("", "", None), lists.dialog(2));
        let saved = map_text(&open("", "", Some("Siege")), lists.dialog(2));

        assert!(map.ends_with(", Map 7"), "{map}");
        assert_eq!(saved, "LOADED GAME: Siege");
    }
}
