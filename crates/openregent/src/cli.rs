//! The command line: the options that make a game and how it runs,
//! and the setup a recording keeps of them.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use bevy::log::info;
use clap::parser::ValueSource;
use clap::{CommandFactory, Parser};
use regent_core::data::build_tree::Tree;
use regent_core::data::fourcc::FourCc;
use regent_core::data::guid::Guid;
use regent_core::data::text::Text;
use regent_core::sim::freestyle::{self, List};
use regent_core::sim::mapgen::custom;
use regent_core::sim::world::CROWN;

use crate::files::env::Install;
use crate::quests;
use crate::setup::{self, Setup, Winning};
use crate::{
    draw::shot, files::config, game::begin, game::record, map::fog, menu, net, panel, ui::chat,
};

/// [`setup::SEED`], as `--help` shows it.
const DAY_SEED: &str = "0x0b609e2c";

/// The quiet day's length in frames: the day1 log's end.
const DAY_FRAMES: u32 = 1254;

/// The most players a Player Forces entry is for
/// ([`regent_core::data::rgs::map::UnitPattern::players`]).
pub(crate) const MAX_PLAYERS: i64 = 4;

/// A code option given with no value: list the valid codes.
const LIST: &str = "";

/// The Wandering Monsters pick for none, which the dialog offers beside
/// the quest's entries.
const NO_MONSTERS: FourCc = FourCc::new(b"NONE");

const AFTER_HELP: &str = "\
With no option but --speed the window opens on the menus; a quest picked there
starts as --quest starts it. A game the options ask for opens on the same
menus, which it returns to.
Headless, no player: nothing is built, hired or cast but what the scripts do.
With no --quest or --scene the game is new, made from the picks and HD's
data alone: unpicked, Wizb, Ruib, NONE, KxHa, map 1. With --quest the game is that quest's: its own map, players,
gold and script; --map-seed stands in for the clock word its map is made from,
by default --seed. With --scene the game is new on the scene file's map,
units and clock (regent_core::sim::scene). An answer the world cannot tell takes
its type's default; --headless counts them.";

#[derive(Parser, Clone)]
#[command(name = "openregent", version, about, after_help = AFTER_HELP, max_term_width = 100)]
pub(crate) struct Cli {
    /// Print the licences of the fonts built in, and exit
    #[arg(long, exclusive = true)]
    pub(crate) licences: bool,
    #[command(flatten)]
    pub(crate) setup: Recorded,
    #[command(flatten)]
    pub(crate) mode: Mode,
}

/// The options that make the game, typed into a [`Setup`], which a
/// recording keeps, so a replay takes them from it.
#[derive(clap::Args, Clone)]
pub(crate) struct Recorded {
    /// Override the session's cheat rule; saves keep theirs when unset.
    #[arg(long, value_name = "BOOL")]
    pub(crate) cheats: Option<bool>,
    /// A scene file (TOML, `regent_core::sim::scene`): a new game on its map,
    /// its units placed and set as it says, e.g. `assets/scenes/palace_shadow.toml`
    #[arg(
        long,
        value_name = "PATH",
        conflicts_with_all = [
            "quest", "land", "settlement", "scenario", "monsters", "side",
            "map_seed", "players", "gold", "victory", "goal", "limits", "event1", "event2"
        ]
    )]
    pub(crate) scene: Option<PathBuf>,
    /// The GPL session seed, decimal or 0x hex
    #[arg(long, default_value = DAY_SEED, value_parser = seed)]
    pub(crate) seed: u32,
    /// A quest, by key or name, e.g. `QU06` or `DAY_OF_RECKONING`; given alone,
    /// lists them
    #[arg(
        long,
        value_name = "NAME",
        num_args = 0..=1,
        default_missing_value = LIST,
        conflicts_with_all = ["land", "settlement", "scenario", "monsters", "side"]
    )]
    pub(crate) quest: Option<String>,
    /// Map Number
    #[arg(long, value_name = "N")]
    pub(crate) map_seed: Option<i32>,
    /// Four-character land code; given alone, lists them
    #[arg(long, value_name = "CODE", num_args = 0..=1, default_missing_value = LIST)]
    pub(crate) land: Option<String>,
    /// Cells a side: 128, 256 or 512; by default the scenario's
    #[arg(long)]
    pub(crate) side: Option<u32>,
    /// Four-character settlement code; given alone, lists them
    #[arg(long, value_name = "CODE", num_args = 0..=1, default_missing_value = LIST)]
    pub(crate) settlement: Option<String>,
    /// Four-character scenario code; given alone, lists them
    #[arg(long, value_name = "CODE", num_args = 0..=1, default_missing_value = LIST)]
    pub(crate) scenario: Option<String>,
    /// Four-character Wandering Monsters code, or NONE; given alone, lists them
    #[arg(long, value_name = "CODE", num_args = 0..=1, default_missing_value = LIST)]
    pub(crate) monsters: Option<String>,
    /// Special Event 1, an `EVSC` key such as EV03, or NONE
    #[arg(long, value_name = "CODE", default_value = "NONE")]
    pub(crate) event1: String,
    /// Special Event 2, as --event1
    #[arg(long, value_name = "CODE", default_value = "NONE")]
    pub(crate) event2: String,
    /// Starting gold of a new game, each player's
    #[arg(long, default_value_t = freestyle::GOLD)]
    pub(crate) gold: u32,
    /// Players of a new game, 1 to 4: human seats 0 up, the Player Forces
    /// and Enemy Forces offered for that many
    #[arg(
        long,
        value_name = "N",
        default_value_t = freestyle::ONE_PLAYER,
        value_parser = clap::value_parser!(u32).range(1..=MAX_PLAYERS),
        conflicts_with_all = ["quest"]
    )]
    pub(crate) players: u32,
    /// A new game's victory condition
    #[arg(long, value_enum, default_value_t = Winning::Eliminate)]
    pub(crate) victory: Winning,
    /// Its goal: gold to pass, or days to last; by default the dialog's
    #[arg(long, value_name = "N")]
    pub(crate) goal: Option<u32>,
    /// A new game's Building Limitations: building caps as CODE=N by
    /// commas, e.g. ABB1=0,ABW1=1; by default none
    #[arg(long, value_name = "CAPS", value_parser = caps, conflicts_with_all = ["quest"])]
    pub(crate) limits: Option<String>,
    /// Mods to load, GUIDs by commas, in load order: the last wins a
    /// shared name; e.g. {D666FCBE-9906-4582-A6A0-DC96F812441E}. None by
    /// default; never on the LAN, as HD; a loaded save plays with its own
    #[arg(long, value_name = "GUIDS", conflicts_with_all = ["server", "client"])]
    pub(crate) mods: Option<String>,
}

/// The options of how the game runs, never recorded: a replay keeps
/// them.
#[derive(clap::Args, Clone)]
pub(crate) struct Mode {
    /// A save of this game's own (`.tar.zst`) to go on from, as the Load
    /// Game page loads it
    #[arg(long, value_name = "PATH", conflicts_with_all = ["scene", "replay"])]
    pub(crate) load: Option<PathBuf>,
    /// Game speed; 1 is the engine's default, a 50 ms frame each 50 real ms.
    /// Unset, each game starts at the settings' `[game] speed`
    #[arg(long)]
    pub(crate) speed: Option<f32>,
    /// What the message box types and scrolls on: real, the wall's as the
    /// engine's, or game, the simulation's (paused, lines stay)
    #[arg(long, value_enum, default_value_t = chat::Clock::Real)]
    pub(crate) message_clock: chat::Clock,
    /// Headless: a scripted player, its stream seeded with this, presses
    /// what the panels offer every five game seconds (a soak test)
    #[arg(long, value_name = "SEED", value_parser = seed)]
    pub(crate) bot: Option<u32>,
    /// Run without a window and print a summary
    #[arg(long)]
    pub(crate) headless: bool,
    /// The seed of the faces behind the menus, decimal or 0x hex; unset,
    /// the clock's, or 0 for a screenshot
    #[arg(long, value_name = "SEED", value_parser = seed)]
    pub(crate) faces_seed: Option<u32>,
    /// Frames to run headless (default 1254), or to draw before a
    /// screenshot (default 60)
    #[arg(long, value_name = "N")]
    pub(crate) frames: Option<u32>,
    /// Draw the opening view, fog lifted and paused, save one frame as
    /// this PNG and exit; nothing else is written
    #[arg(long, value_name = "PNG", conflicts_with = "headless")]
    pub(crate) screenshot: Option<PathBuf>,
    /// Open the menus on this page: main, profiles, map[-top|-bottom][:QUEST|:ID]
    /// (the quest or control hovered), extras[:QUEST] (its row lit), erase, settings, background:QUEST,
    /// summary:QUEST (a code or a mod quest's GUID), lore:CODE (the Book of Lore on that entry); past the profiles, the last profile's victories
    #[allow(rustdoc::broken_intra_doc_links, reason = "clap help, not a link")]
    #[arg(long, value_name = "PAGE", conflicts_with_all = ["headless", "bot", "replay", "quest"])]
    pub(crate) menu: Option<String>,
    /// The screenshot's window, in pixels
    #[arg(long, value_name = "WxH", default_value = shot::SIZE, value_parser = shot::size)]
    pub(crate) size: shot::Size,
    /// With --screenshot: select the Nth piece (from 0) of this
    /// descriptor, as CODE or CODE:N (CODE may be an XML ID)
    #[arg(long, value_name = "CODE[:N]", requires = "screenshot", value_parser = shot::chosen)]
    pub(crate) select: Option<(regent_core::data::fourcc::FourCc, usize)>,
    /// With --screenshot: arm this spell, as its button does, once the
    /// game offers it
    #[arg(long, value_name = "CODE", requires = "screenshot", value_parser = shot::spell)]
    pub(crate) aim: Option<regent_core::data::fourcc::FourCc>,
    /// With --screenshot: the page to open under the selection
    #[arg(long, value_parser = pages(), requires = "screenshot")]
    pub(crate) page: Option<panel::Page>,
    /// With --screenshot: play this many frames first, the replay's and
    /// the bot's input in them
    #[arg(long, value_name = "N", requires = "screenshot")]
    pub(crate) warm: Option<u32>,
    /// With --screenshot: the game runs through the drawn frames, not
    /// paused (the perf runs, `tools/perf/`)
    #[arg(long, requires = "screenshot")]
    pub(crate) running: bool,
    /// With --screenshot: `[enhancements] ui_upscale` for the capture
    #[arg(long, value_enum, requires = "screenshot")]
    pub(crate) ui_upscale: Option<config::Upscale>,
    /// With --screenshot: `[enhancements] view_upscale` for the capture
    #[arg(long, value_enum, requires = "screenshot")]
    pub(crate) view_upscale: Option<config::ViewUpscale>,
    /// With --screenshot: `[enhancements] video_upscale` for the capture
    #[arg(long, value_enum, requires = "screenshot")]
    pub(crate) video_upscale: Option<config::VideoUpscale>,
    /// With --screenshot: `[enhancements] shadows` for the capture
    #[arg(long, value_enum, requires = "screenshot")]
    pub(crate) shadows: Option<config::Shadows>,
    /// With --screenshot: `[enhancements] cast` for the capture
    #[arg(long, value_enum, requires = "screenshot")]
    pub(crate) cast: Option<config::Cast>,
    /// With --screenshot: `[enhancements] sun_direction` for the capture
    #[arg(
        long,
        value_name = "DEG",
        requires = "screenshot",
        allow_negative_numbers = true
    )]
    pub(crate) sun_direction: Option<f32>,
    /// With --screenshot: `[enhancements] sun_elevation` for the capture
    #[arg(long, value_name = "DEG", requires = "screenshot")]
    pub(crate) sun_elevation: Option<f32>,
    /// With --screenshot: `[enhancements] sun_size` for the capture
    #[arg(long, value_name = "DEG", requires = "screenshot")]
    pub(crate) sun_size: Option<f32>,
    /// With --screenshot: `[enhancements] self_shadows` for the capture
    #[arg(long, value_name = "BOOL", requires = "screenshot")]
    pub(crate) self_shadows: Option<bool>,
    /// With --screenshot: `[ui] font` for the capture
    #[arg(long, value_enum, requires = "screenshot")]
    pub(crate) font: Option<config::Typeface>,
    /// With --screenshot: `[enhancements] rings` for the capture
    #[arg(long, value_enum, requires = "screenshot")]
    pub(crate) rings: Option<config::Rings>,
    /// With --screenshot: `[enhancements] side_ring` for the capture
    #[arg(long, value_name = "BOOL", requires = "screenshot")]
    pub(crate) side_ring: Option<bool>,
    /// With --screenshot: `[enhancements] smooth_position` for the capture
    #[arg(long, value_name = "BOOL", requires = "screenshot")]
    pub(crate) smooth_position: Option<bool>,
    /// With --screenshot: trees sway (`[enhancements] wind`), the wind held
    /// at this game ms
    #[arg(long, value_name = "MS", requires = "screenshot")]
    pub(crate) wind_at: Option<u32>,
    /// With --screenshot: `[enhancements] wind_amplitude`, art pixels a
    /// tree's top leans
    #[arg(long, value_name = "PX", requires = "screenshot")]
    pub(crate) wind_amplitude: Option<f32>,
    /// With --wind-at: whether trees' shadows sway too
    #[arg(long, value_enum, requires = "wind_at", default_value_t)]
    pub(crate) wind_shadow: regent_draw::wind::Shadow,
    /// With --screenshot: the fog `fogged`, drawn, or `lifted` (default),
    /// as the captures' reveal cheat
    #[arg(long, value_enum, requires = "screenshot", default_value_t = fog::Sight::Lifted)]
    pub(crate) fog: fog::Sight,
    /// With --screenshot: place this building, as its build menu row
    /// does: its preview under --pointer
    #[arg(long, value_name = "CODE", requires = "screenshot", value_parser = shot::building)]
    pub(crate) build: Option<regent_core::data::fourcc::FourCc>,
    /// With --screenshot: pixels a logical point, as a display's scale
    /// (1.8 at 180 %); --size stays the PNG's pixels
    #[arg(long, value_name = "S", default_value_t = shot::SCALE, requires = "screenshot")]
    pub(crate) scale: f32,
    /// With --screenshot: draw at this view zoom, smooth zoom on; the
    /// quest map at it (window pixels a map pixel), about the quest named
    #[arg(long, value_name = "Z", requires = "screenshot")]
    pub(crate) zoom: Option<f32>,
    /// With --screenshot: the game won or lost by the cheat after the
    /// warm frames, drawn at this stage of its end
    #[arg(long, value_enum, requires = "screenshot")]
    pub(crate) end: Option<shot::Ending>,
    /// With --screenshot: centre the view on this cell, X,Y
    #[arg(long, value_name = "X,Y", requires = "screenshot", value_parser = shot::cell)]
    pub(crate) look: Option<(f32, f32)>,
    /// With --screenshot: centre the view on this cell first, X,Y, then
    /// move to --look: a pan
    #[arg(long, value_name = "X,Y", requires = "look", value_parser = shot::cell)]
    pub(crate) look_from: Option<(f32, f32)>,
    /// With --screenshot: the minimap held pressed on this cell, X,Y: the
    /// view centred there each frame, as a drag on it does
    #[arg(long, value_name = "X,Y", requires = "screenshot", value_parser = shot::cell)]
    pub(crate) mini: Option<(f32, f32)>,
    /// With --screenshot: the pointer resting at this window point, X,Y
    /// in points
    #[arg(long, value_name = "X,Y", requires = "screenshot", value_parser = shot::cell)]
    pub(crate) pointer: Option<(f32, f32)>,
    /// With --screenshot: move the view along this path each drawn frame
    /// (the perf runs)
    #[arg(long, value_enum, requires = "screenshot")]
    pub(crate) camera: Option<shot::Camera>,
    /// With --screenshot: open this menu over the game: options (AP36),
    /// goals (AP88), settings (the editor, over AP36), save (AP49), load
    /// (AP27) or overwrite (AP49's question on a name taken)
    #[arg(long, value_enum, requires = "screenshot")]
    pub(crate) dialog: Option<shot::Dialog>,
    /// With --screenshot: the settings editor's tab, as --menu settings
    /// or --dialog settings opens it
    #[arg(long, value_enum, requires = "screenshot")]
    pub(crate) settings_tab: Option<menu::settings::Tab>,
    /// This machine's player, a human seat of the game: what it shows and
    /// hears and whose orders it gives. Not recorded
    #[arg(long, value_name = "N", default_value_t = CROWN)]
    pub(crate) local: u32,
    /// Play a recorded session again: its setup, then its inputs at their
    /// frames (`play.input` beside the log); its setup replaces the game's
    /// options
    #[arg(long, value_name = "PATH")]
    pub(crate) replay: Option<PathBuf>,
    /// Host a LAN game on this UDP port, as player 0: a new game, its
    /// options sent to every guest; --players 2 unless given, the map
    /// number the clock's unless given
    #[arg(
        long,
        value_name = "PORT",
        num_args = 0..=1,
        default_missing_value = net::PORT_TEXT,
        conflicts_with_all = ["client", "replay", "scene", "quest", "screenshot", "menu", "local"]
    )]
    pub(crate) server: Option<u16>,
    /// Join the LAN game hosted at HOST\[:PORT]: the host's options make
    /// the game, and pick this machine's player
    #[arg(
        long,
        value_name = "HOST[:PORT]",
        conflicts_with_all = ["replay", "scene", "screenshot", "menu", "local"]
    )]
    pub(crate) client: Option<String>,
    /// LAN host: frames an order waits to reach every peer, as given;
    /// unless given, picked from the guests' pings
    #[arg(
        long,
        value_name = "N",
        value_parser = clap::value_parser!(u32).range(i64::from(net::LEAST_DELAY)..=i64::from(net::MOST_DELAY))
    )]
    pub(crate) net_delay: Option<u32>,
    /// LAN host: let guests run another commit of the same version,
    /// profile and game data
    #[arg(long, requires = "server")]
    pub(crate) net_any_build: bool,
}

/// `--page`'s names for the panel pages.
const PAGES: [(&str, panel::Page); 18] = [
    ("help", panel::Page::Help),
    ("build", panel::Page::Build),
    ("rewards", panel::Page::Rewards),
    ("members", panel::Page::Members),
    ("visitors", panel::Page::Visitors),
    ("research", panel::Page::Research),
    ("crypt", panel::Page::Crypt),
    ("graves", panel::Page::Graves),
    ("bounty", panel::Page::Bounty),
    ("casting", panel::Page::Casting),
    ("seekers", panel::Page::Seekers),
    ("stats", panel::Page::Stats),
    ("items", panel::Page::Items),
    ("spells", panel::Page::Spells),
    ("statistics", panel::Page::Statistics),
    ("heroes", panel::Page::Heroes),
    ("buildings", panel::Page::Buildings),
    ("henchmen", panel::Page::Henchmen),
];

/// `--page`'s parser: one of [`PAGES`]' names.
fn pages() -> impl clap::builder::TypedValueParser<Value = panel::Page> {
    use clap::builder::TypedValueParser as _;

    clap::builder::PossibleValuesParser::new(PAGES.map(|(name, _)| name)).map(|name| {
        PAGES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, page)| *page)
            .unwrap_or_default()
    })
}

/// A seed in decimal or `0x` hex.
fn seed(text: &str) -> Result<u32, String> {
    match text.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => text.parse(),
    }
    .map_err(|e| format!("{text}: {e}"))
}

/// `--limits`'s parser: caps as [`Tree::caps_text`] writes them, kept so.
fn caps(text: &str) -> Result<String, String> {
    Tree::caps_of(text).map(|_| text.to_owned())
}

impl TryFrom<&Recorded> for Setup {
    type Error = String;

    /// The options given, typed: a code not four bytes, or a mod not a
    /// GUID, refused.
    fn try_from(r: &Recorded) -> Result<Setup, String> {
        let code = |c: &Option<String>| c.as_deref().map(custom::code).transpose();
        let mods = r
            .mods
            .iter()
            .flat_map(|list| list.split(','))
            .filter(|text| !text.trim().is_empty())
            .map(|text| {
                setup::Wanted::parse(text).ok_or_else(|| format!("--mods: {text} is no GUID"))
            })
            .collect::<Result<_, _>>()?;

        Ok(Setup {
            cheats: r.cheats.map(setup::cheat_rule),
            scene: r.scene.clone(),
            seed: r.seed,
            quest: r.quest.clone(),
            map_seed: r.map_seed,
            land: code(&r.land)?,
            side: r.side,
            settlement: code(&r.settlement)?,
            scenario: code(&r.scenario)?,
            monsters: code(&r.monsters)?,
            events: [custom::code(&r.event1)?, custom::code(&r.event2)?],
            gold: r.gold,
            players: r.players,
            victory: r.victory,
            goal: r.goal,
            limits: r.limits.as_deref().map(Tree::caps_of).transpose()?,
            mods,
        })
    }
}

impl Mode {
    /// Frames to run headless.
    pub(crate) fn headless_frames(&self) -> u32 {
        self.frames.unwrap_or(DAY_FRAMES)
    }

    /// A screenshot's play before it is drawn; none without one.
    pub(crate) fn warm(&self) -> Option<begin::Warm> {
        self.screenshot.as_ref().map(|_| begin::Warm {
            frames: self.warm.unwrap_or(0),
            bot: self.bot,
            end: self.end,
        })
    }

    /// The screenshot asked for, if any.
    pub(crate) fn shot(&self) -> Option<shot::Shot> {
        self.screenshot.as_ref().map(|path| shot::Shot {
            path: path.clone(),
            size: self.size,
            scale: self.scale,
            frames: Some(self.frames.unwrap_or(shot::FRAMES)),
            waited: 0,
            settled: 0,
            ready: 0,
            counted: 0,
            select: self.select,
            aim: self.aim,
            build: self.build,
            page: self.page,
            zoom: self.zoom,
            end: self.end,
            look: self.look,
            from: self.look_from,
            pointer: self.pointer,
            mini: self.mini,
            hold: if self.running {
                shot::Hold::Running
            } else {
                shot::Hold::Paused
            },
            camera: self.camera,
            dialog: self.dialog,
            settings_tab: self.settings_tab,
            wind: self.wind_at,
            wind_shadow: self.wind_shadow,
            sight: self.fog,
        })
    }
}

/// What a dialog list offers: each code and its name.
type Offers = Vec<(FourCc, String)>;

/// One code option: its flag, the dialog list it picks from, the value
/// given and the default.
struct CodeOption {
    flag: &'static str,
    list: List,
    given: Option<String>,
    default: FourCc,
}

/// The code options, in `--help` order, given as `given` holds them:
/// land, settlement, scenario, monsters.
fn code_options(given: [Option<String>; 4]) -> [CodeOption; 4] {
    let (settlement, scenario, monsters, land, _) = custom::DEFAULTS;
    let [g_land, g_settlement, g_scenario, g_monsters] = given;

    [
        CodeOption::new("--land", List::Land, g_land, land),
        CodeOption::new("--settlement", List::Settlement, g_settlement, settlement),
        CodeOption::new("--scenario", List::Scenario, g_scenario, scenario),
        CodeOption::new("--monsters", List::Monsters, g_monsters, monsters),
    ]
}

impl Recorded {
    /// The code options, as typed.
    fn code_options(&self) -> [CodeOption; 4] {
        code_options([
            self.land.clone(),
            self.settlement.clone(),
            self.scenario.clone(),
            self.monsters.clone(),
        ])
    }
}

impl CodeOption {
    const fn new(flag: &'static str, list: List, given: Option<String>, default: FourCc) -> Self {
        CodeOption {
            flag,
            list,
            given,
            default,
        }
    }
}

/// Whether a list option was given alone.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Lists {
    /// None: play.
    None,
    /// Its list printed: stop.
    Printed,
}

/// Print the list each list option given alone asks for: `--quest`'s,
/// else each code option's.
pub(crate) fn lists(recorded: &Recorded, install: &Install) -> Result<Lists, String> {
    if recorded.quest.as_deref() == Some(LIST) {
        println!("{}", quest_list(&quests(install)?).trim_end());
        Ok(Lists::Printed)
    } else {
        let asked: Vec<CodeOption> = recorded
            .code_options()
            .into_iter()
            .filter(|o| o.given.as_deref() == Some(LIST))
            .collect();

        for (option, offers) in offered_each(&asked, recorded.players, &install.hd)? {
            println!("{}", listing(option, &offers).trim_end());
        }

        Ok(if asked.is_empty() {
            Lists::None
        } else {
            Lists::Printed
        })
    }
}

/// Refuse a code `setup` picks that its dialog does not offer, the list
/// in the error.
pub(crate) fn codes(setup: &Setup, hd: &Path) -> Result<(), String> {
    let text = |c: Option<FourCc>| c.map(|c| c.to_string());
    let options = code_options([
        text(setup.land),
        text(setup.settlement),
        text(setup.scenario),
        text(setup.monsters),
    ]);

    for (option, offers) in offered_each(&options, setup.players, hd)? {
        let given = option.given.as_deref().unwrap_or_default();

        if !offers
            .iter()
            .any(|(code, _)| code.as_bytes() == given.as_bytes())
        {
            Err(format!(
                "{given} is not a {} code; one of:\n{}",
                option.flag,
                listing(option, &offers)
            ))?;
        }
    }

    Ok(())
}

/// Each option of `options` given, with what the dialog of `players`
/// offers for it. The names are the base game's texts, read where a
/// code is given.
fn offered_each<'a>(
    options: &'a [CodeOption],
    players: u32,
    hd: &Path,
) -> Result<Vec<(&'a CodeOption, Offers)>, String> {
    let given: Vec<&CodeOption> = options.iter().filter(|o| o.given.is_some()).collect();

    if given.is_empty() {
        Ok(Vec::new())
    } else {
        let text = Text::build(&regent_draw::data::catalogue(
            &crate::game::load::archives(hd)?,
        )?);

        given
            .into_iter()
            .map(|option| Ok((option, offered(hd, (option, players), &text)?)))
            .collect()
    }
}

/// What the dialog of `players` offers for `option`, `NONE` first for
/// the monsters, then the Random picks ([`crate::game::load::randoms`]).
fn offered(
    hd: &Path,
    (option, players): (&CodeOption, u32),
    text: &Text<'_>,
) -> Result<Offers, String> {
    let none =
        (option.list == List::Monsters).then(|| (NO_MONSTERS, crate::game::load::none_name(text)));

    Ok(none
        .into_iter()
        .chain(crate::game::load::randoms(option.list, text))
        .chain(crate::game::load::offers(hd, option.list, players)?)
        .collect())
}

/// One line per code, its name beside it, the default marked:
///
/// ```text
///   Rave  Ravenswood
///   KxHa  K'Hal-Kratis  (default)
/// ```
fn listing(option: &CodeOption, offers: &[(FourCc, String)]) -> String {
    offers.iter().fold(
        format!("{} codes:\n", option.flag),
        |mut out, (code, name)| {
            let mark = if *code == option.default {
                "  (default)"
            } else {
                ""
            };

            let _ = writeln!(out, "  {code}  {name}{mark}");
            out
        },
    )
}

/// How the window opens: on the menus, or on the game the options ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Opening {
    Menus,
    Options,
}

/// Options that still open the menus: they set how a game runs, not which.
const MENU_KEEPS: [&str; 5] = [
    "speed",
    "message_clock",
    "local",
    "net_delay",
    "net_any_build",
];

/// The menus open when no option is given but [`MENU_KEEPS`]: no setup,
/// replay, headless run or screenshot.
pub(crate) fn opening(matches: &clap::ArgMatches) -> Opening {
    // The arguments, not `matches.ids()`: that holds the derive's group too.
    let given = Cli::command()
        .get_arguments()
        .map(|arg| arg.get_id().as_str())
        .filter(|id| !MENU_KEEPS.contains(id))
        .any(|id| matches.value_source(id) == Some(ValueSource::CommandLine));

    if given {
        Opening::Options
    } else {
        Opening::Menus
    }
}

/// `cli` as a LAN host plays it: two players, and the map number the
/// clock's as the menus roll it, where not given.
pub(crate) fn hosted(mut cli: Cli, matches: &clap::ArgMatches) -> Cli {
    if cli.mode.server.is_some() {
        if matches.value_source("players") != Some(ValueSource::CommandLine) {
            cli.setup.players = 2;
        }

        cli.setup.map_seed = cli.setup.map_seed.or_else(|| {
            let rolled = regent_core::sim::mapgen::preset::map_number(menu::freestyle::clock());

            Some(rolled.max(menu::freestyle::MAP_NUMBER_MIN))
        });
    }

    cli
}

/// The host's setup `welcome` from `host` carries, which the guest plays.
///
/// # Errors
///
/// A setup naming mods.
pub(crate) fn joined(welcome: &net::Welcome, host: &str) -> Result<Setup, String> {
    // HD plays no mods on the LAN, nor does
    // this game: a host's setup naming some is refused.
    if welcome.setup.mods.is_empty() {
        Ok(welcome.setup.clone())
    } else {
        Err(format!("{host}'s game names mods: LAN games play none"))
    }
}

/// The quest `setup` asks for, as [`quest_of`] finds it among those
/// offered.
pub(crate) fn quest(
    setup: &Setup,
    install: &Install,
) -> Result<Option<Box<crate::game::load::Offered>>, String> {
    let offered = if setup.quest.is_some() {
        quests(install)?
    } else {
        Vec::new()
    };

    quest_of(setup, &offered)?
        .map(|index| {
            let quest = offered.into_iter().nth(index).ok_or("no quest")?;

            Ok(Box::new(quest))
        })
        .transpose()
}

/// The quest of `offered` `setup` asks for, by its index: its quest's,
/// by key, name or whole id in any case, the first offered, a name not
/// offered an error carrying the list; else none.
///
/// # Errors
///
/// A quest not offered.
pub(crate) fn quest_of(
    setup: &Setup,
    offered: &[crate::game::load::Offered],
) -> Result<Option<usize>, String> {
    let named = |given: &str, o: &crate::game::load::Offered| {
        o.quest.key.to_string().eq_ignore_ascii_case(given)
            || o.quest.name.eq_ignore_ascii_case(given)
            || Guid::parse(given) == Some(o.quest.id)
    };

    setup
        .quest
        .as_deref()
        .map(|given| {
            offered
                .iter()
                .position(|o| named(given, o))
                .ok_or_else(|| format!("{given} is no quest; one of:\n{}", quest_list(offered)))
        })
        .transpose()
}

/// One line per quest, in the list's order:
///
/// ```text
///   QU06  DAY_OF_RECKONING      Day of Reckoning
///   XQ07  SIEGE                 The Siege  (expansion)
///   {E502799E-…}  BigMap        13 - Big Map  (expansion, mod 2827106524)
/// ```
fn quest_list(offered: &[crate::game::load::Offered]) -> String {
    offered
        .iter()
        .fold("--quest names:\n".to_owned(), |mut out, o| {
            let dataset = o.quest.expansion().then_some("expansion".to_owned());
            let package = match &o.origin {
                crate::game::load::Origin::Install => None,
                crate::game::load::Origin::Mod(dir) => Some(format!(
                    "mod {}",
                    dir.file_name().unwrap_or_default().to_string_lossy()
                )),
            };
            let marks: Vec<String> = [dataset, package].into_iter().flatten().collect();
            let mark = if marks.is_empty() {
                String::new()
            } else {
                format!("  ({})", marks.join(", "))
            };

            let _ = writeln!(
                out,
                "  {}  {:<20}  {}{mark}",
                o.named(),
                o.quest.name,
                o.title
            );
            out
        })
}

/// With `--replay`, the recording's setup in `setup`'s place, and its
/// events; else `setup` and none.
pub(crate) fn replayed(mode: &Mode, setup: Setup) -> Result<(Setup, Vec<record::Event>), String> {
    match &mode.replay {
        None => Ok((setup, Vec::new())),
        Some(path) => {
            let session = record::read(path)?;
            let played: Vec<_> = session
                .events
                .iter()
                .filter(|e| !matches!(e, record::Event::Digest { .. }))
                .collect();
            let last = played.last().map_or(0, |e| e.frame());

            info!(
                "replay: {}, {} events, the last at frame {last}",
                path.display(),
                played.len()
            );

            Ok((session.setup, session.events))
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use clap::FromArgMatches;
    use regent_core::sim::freestyle::Victory;
    use regent_core::sim::world::cheat::Cheats;

    use crate::{files::env, game::drive, save};

    /// The setup `args` give, the program's name first.
    pub(crate) fn parsed<I, T>(args: I) -> Setup
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        Setup::try_from(&Cli::parse_from(args).setup).expect("a setup")
    }

    /// `setup` as a recording keeps it, written and read back.
    fn kept(setup: &Setup) -> Setup {
        let session = record::Session {
            setup: setup.recorded(),
            events: Vec::new(),
        };

        record::parse(&session.text()).expect("read back").setup
    }

    /// A new game records its unpicked map codes, Map Number and goal as
    /// their defaults, so a replay makes the same game.
    #[test]
    fn a_new_game_records_its_defaults() {
        let (settlement, scenario, monsters, land, map_seed) = custom::DEFAULTS;
        let recorded = kept(&parsed(["openregent", "--victory", "gold"]));

        assert_eq!(
            [
                recorded.settlement,
                recorded.scenario,
                recorded.monsters,
                recorded.land
            ],
            [Some(settlement), Some(scenario), Some(monsters), Some(land)]
        );
        assert_eq!(recorded.map_seed, Some(map_seed));
        assert_eq!(recorded.goal, Some(60_000));
        assert_eq!(recorded.recorded(), recorded);
    }

    /// No option given is the default setup.
    #[test]
    fn no_option_is_the_default_setup() {
        assert_eq!(parsed(["openregent"]), Setup::default());
        assert_eq!(seed(DAY_SEED), Ok(setup::SEED));
    }

    /// A replay takes the recording's setup in place of the options'.
    #[test]
    fn a_replay_takes_the_recordings_setup() {
        let dir = crate::testdata::scratch("replay");
        let path = dir.path().join("play.input");

        let session = record::Session {
            setup: Setup {
                seed: 7,
                gold: 900,
                ..Setup::default()
            },
            events: Vec::new(),
        };

        std::fs::write(&path, session.text()).expect("written");

        let given = [
            "openregent",
            "--replay",
            path.to_str().expect("utf-8"),
            "--gold",
            "100",
        ];
        let cli = Cli::parse_from(given);
        let (replayed, _) = replayed(&cli.mode, parsed(given)).expect("replayed");

        assert_eq!((replayed.seed, replayed.gold), (7, 900));
    }

    #[test]
    fn an_unset_goal_is_the_dialogs() {
        let gold = parsed(["openregent", "--victory", "gold"]);
        let days = parsed(["openregent", "--victory", "survive", "--goal", "15"]);

        assert_eq!(gold.aim(), (Victory::Gold, 60_000));
        assert_eq!(days.aim(), (Victory::Survive, 15));
        assert_eq!(parsed(["openregent"]).aim(), (Victory::Eliminate, 0));
    }

    /// Last Palace, offered with more than one player; alone, Eliminate
    /// as the dialog turns it.
    #[test]
    fn last_palace_is_a_game_of_more_than_one() {
        let aim = |args: &[&str]| {
            let all = ["openregent", "--victory", "last-palace"].into_iter();

            parsed(all.chain(args.iter().copied())).aim()
        };

        assert_eq!(aim(&["--players", "2"]), (Victory::LastPalace, 0));
        assert_eq!(aim(&[]), (Victory::Eliminate, 0));
    }

    #[test]
    fn the_menus_open_only_without_setup_or_mode_options() {
        let opens = |args: &[&str]| {
            let all = std::iter::once("openregent").chain(args.iter().copied());

            opening(&Cli::command().get_matches_from(all))
        };
        let options = [
            &["--quest", "QU06"][..],
            &["--seed", "1"],
            &["--gold", "3000"],
            &["--land", "KxHa"],
            &["--replay", "play.input"],
            &["--headless"],
            &["--screenshot", "x.png"],
            &["--server"],
            &["--client", "host"],
        ];

        assert_eq!(opens(&[]), Opening::Menus);
        assert_eq!(opens(&["--net-delay", "5"]), Opening::Menus);
        assert_eq!(opens(&["--speed", "2"]), Opening::Menus);
        assert_eq!(opens(&["--message-clock", "game"]), Opening::Menus);
        assert_eq!(opens(&["--local", "1"]), Opening::Menus);
        assert_eq!(opens(&["--players", "2"]), Opening::Options);

        for args in options {
            assert_eq!(opens(args), Opening::Options, "{args:?}");
        }
    }

    /// A LAN host seats two players on a map number the clock rolls,
    /// unless told; it hosts a new game only.
    #[test]
    fn a_host_plays_two_players_on_a_rolled_map_unless_told() {
        let host = |args: &[&str]| {
            let all = std::iter::once("openregent").chain(args.iter().copied());
            let matches = Cli::command().try_get_matches_from(all)?;

            Cli::from_arg_matches(&matches).map(|cli| hosted(cli, &matches))
        };
        let plain = host(&["--server"]).expect("parsed");
        let told =
            host(&["--server", "5000", "--players", "3", "--map-seed", "7"]).expect("parsed");
        let alone = host(&["--players", "3"]).expect("parsed");

        assert_eq!(
            plain.mode.server,
            Some(net::PORT_TEXT.parse().expect("port"))
        );
        assert_eq!(plain.setup.players, 2);
        assert!(plain.setup.map_seed.is_some());
        assert_eq!((told.mode.server, told.setup.players), (Some(5000), 3));
        assert_eq!(told.setup.map_seed, Some(7));
        assert_eq!((alone.setup.players, alone.setup.map_seed), (3, None));
        assert!(host(&["--server", "--client", "h"]).is_err());
        assert!(host(&["--client", "h", "--local", "1"]).is_err());
        assert!(host(&["--server", "--net-delay", "1"]).is_err());
    }

    /// The delay is picked from the pings unless `--net-delay` gives it.
    #[test]
    fn the_net_delay_is_automatic_unless_given() {
        let delay = |args: &[&str]| {
            let all = std::iter::once("openregent").chain(args.iter().copied());

            Cli::try_parse_from(all).map(|cli| cli.mode.net_delay)
        };

        assert_eq!(delay(&["--server"]).expect("parsed"), None);
        assert_eq!(
            delay(&["--server", "--net-delay", "5"]).expect("parsed"),
            Some(5)
        );
        assert!(delay(&["--server", "--net-delay", "51"]).is_err());
    }

    /// A game hosted from the menus records the freestyle config and its
    /// player count; the guest's game reads back from that, on its seat.
    #[test]
    fn a_hosted_custom_game_records_its_players_and_a_guest_reads_it_back() {
        let custom = menu::freestyle::Custom {
            map_number: 4242,
            gold: 15_000,
            cheats: Cheats::Barred,
            ..menu::freestyle::Custom::default()
        };
        let recorded = kept(&Setup::hosted(&custom, 3));

        assert_eq!(recorded.players, 3);
        assert_eq!(recorded.map_seed, Some(4242));
        assert_eq!(recorded.gold, 15_000);
        assert_eq!(recorded.cheats, Some(Cheats::Barred));

        let welcome = net::Welcome {
            seat: 2,
            players: 3,
            delay: 4,
            setup: recorded.clone(),
            nicks: Vec::new(),
            host: 0,
            save: None,
        };
        let guest = joined(&welcome, "host").expect("read back");

        assert_eq!(guest.recorded(), recorded);
        assert!(
            joined(
                &net::Welcome {
                    setup: parsed(["openregent", "--mods", GUARDHOUSE_MOD]),
                    ..welcome
                },
                "host"
            )
            .is_err()
        );
    }

    /// A guest's lobby reads the host's config back from its recorded
    /// setup, but the tree's name.
    #[test]
    fn a_hosted_setup_reads_back_as_its_config() {
        let custom = menu::freestyle::Custom {
            scenario: FourCc::new(b"Shad"),
            land: FourCc::new(b"East"),
            map_number: 680_631_669,
            gold: 20_000,
            victory: Victory::Gold,
            goal: 60_000,
            events: [FourCc::new(b"EV03"), FourCc::new(b"NONE")],
            caps: [(FourCc::new(b"ABF1"), 0)].into_iter().collect(),
            buildings: "No Inns".to_owned(),
            cheats: Cheats::Barred,
            ..menu::freestyle::Custom::default()
        };
        let read = kept(&Setup::hosted(&custom, 2)).custom();

        assert_eq!(
            read,
            menu::freestyle::Custom {
                buildings: menu::freestyle::Custom::default().buildings,
                ..custom
            }
        );
    }

    #[test]
    fn a_seed_reads_as_hex_or_decimal() {
        assert_eq!(seed("191929900"), Ok(191_929_900));
        assert!(seed("0xzz").is_err());
    }

    #[test]
    fn a_map_seed_is_the_picks_seed() {
        let cli = parsed(["openregent"]);
        let generated = parsed(["openregent", "--map-seed", "7"]);

        assert_eq!(cli.picks().seed, None);
        assert_eq!(generated.picks().seed, Some(7));
    }

    /// The setup page's config becomes the options a custom game is made
    /// and recorded from, and the recording makes the same options again.
    #[test]
    fn a_freestyle_config_is_the_custom_games_options() {
        let custom = menu::freestyle::Custom {
            scenario: FourCc::new(b"Shad"),
            land: FourCc::new(b"East"),
            map_number: 680_631_669,
            gold: 20_000,
            victory: Victory::Gold,
            goal: 60_000,
            events: [FourCc::new(b"EV03"), FourCc::new(b"NONE")],
            ..menu::freestyle::Custom::default()
        };
        let cli = Setup::default().freestyle(&custom);
        let picks = cli.picks();
        let again = kept(&cli);

        assert_eq!(picks.settlement, Some(FourCc::new(b"BEGI")));
        assert_eq!(picks.scenario, Some(FourCc::new(b"Shad")));
        assert_eq!(picks.monsters, Some(FourCc::new(b"NONE")));
        assert_eq!(picks.land, Some(FourCc::new(b"East")));
        assert_eq!(picks.seed, Some(680_631_669));
        assert_eq!(picks.side, None);
        assert_eq!(cli.gold, 20_000);
        assert_eq!(cli.aim(), (Victory::Gold, 60_000));
        assert_eq!(cli.events, custom.events);
        assert!(cli.quest.is_none());

        assert_eq!(format!("{:?}", again.picks()), format!("{:?}", cli.picks()));
        assert_eq!(again.aim(), cli.aim());
        assert_eq!(again.events, cli.events);
        assert_eq!(again.gold, cli.gold);
    }

    #[test]
    fn a_code_option_alone_asks_for_its_list() {
        let cli = Cli::parse_from(["openregent", "--land", "--headless", "--monsters", "NONE"]);
        let given = cli.setup.code_options().map(|o| o.given);
        let given = given.each_ref().map(Option::as_deref);

        assert_eq!(given, [Some(LIST), None, None, Some("NONE")]);
        assert!(cli.mode.headless);
    }

    #[test]
    fn a_quest_records_its_name_and_map_word_and_no_picks() {
        let cli = parsed(["openregent", "--quest", "qu06"]);
        let replayed = kept(&cli);

        assert_eq!(replayed.quest.as_deref(), Some("qu06"));
        assert_eq!(replayed.map_seed, Some(cli.word()));
        assert_eq!((replayed.land, replayed.goal), (None, None));
    }

    /// A new game records its players, which a replay reads; one or more
    /// to four, and no quest's.
    #[test]
    fn a_new_game_records_its_players() {
        let cli = parsed(["openregent", "--players", "3"]);
        let parse = |args: &[&str]| {
            Cli::try_parse_from(std::iter::once("openregent").chain(args.iter().copied()))
        };

        assert_eq!(kept(&cli).players, 3);
        assert_eq!(cli.picks().players(), 3);
        assert!(parse(&["--players", "0"]).is_err());
        assert!(parse(&["--players", "5"]).is_err());
        assert!(parse(&["--players", "2", "--quest", "QU06"]).is_err());
    }

    /// `--local` picks one of the game's human seats; one past them is
    /// refused.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_local_player_is_one_of_the_games_seats() {
        let mut two = crate::game::begin::test_game(&["--players", "2"]).0.sim;

        assert_eq!(two.world().humans(), 0..2);
        assert_eq!(two.set_local(1), Ok(()));
        assert_eq!(two.local(), 1);
        assert!(two.set_local(2).is_err());
        assert_eq!(two.local(), 1);

        let mut one = crate::game::begin::test_game(&[]).0.sim;

        assert!(one.set_local(1).is_err());
        assert_eq!(one.local(), CROWN);
    }

    /// The example scene, beside the workspace.
    fn example_scene() -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/scenes/palace_shadow.toml")
            .display()
            .to_string()
    }

    /// A scene game records its file and none of the picks it replaces,
    /// and reads back from what it recorded; a pick beside it is refused.
    #[test]
    fn a_scene_records_its_path_and_no_picks() {
        let path = example_scene();
        let cli = parsed(["openregent", "--scene", &path]);
        let replayed = kept(&cli);

        assert_eq!((replayed.land, replayed.map_seed), (None, None));
        assert_eq!(replayed.goal, None);
        assert_eq!(
            replayed.scene,
            cli.scene.map(|p| p.canonicalize().expect("there"))
        );
        assert!(Cli::try_parse_from(["openregent", "--scene", &path, "--land", "KxHa"]).is_err());
        assert!(Cli::try_parse_from(["openregent", "--scene", &path, "--server"]).is_err());
    }

    /// The example scene's game, made as the command line makes it,
    /// twice: the same game frame for frame, the warrior west of the
    /// palace at the start.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_scene_game_plays_the_same_twice() {
        const FRAMES: u32 = 200;

        let path = example_scene();
        let run = || {
            let mut sim = crate::game::begin::test_game(&["--scene", &path]).0.sim;
            let world = sim.world();
            let at = |code: &[u8; 4]| {
                (0..world.next_id())
                    .find(|id| world.piece(*id).is_some_and(|p| &p.code.0 == code))
                    .and_then(|id| world.position(id))
            };
            let start = (at(b"ABJ1"), at(b"AVL1"));
            let mut digests = Vec::new();

            drive::headless(&mut sim, FRAMES, &mut None, |sim| {
                digests.push(sim.digest());
            });
            (start, digests)
        };
        let (start, first) = run();
        let (_, second) = run();

        assert_eq!(start, (Some((2048, 2048)), Some((1824, 2176))));
        assert_eq!(first.len(), FRAMES as usize);
        assert_eq!(first, second);
    }

    /// Adjust Guardhouse, Workshop item 617966615.
    const GUARDHOUSE_MOD: &str = "{D666FCBE-9906-4582-A6A0-DC96F812441E}";

    /// Adjust Guardhouse's one change, its `GuardHouse2` instance: a
    /// level 2 guardhouse is made with `Max_Guards` 2, not 1, and, as the
    /// mod copied the base game's instance, with no `upgradescript`. The
    /// game reads it from the mod's image, as the command line names it,
    /// and records the mod.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn adjust_guardhouse_raises_the_most_guards() {
        let scene = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/scenes/guardhouse.toml")
            .display()
            .to_string();
        let made = |mods: &[&str]| {
            let args: Vec<&str> = ["--scene", scene.as_str()]
                .into_iter()
                .chain(mods.iter().copied())
                .collect();
            let begun = crate::game::begin::test_game(&args).0;
            let world = begun.sim.world();
            let house = (0..world.next_id())
                .find(|id| world.piece(*id).is_some_and(|p| &p.code.0 == b"ABE2"))
                .and_then(|id| world.agent(id))
                .expect("the guardhouse's agent");
            let member = |name: &str| {
                house
                    .values
                    .iter()
                    .find(|m| m.name.eq_ignore_ascii_case(name))
                    .map(|m| m.value.clone())
            };

            (
                member("Max_Guards"),
                member("upgradescript"),
                begun.session.needs.mods,
            )
        };
        let (most, stock_upgrade, needs) = made(&[]);

        assert_eq!(most, Some(regent_core::gpl::ops::Value::Integer(1)));
        assert!(stock_upgrade.is_some());
        assert_eq!(needs.len(), 0);

        let (most, modded_upgrade, needs) = made(&["--mods", GUARDHOUSE_MOD]);

        assert_eq!(most, Some(regent_core::gpl::ops::Value::Integer(2)));
        assert_ne!(modded_upgrade, stock_upgrade);
        assert_eq!(
            needs,
            [save::Needed {
                guid: GUARDHOUSE_MOD.to_owned(),
                name: "Adjust Guardhouse".to_owned(),
                image: None,
                flags: None,
            }]
        );
    }

    /// Guards out from the scene's guardhouse after `frames`, with
    /// `mods`; upgraded, if asked, once its first guard is out.
    fn guards_after(scene: &str, mods: &[&str], upgrade: Upgrade, frames: u32) -> usize {
        let scene = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/scenes")
            .join(scene)
            .display()
            .to_string();
        let args: Vec<&str> = ["--scene", scene.as_str()]
            .into_iter()
            .chain(mods.iter().copied())
            .collect();
        let mut sim = crate::game::begin::test_game(&args).0.sim;
        let guards = |world: &regent_core::sim::world::World| {
            (0..world.next_id())
                .filter(|id| world.piece(*id).is_some())
                .filter(|id| {
                    world
                        .agent_name(*id)
                        .is_some_and(|n| n.contains("City_Guard"))
                })
                .count()
        };
        let mut asked = upgrade == Upgrade::No;

        drive::headless(&mut sim, frames, &mut None, |sim| {
            let world = sim.world();
            let house = (0..world.next_id()).find(|id| {
                world
                    .piece(*id)
                    .is_some_and(|p| p.code.0.starts_with(b"ABE"))
            });

            if let Some(id) = house.filter(|_| !asked && guards(world) == 1) {
                asked = true;
                sim.input(regent_core::sim::game::Input::Upgrade(id));
            }
        });

        guards(sim.world())
    }

    /// Whether [`guards_after`] upgrades the guardhouse.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Upgrade {
        Yes,
        No,
    }

    /// The built-in mod "Peasant houses mend themselves".
    pub const HOUSE_MEND_MOD: &str = "{A2A48A02-2BCE-42B5-8D2F-22B59DB3B720}";

    /// The scene's house's hit points after `frames`, with `mods`, and
    /// the game's script errors.
    fn house_hp_after(mods: &[&str], frames: u32) -> (i32, usize, bool) {
        let scene = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/scenes/damaged_house.toml")
            .display()
            .to_string();
        let args: Vec<&str> = ["--scene", scene.as_str()]
            .into_iter()
            .chain(mods.iter().copied())
            .collect();
        let mut sim = crate::game::begin::test_game(&args).0.sim;

        drive::headless(&mut sim, frames, &mut None, |_| {});

        let world = sim.world();

        let hp = (0..world.next_id())
            .filter_map(|id| world.piece(id))
            .find(|p| p.code.0 == *b"ABe1")
            .and_then(|p| p.attributes.get(&regent_core::data::attrib::HP.packed()))
            .copied()
            .expect("the house's hit points");

        let heals = world
            .descriptors()
            .body(regent_core::data::fourcc::FourCc(*b"ABe1"))
            .is_some_and(|body| body.heals);

        (hp, sim.reports().count, heals)
    }

    /// The built-in mod gives houses native healing, once per 5 seconds.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_house_mends_with_the_builtin_mod() {
        // 20 s of game time, day 0.
        const FRAMES: u32 = 400;

        assert_eq!(house_hp_after(&[], FRAMES), (20, 0, false));
        assert_eq!(
            house_hp_after(&["--mods", HOUSE_MEND_MOD], FRAMES),
            (24, 0, true)
        );

        // An original Majesty quest: the base game's dataset loads it too.
        let (errors, _) = script_errors(&["--quest", "QU01", "--mods", HOUSE_MEND_MOD], FRAMES);

        assert_eq!(errors.count, 0, "{:?}", errors.first);
    }

    /// The built-in mod "Bug fixes".
    pub const BUGFIXES_MOD: &str = "{07C66EB7-1379-4B7A-8328-CDA91B2AD5DB}";

    /// Whether the payout `DropGoldInRadius_SamePlayer` of the game `args`
    /// make lists `"hidden"` heroes: Aura of Peace's `"type"`.
    fn payout_lists_hidden(args: &[&str]) -> bool {
        use regent_core::gpl::exec::{Dest, Literal, Operand};

        let program = crate::game::begin::test_program(args);
        let payout = program
            .function("DropGoldInRadius_SamePlayer")
            .expect("defined");
        let hidden = |l: &Literal| matches!(l, Literal::Str(s) if program.names().get(*s).eq_ignore_ascii_case("hidden"));

        payout.code.iter().any(|i| {
            let operand = |o: &Operand| matches!(o, Operand::Value(l) if hidden(l));

            matches!(&i.dest, Dest::Value(l) if hidden(l)) || operand(&i.lhs) || operand(&i.rhs)
        })
    }

    /// The built-in mod lists hidden heroes (Aura of Peace) in the gold
    /// payout, as the expansion's explore flag already counts them.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_hidden_hero_is_paid_with_the_bugfixes_mod() {
        let plain = ["--quest", "XQ01"];
        let with = ["--quest", "XQ01", "--mods", BUGFIXES_MOD];

        assert!(!payout_lists_hidden(&plain));
        assert!(payout_lists_hidden(&with));
    }

    /// The warrior's gold after Aura of Peace hides it and 100 gold is
    /// paid by `DropGoldInRadius_SamePlayer` at the Palace beside it,
    /// with `mods` on the expansion's scene.
    fn hidden_hero_gold(mods: &[&str]) -> i32 {
        use regent_core::data::attrib::GOLD;
        use regent_core::gpl::ops::Value;

        const PAYOUT: i32 = 100;
        // 5 s of game time: the births run.
        const FRAMES: u32 = 100;

        let scene = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/scenes/hidden_hero.toml")
            .display()
            .to_string();
        let args: Vec<&str> = ["--scene", scene.as_str()]
            .into_iter()
            .chain(mods.iter().copied())
            .collect();
        let mut sim = crate::game::begin::test_game(&args).0.sim;

        drive::headless(&mut sim, FRAMES, &mut None, |_| {});

        let find = |code: &[u8; 4]| {
            let world = sim.world();

            (0..world.next_id())
                .find(|id| world.piece(*id).is_some_and(|p| p.code.0 == *code))
                .expect("placed")
        };
        let (palace, hero) = (find(b"ABJ1"), find(b"AVL1"));

        sim.call(
            "Aura_of_Peace_begin",
            &[Value::Agent(hero), Value::Agent(hero)],
        );
        sim.call(
            "DropGoldInRadius_SamePlayer",
            &[Value::Agent(palace), Value::Integer(PAYOUT)],
        );

        sim.world()
            .piece(hero)
            .and_then(|p| p.attributes.get(&GOLD.packed()))
            .copied()
            .unwrap_or(0)
    }

    /// A hero under Aura of Peace is paid by the payout only with the
    /// built-in mod "Bug fixes".
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_hidden_hero_takes_gold_with_the_bugfixes_mod() {
        let plain = hidden_hero_gold(&[]);
        let fixed = hidden_hero_gold(&["--mods", BUGFIXES_MOD]);

        assert_eq!(fixed - plain, 100, "plain {plain}, fixed {fixed}");
    }

    /// The game `args` make, played `frames` headless: its script errors,
    /// and how many records its XML made.
    fn script_errors(args: &[&str], frames: u32) -> (regent_core::sim::host::Reports, usize) {
        let (begun, catalogue) = crate::game::begin::test_game(args);
        let mut sim = begun.sim;
        let made = catalogue
            .archives
            .iter()
            .filter(|(name, _)| *name == "descriptions.xml")
            .map(|(_, a)| a.records().count())
            .sum();

        drive::headless(&mut sim, frames, &mut None, |_| {});
        (sim.reports().clone(), made)
    }

    /// Majesty Expanded, its XML mod.
    const EXPANDED_MOD: &str = "{17BF53DE-722F-4683-8E63-6B93D1606DFF}";

    /// Mods listing XML start and play without a script error: Phantoms
    /// Haunt, Improved Guards Plus, and Majesty Expanded's quests over
    /// its mod (Random Map, Sandbox Survival and War in the Sewers err
    /// without it).
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn xml_mods_play_without_script_errors() {
        // 100 s of game time.
        const FRAMES: u32 = 2000;

        let games: [&[&str]; 5] = [
            &["--mods", "{8c48289e-7c70-4426-8913-133f3544a182}"],
            &["--mods", "{C39B8C91-2185-1DD1-834E-CD212019666D}"],
            &[
                "--quest",
                "{8178F67E-EC43-4E83-90C9-21B56E859135}",
                "--mods",
                EXPANDED_MOD,
            ],
            &[
                "--quest",
                "{03A8812C-5641-4DFC-9B13-77CE79E355EE}",
                "--mods",
                EXPANDED_MOD,
            ],
            &[
                "--quest",
                "{6C8CBC3B-1163-43F9-997C-FEEEAA914CA1}",
                "--mods",
                EXPANDED_MOD,
            ],
        ];

        for args in games {
            let (reports, made) = script_errors(args, FRAMES);

            assert!(made > 0, "{args:?}: no record made from XML");
            assert_eq!(reports.count, 0, "{args:?}: {:?}", reports.first);
        }

        // Without the mod each quest errs: an attribute its XML gives.
        for quest in &games[2..] {
            let (reports, made) = script_errors(&quest[..2], FRAMES);

            assert_eq!(made, 0);
            assert!(reports.count > 0, "{quest:?} plays unmodded");
        }
    }

    /// Adjust Guardhouse gives a second guard only on an upgrade, as in
    /// HD. A guard's birth runs inside `SpawnUnit`, in the spawner's own
    /// thread run, so its `RestartGuardSpawnThread` finds the spawner
    /// running and starts nothing (gpl dispatch,
    /// gpl thread active; play3's trace nests `City_Guard_Birth`
    /// in `City_Guard_Spawner`). An upgrade's restart runs outside it.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn adjust_guardhouse_adds_a_guard_on_upgrade() {
        // 50 s of game time: the second guard comes at about 27 s.
        const FRAMES: u32 = 1000;

        let modded = ["--mods", GUARDHOUSE_MOD];

        assert_eq!(
            guards_after("guardhouse.toml", &modded, Upgrade::No, FRAMES),
            1
        );
        assert_eq!(
            guards_after("guardhouse_upgrade.toml", &[], Upgrade::Yes, FRAMES),
            1
        );
        assert_eq!(
            guards_after("guardhouse_upgrade.toml", &modded, Upgrade::Yes, FRAMES),
            2
        );
    }

    /// Majesty Expanded's copy of Barren Waste: listed after the
    /// install's quests, named by its whole id, keyed by its CRC
    /// (`quest::key_of`), titled by its `DisplayName` as `STRT QUES`
    /// holds no such key, and begun with a mod as a menu's quest is.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_mods_quest_is_named_by_its_id_and_begins() {
        const COPY: &str = "{31305551-0054-0032-0000-000000000000}";

        let install = env::Install::from_env().expect("MAJESTY_HD");
        let quests = quests(&install).expect("quests");
        let picked = |given: &str| {
            quest_of(&parsed(["openregent", "--quest", given]), &quests)
                .ok()
                .flatten()
        };
        let copy = picked(COPY).expect("the copy");

        assert!(matches!(
            quests[copy].origin,
            crate::game::load::Origin::Mod(_)
        ));
        assert_eq!(quests[copy].named(), COPY);
        assert_eq!(
            quests[picked("QU01").expect("QU01")].origin,
            crate::game::load::Origin::Install
        );

        let (begun, _) =
            crate::game::begin::test_game(&["--quest", COPY, "--mods", GUARDHOUSE_MOD]);

        let id = Guid::parse(COPY).expect("a GUID");

        assert_eq!(
            begun.session.quest,
            Some((
                regent_core::content::quest::key_of(id),
                "121 - Original Quest - Barren Waste".to_owned()
            ))
        );
        assert_eq!(begun.session.needs.mods.len(), 1);
    }

    /// `MD01`'s title, as the extras list and its briefing show it:
    /// the base `STRT QUES`'s, merged under the expansion's
    /// (`regent_core::data::text::MERGED`). **Game**:
    /// `ui_downloaded_quests.png`.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn wrath_of_krolm_keeps_its_the() {
        let install = env::Install::from_env().expect("MAJESTY_HD");
        let quests = quests(&install).expect("quests");
        let krolm = quests
            .iter()
            .find(|o| o.quest.key == FourCc(*b"MD01"))
            .expect("MD01");

        assert_eq!(krolm.title, "The Wrath of Krolm");
    }

    /// The Wild, a Workshop quest, begun as the menus begin it: its end
    /// panel says its own strings, ME Game Modifiers' over them
    /// (`regent_core::view::menu::end::verdict`).
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_workshop_quests_verdict_is_its_strings() {
        use regent_core::sim::world::defeat::End;

        const WILD: &str = "{23713EFA-0595-4A99-9890-5178D253C70D}";
        const MODIFIERS: &str = "{4233391D-0EEA-4E85-B7BD-AC0DE66DFF58}";

        let install = env::Install::from_env().expect("MAJESTY_HD");
        let quests = quests(&install).expect("quests");
        let said = |args: &[&str]| {
            let cli = parsed(std::iter::once("openregent").chain(args.iter().copied()));
            let offered = quest_of(&cli, &quests)
                .expect("its quest")
                .map(|i| &quests[i]);
            let plan = begin::Plan::of(&install, &cli, offered).expect("planned");
            let catalogue = regent_draw::data::catalogue_of(plan.sources()).expect("catalogue");
            let text = regent_draw::data::text_of(plan.sources(), &catalogue);
            let session = plan.begin(&catalogue).expect("begun").session;

            [End::Won, End::Lost].map(|end| {
                regent_core::view::menu::end::verdict(
                    &text,
                    end,
                    session.quest_key(),
                    &session.messages,
                )
            })
        };

        assert_eq!(
            said(&["--quest", WILD]),
            [
                "We have pushed back the wild beasts! Victory is ours!",
                "Our palace has fallen to the beasts... our kingdom is no more...",
            ]
        );
        assert_eq!(
            said(&["--quest", WILD, "--mods", MODIFIERS]),
            [
                "Your Majesty, we... won? I don't know how... but... we did it! Congratulations..?",
                "Our kingdom falls, but we must never forget how long we lasted against all odds!",
            ]
        );
    }

    /// The Wild names the red player's inn `SpecifyName(redinn, "REDINN")`:
    /// the string's message number is its id, so the quest's string of
    /// that id names it, as HD's panel does ("Hooligan's Haughty
    /// Hangout"); its Hooligans, `"HOOL"`, likewise ("Hooligan").
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_workshop_quests_given_name_is_its_string() {
        use regent_core::view::panel::portrait;

        const WILD: &str = "{23713EFA-0595-4A99-9890-5178D253C70D}";

        let install = env::Install::from_env().expect("MAJESTY_HD");
        let quests = quests(&install).expect("quests");
        let cli = parsed(["openregent", "--quest", WILD]);
        let offered = quest_of(&cli, &quests)
            .expect("its quest")
            .map(|i| &quests[i]);
        let plan = begin::Plan::of(&install, &cli, offered).expect("planned");
        let catalogue = regent_draw::data::catalogue_of(plan.sources()).expect("catalogue");
        let text = regent_draw::data::text_of(plan.sources(), &catalogue);
        let begun = plan.begin(&catalogue).expect("begun");
        let world = begun.sim.world();
        let named: std::collections::BTreeSet<String> = (0..world.next_id())
            .filter(|id| world.given_name(*id).is_some())
            .filter_map(|id| {
                let code = world.piece(id)?.code;

                Some(portrait::own_name(
                    &catalogue,
                    &text,
                    &portrait::who(world, id, code),
                ))
            })
            .collect();

        assert_eq!(
            named.into_iter().collect::<Vec<_>>(),
            ["Hooligan", "Hooligan's Haughty Hangout"]
        );
    }

    /// `--mods` is recorded, so a replay loads the same mods; a GUID no
    /// folder holds will not start a game; one listing XML will.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn mods_are_recorded_and_checked() {
        let cli = parsed(["openregent", "--mods", GUARDHOUSE_MOD]);
        let recorded = kept(&cli);

        assert_eq!(
            recorded
                .mods
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            [GUARDHOUSE_MOD]
        );
        assert!(Cli::try_parse_from(["openregent", "--mods", GUARDHOUSE_MOD, "--server"]).is_err());
        assert_eq!(cli.mods.len(), 1);
        assert!(
            Setup::try_from(&Cli::parse_from(["openregent", "--mods", "{nope}"]).setup).is_err()
        );

        let install = env::Install::from_env().expect("MAJESTY_HD");
        let missing = Guid::parse("{00000000-0000-0000-0000-000000000001}").expect("a GUID");
        // Majesty Expanded: XML descriptions and strings.
        let xml = Guid::parse("{17BF53DE-722F-4683-8E63-6B93D1606DFF}").expect("a GUID");

        assert!(begin::chosen(&install, &[missing.into()]).is_err_and(|e| e.contains("none of")));
        assert!(begin::chosen(&install, &[xml.into()]).is_ok_and(|m| m.len() == 1));
    }

    /// A scene on a freestyle map, its start's units cleared: only what
    /// it places stands.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_freestyle_scene_places_on_the_generated_map() {
        let dir = crate::testdata::scratch("scene");
        let path = dir.path().join("freestyle.toml");
        let text = "version = 1\nchart = \"cleared\"\n\
            map = { freestyle = { map_number = 3 } }\n\
            [[unit]]\ncode = \"ABJ1\"\nat = [40, 50]\n";

        std::fs::write(&path, text).expect("written");

        let sim = crate::game::begin::test_game(&["--scene", path.to_str().expect("utf-8")])
            .0
            .sim;
        let world = sim.world();
        let buildings: Vec<(u32, [u8; 4])> = (0..world.next_id())
            .filter_map(|id| world.piece(id).map(|p| (id, p.code.0)))
            .filter(|(_, code)| code[1] == b'B')
            .collect();

        assert_eq!(buildings.len(), 1, "{buildings:?}");
        assert_eq!(world.position(buildings[0].0), Some((40 * 32, 50 * 32)));
    }

    #[test]
    fn a_quest_is_no_custom_game() {
        let both = Cli::try_parse_from(["openregent", "--quest", "QU06", "--land", "KxHa"]);
        let listed = Cli::parse_from(["openregent", "--quest"]);

        assert!(both.is_err());
        assert_eq!(listed.setup.quest.as_deref(), Some(LIST));
    }

    #[test]
    fn a_quest_is_named_by_key_or_name_in_any_case() {
        let text = r#"<Quest id="{36305551-0000-0000-0000-000000000000}">
            <Name>DAY_OF_RECKONING</Name><Template>day_reckoning.q</Template>"#;
        let offered = [crate::game::load::Offered {
            quest: regent_core::content::quest::Quest::parse(text, Path::new("Quests"))
                .expect("quest"),
            title: "The Day of Reckoning".to_owned(),
            origin: crate::game::load::Origin::Install,
        }];
        let picked = |given: &str| {
            let cli = parsed(["openregent", "--quest", given]);

            quest_of(&cli, &offered)
        };
        let key = |given| picked(given).ok().flatten().map(|i| offered[i].quest.key);

        assert_eq!(key("QU06"), Some(FourCc::new(b"QU06")));
        assert_eq!(key("day_of_reckoning"), Some(FourCc::new(b"QU06")));
        assert!(picked("QU99").is_err_and(|e| e.contains("QU06")));
    }

    #[test]
    fn a_listing_marks_the_default() {
        let option = CodeOption::new("--land", List::Land, None, FourCc::new(b"KxHa"));
        let offers = [
            (FourCc::new(b"Rave"), "Ravenswood".to_owned()),
            (FourCc::new(b"KxHa"), "K'Hal-Kratis".to_owned()),
        ];

        assert_eq!(
            listing(&option, &offers),
            "--land codes:\n  Rave  Ravenswood\n  KxHa  K'Hal-Kratis  (default)\n"
        );
    }
}
