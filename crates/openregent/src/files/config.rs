//! Settings, from a TOML file in the XDG config directory.
//!
//! ```text
//!  $XDG_CONFIG_HOME/openregent/config.toml   (~/.config/... by default)
//!    absent  ──> the defaults and commented optional examples, written
//!    present ──> read key by key: a missing key, or one whose value
//!                will not read or is out of its range, takes its
//!                default (the latter warned of); unknown keys and
//!                sections are ignored
//!    saved   ──> a value written only where it differs from the file's
//!                reading, or is missing: comments, unknown keys, and
//!                an unread value left as it was, stay
//!  [paths] majesty_hd ──> the HD install ([`hd_at`], `env`): read
//!                apart, never written as an active value or in the editor
//!  [paths] mods ──> more folders to find mods in ([`mods_at`], `env`):
//!                the same
//! ```
//!
//! The mod list and its toggle have their own file, `mods.toml`
//! ([`crate::files::mod_list`]), so this one stays short.

use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use crate::files::setting::{Meta, Part, Setting, Visit, VisitMut, choices, in_range};
use bevy::log::warn;
use bevy::prelude::Resource;
use regent_core::gpl::runtime;
use regent_core::view::map::pan;
use regent_derive::{Section, Sections};
pub use regent_draw::art::Upscale;
pub use regent_draw::art::fsr::VideoUpscale;
pub use regent_draw::present::ViewUpscale;
pub use regent_draw::ring::Rings;
pub use regent_draw::shadow::Shadows;
pub use regent_draw::sun::Cast;
use serde::{Deserialize, Serialize};
use toml::{Table, Value};

/// The file in [`crate::files::env::OWN`].
const FILE: &str = "config.toml";

/// Examples added only when the settings file is empty.
const OPTIONAL: &str = "\
# Override the install or add mod folders:\n\
# [paths]\n\
# majesty_hd = \"/path/to/Majesty HD\"\n\
# mods = [\"/path/to/mods\"]\n\n";

/// Every setting.
#[derive(Resource, Clone, Debug, Default, PartialEq, Sections)]
pub struct Settings {
    pub ui: Ui,
    pub audio: Audio,
    pub game: Game,
    #[section(tab = Enhancements)]
    pub enhancements: Enhancements,
}

/// The interface.
#[derive(Clone, Debug, PartialEq, Section)]
#[section(group = Interface)]
pub struct Ui {
    #[setting(
        name = "Hit point bars",
        about = "Which units show a hit point bar on the map: all, only hurt ones, or none."
    )]
    pub bars: HpBars,
    /// `heroes` is this game's.
    #[setting(name = "Unit names", about = "Whose names show under them on the map.")]
    pub names: Names,
    #[setting(
        name = "Main menu picture",
        about = "The main menu's picture: the original game's room, or the Northern Expansion's snowy one."
    )]
    pub main_menu: MainMenu,
    /// The original's `ShowIntro`, and its `-nointro`.
    #[setting(name = "Intro videos", about = "Play the opening videos at start-up.")]
    pub intro: Intro,
    /// The original's `ScrollSpeed`.
    #[setting(
        about = "How far the map scrolls, in pixels, for each press of an arrow key. At the window's edge it scrolls 20 times that a second at full speed.",
        default = pan::SCROLL_SPEED,
        range = pan::SPEEDS
    )]
    pub scroll_speed: i32,
    /// The original's `Windowed` switch.
    #[setting(
        about = "Windowed, or full screen. Full screen fills the screen without changing its mode. Exclusive full screen changes the screen's mode; where the system does not allow that (Wayland), it works as full screen. Alt+Enter switches between windowed and full screen.",
        group = Display
    )]
    pub window: WindowMode,
    #[setting(
        about = "The typeface of the text. Inknut Antiqua looks like the original game's lettering; Ubuntu Light is plainer."
    )]
    pub font: Typeface,
    /// This game's.
    #[setting(
        about = "In a LAN game, a list of the players in the map's top right corner. With timing, it also shows how long messages take to each player and back, and the delay in use."
    )]
    pub player_list: PlayerList,
    /// This game's; W, A, S and D by place on the keyboard.
    #[setting(
        name = "WASD scroll",
        about = "W, A, S and D scroll the map as the arrow keys do. Hot keys on those letters, such as upgrades, no longer work."
    )]
    pub wasd_scroll: bool,
}

/// How the window shows (`window`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowMode {
    #[default]
    Windowed,
    FullScreen,
    Exclusive,
}

impl WindowMode {
    /// Alt+Enter's: windowed to full screen, either full screen back.
    #[must_use]
    pub const fn flipped(self) -> WindowMode {
        match self {
            WindowMode::Windowed => WindowMode::FullScreen,
            WindowMode::FullScreen | WindowMode::Exclusive => WindowMode::Windowed,
        }
    }
}

impl Ui {
    /// [`Ui::scroll_speed`] held to its range.
    pub fn scroll(&self) -> i32 {
        self.scroll_speed
            .clamp(*pan::SPEEDS.start(), *pan::SPEEDS.end())
    }
}

/// Whether the start-up intro plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Intro {
    #[default]
    On,
    Off,
}

/// The main menu's art: the base game's `AP29`, as HD shows it, or the
/// expansion's snowy room under its logo cover (`docs/formats/panels.md`, the
/// panels; open question 74).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MainMenu {
    #[default]
    Base,
    Expansion,
}

/// The typeface egui draws the text in (`typeface`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Typeface {
    #[default]
    Inknut,
    Egui,
}

/// Which units show a hit point bar. The original has `always` while
/// `DisplayFlagStats` holds, else `never` (`view::map::bar::Stats`).
/// `damaged` is this game's: a unit below its most hit points, so a
/// building under construction too.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HpBars {
    Always,
    #[default]
    Damaged,
    Never,
}

/// What a LAN game's player list shows. This game's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayerList {
    Off,
    Players,
    #[default]
    Details,
}

/// Whose names show under them. The original has `on` while
/// `DisplayFlagNames` holds, its default (registry record 2, 1), else
/// `off` (`view::map::label`). `heroes` is this game's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Names {
    Off,
    #[default]
    On,
    Heroes,
}

impl Names {
    /// The minimap's names check pressed, as the original's on and off:
    /// off to `kept`, the last shown (`on` if none), else off.
    #[must_use]
    pub const fn toggled(self, kept: Names) -> Names {
        match (self, kept) {
            (Names::Off, Names::Off) => Names::On,
            (Names::Off, kept) => kept,
            _ => Names::Off,
        }
    }
}

/// A loudness: 0 mute, 1 as the engine plays it.
pub const LOUDNESS: RangeInclusive<f32> = 0.0..=1.0;

/// Loudness, each 0 (mute) to 1 (as the engine plays it).
#[derive(Clone, Debug, PartialEq, Section)]
#[section(group = Sound)]
pub struct Audio {
    #[setting(name = "Effects volume",
        about = "How loud sound effects and voices are, from 0 (silent) to 1 (full).", default = 1.0, range = LOUDNESS)]
    pub effects: f32,
    #[setting(name = "Music volume",
        about = "How loud the music is, from 0 (silent) to 1 (full).", default = 1.0, range = LOUDNESS)]
    pub music: f32,
    #[setting(name = "Mute", about = "Silence all sound. The volumes are kept.")]
    pub muted: bool,
}

impl Audio {
    /// The effects' loudness as played: 0 while muted.
    pub const fn effects_now(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.effects.clamp(*LOUDNESS.start(), *LOUDNESS.end())
        }
    }

    /// The music's loudness as played: 0 while muted.
    pub const fn music_now(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.music.clamp(*LOUDNESS.start(), *LOUDNESS.end())
        }
    }
}

/// Play.
#[derive(Clone, Debug, PartialEq, Eq, Section)]
#[section(group = Game)]
pub struct Game {
    /// The original offers 5, 10 and 20 days and keeps one.
    #[setting(
        about = "How often the game saves itself, in game days. The last 10 autosaves are kept. The original game saves every 5, 10 or 20 days and keeps one."
    )]
    pub autosave: Autosave,
    /// The original's options slider (`runtime::PACES`).
    #[setting(
        name = "Game speed",
        about = "How fast the game runs, as on the original's options slider: 1 is slowest, 5 normal, 9 fastest. Each game starts at this speed; the < and > keys change it. Network games always run at 5."
    )]
    pub speed: SpeedSlider,
}

/// The game speed's slider position (`speed`), `runtime::SLIDER`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum SpeedSlider {
    #[serde(rename = "1")]
    S1,
    #[serde(rename = "2")]
    S2,
    #[serde(rename = "3")]
    S3,
    #[serde(rename = "4")]
    S4,
    #[default]
    #[serde(rename = "5")]
    S5,
    #[serde(rename = "6")]
    S6,
    #[serde(rename = "7")]
    S7,
    #[serde(rename = "8")]
    S8,
    #[serde(rename = "9")]
    S9,
}

impl SpeedSlider {
    /// Every position, slowest first.
    const ALL: [SpeedSlider; 9] = [
        SpeedSlider::S1,
        SpeedSlider::S2,
        SpeedSlider::S3,
        SpeedSlider::S4,
        SpeedSlider::S5,
        SpeedSlider::S6,
        SpeedSlider::S7,
        SpeedSlider::S8,
        SpeedSlider::S9,
    ];

    /// Its slider position, 1 to 9.
    #[must_use]
    pub const fn position(self) -> u32 {
        self as u32 + 1
    }

    /// Times the engine's speed: the default pace over its own.
    #[must_use]
    pub fn factor(self) -> f32 {
        let pace = runtime::slider_pace(self.position()).unwrap_or(runtime::DEFAULT_PACE);

        #[allow(clippy::cast_precision_loss, reason = "a pace is a few hundred ms")]
        let factor = runtime::DEFAULT_PACE as f32 / pace as f32;

        factor
    }

    /// The position running at `factor` times the engine's speed; `None`
    /// for a factor no position has (`--speed 3`).
    #[must_use]
    pub fn of_factor(factor: f32) -> Option<SpeedSlider> {
        SpeedSlider::ALL
            .into_iter()
            .find(|s| (s.factor() - factor).abs() < f32::EPSILON)
    }
}

/// How often the game saves itself, in game days (`autosave`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum Autosave {
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "1")]
    Day,
    #[default]
    #[serde(rename = "5")]
    FiveDays,
    #[serde(rename = "10")]
    TenDays,
}

impl Autosave {
    /// Game days between autosaves; `None` off.
    pub const fn days(self) -> Option<u32> {
        match self {
            Autosave::Off => None,
            Autosave::Day => Some(1),
            Autosave::FiveDays => Some(5),
            Autosave::TenDays => Some(10),
        }
    }
}

// Each enum's choices on the settings page, in its order there, and
// their labels.
choices! {
    HpBars { Always => "Always", Damaged => "When hurt", Never => "Never" }
    Names { Off => "Off", On => "All units", Heroes => "Heroes only" }
    MainMenu { Base => "Majesty", Expansion => "Northern Expansion" }
    Intro { On => "On", Off => "Off" }
    WindowMode { Windowed => "Windowed", FullScreen => "Full screen", Exclusive => "Exclusive full screen" }
    Typeface { Inknut => "Inknut Antiqua", Egui => "Ubuntu Light" }
    PlayerList { Off => "Off", Players => "Players", Details => "Players and timing" }
    Autosave { Off => "Off", Day => "Every day", FiveDays => "Every 5 days", TenDays => "Every 10 days" }
    SpeedSlider {
        S1 => "1 (slowest)", S2 => "2", S3 => "3", S4 => "4", S5 => "5 (normal)",
        S6 => "6", S7 => "7", S8 => "8", S9 => "9 (fastest)",
    }
    Upscale { None => "Off", Xbrz => "xBRZ" }
    ViewUpscale {
        Nearest => "Off",
        Sharp => "Sharp",
        Fsr => "FSR",
        SharpSprite => "Sharp, per sprite",
        FsrSprite => "FSR, per sprite",
    }
    VideoUpscale { Linear => "Smooth", Fsr => "FSR" }
    Shadows { Blend => "Stacked", Soft => "Soft" }
    Rings { Art => "Original art", Native => "Drawn", Modern => "Modern" }
    Cast { Painted => "Painted", Models => "From models" }
}

/// Departures from the original (`docs/openregent/enhancements.md`). The
/// default is [`Preset::Modern`]; `classic` each key's [`Preset::Classic`]
/// value.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches, each a TOML key"
)]
#[derive(Clone, Debug, PartialEq, Section)]
#[section(presets)]
pub struct Enhancements {
    #[setting(
        about = "Keys 1, 2 and 3 set the game speed to normal, double and four times.",
        group = Game,
        default = true,
        classic = false
    )]
    pub speed_keys: bool,
    /// L lifts the fog; O draws the casting models in place of their sprites.
    #[setting(
        about = "Testing keys: L lifts the fog of war; O shows the 3D models that cast shadows in place of their buildings.",
        show = Hidden,
        kept
    )]
    pub debug_keys: bool,
    #[setting(
        about = "The mouse wheel zooms in small steps, not only between the original's two levels. Gold amounts show at every zoom. A middle click or 0 goes back to normal size.",
        group = Interface,
        default = true,
        classic = false
    )]
    pub smooth_zoom: bool,
    #[setting(
        about = "Units move smoothly between the game's steps, instead of jumping from one to the next.",
        group = Display,
        default = true,
        classic = false
    )]
    pub smooth_position: bool,
    #[setting(
        name = "Spell range on the map",
        about = "When you aim a spell that only works near a Wizards Guild, its range shows on the map too, not only on the minimap.",
        group = Game,
        default = true,
        classic = false
    )]
    pub show_wizards_guild_spell_range: bool,
    #[setting(
        name = "Interface upscaling",
        about = "Sharpen the pictures of the menus and panels in a large window. Text and the map do not change.",
        group = Display,
        default = Upscale::Xbrz,
        classic = Upscale::None
    )]
    pub ui_upscale: Upscale,
    #[setting(
        name = "Map upscaling",
        about = "How the map is enlarged to the window. Off repeats pixels. Sharp and FSR enlarge the whole picture. The per-sprite choices smooth each sprite at the screen's own resolution.",
        group = Display,
        default = ViewUpscale::Sharp,
        classic = ViewUpscale::Nearest
    )]
    pub view_upscale: ViewUpscale,
    #[setting(
        name = "Video upscaling",
        about = "How videos are enlarged: smooth filtering, or AMD FSR for sharper edges.",
        group = Display,
        default = VideoUpscale::Fsr,
        classic = VideoUpscale::Linear
    )]
    pub video_upscale: VideoUpscale,
    /// Classic's `blend` stacks in draw order, as the original's: shadows
    /// have no off.
    #[setting(
        about = "Soft: where shadows overlap, they do not get darker. Stacked: overlapping shadows darken each other, as in the original game.",
        group = Display,
        default = Shadows::Soft,
        classic = Shadows::Blend
    )]
    pub shadows: Shadows,
    #[setting(
        about = "Game messages about a unit show its portrait. Click it to select the unit and move the view to it, or to where it was last.",
        group = Interface,
        default = true,
        classic = false
    )]
    pub message_links: bool,
    #[setting(
        name = "Rage of Krolm on the spell bar",
        about = "With a Temple to Krolm built, Rage of Krolm also has a button on the spell bar. The button darkens as the rage runs out.",
        group = Interface,
        default = true,
        classic = false
    )]
    pub rage_on_spell_bar: bool,
    #[setting(
        name = "Selection rings",
        about = "How selection rings look. Original art uses the game's pictures. Drawn looks the same but stays smooth at any zoom. Modern is a thin line with a soft glow.",
        group = Display,
        default = Rings::Modern,
        classic = Rings::Art
    )]
    pub rings: Rings,
    #[setting(
        name = "Side colour on hover",
        about = "The ring under the unit you point at shows its side's colour, so you can tell sides apart even when no one is hurt.",
        group = Display,
        default = true,
        classic = false
    )]
    pub side_ring: bool,
    #[setting(
        name = "Smooth arrow keys",
        about = "Holding an arrow key scrolls smoothly and speeds up, like scrolling at the window's edge, instead of jumping each time the key repeats.",
        group = Interface,
        default = true,
        classic = false
    )]
    pub smooth_arrows: bool,
    /// The switch of the `gate = Experimental` keys: off, each plays as
    /// if off, its value kept.
    #[setting(
        about = "Turn on the experimental settings below. While this is off, they have no effect but keep their values.",
        gate = Switch,
        classic = false
    )]
    pub experimental: bool,
    /// `regent_draw::wind`.
    #[setting(
        about = "Trees sway in a wind from the west.",
        gate = Experimental
    )]
    pub wind: bool,
    /// Art pixels a tree `regent_draw::wind::Trees::reference` tall leans
    /// at most.
    #[setting(
        name = "Wind strength",
        about = "How far the tops of trees lean, in pixels of the game's art.",
        gate = Experimental,
        default = 2.0,
        range = WIND_REACHES
    )]
    pub wind_amplitude: f32,
    #[setting(
        name = "Building shadows",
        about = "Painted into the buildings' pictures, or cast from 3D models of the buildings by a single sun, onto hills and other things too. Only with soft shadows.",
        gate = Experimental,
        default = Cast::Painted
    )]
    pub cast: Cast,
    /// Degrees from screen right toward the viewer.
    #[setting(
        about = "Where the sun is, in degrees from the right of the screen towards you. Shadows fall the other way.",
        gate = Experimental,
        default = 29.0,
        range = SUN_DIRECTIONS
    )]
    pub sun_direction: f32,
    #[setting(
        about = "How high the sun is, in degrees above the ground.",
        gate = Experimental,
        default = 50.0,
        range = SUN_ELEVATIONS
    )]
    pub sun_elevation: f32,
    /// Degrees; 0, a shadow as sharp all along.
    #[setting(
        about = "How large the sun looks, in degrees. A larger sun makes a shadow softer the further it falls from its building; 0 keeps shadows sharp.",
        gate = Experimental,
        default = regent_draw::sun::cast::SIZE,
        range = SUN_SIZES
    )]
    pub sun_size: f32,
    /// For sprites with their painted self-shade removed.
    #[setting(
        about = "With shadows from models, each building also shades itself. Meant for pictures without painted shading: the original pictures get darker twice.",
        gate = Experimental
    )]
    pub self_shadows: bool,
}

/// A set of [`Enhancements`] picked at once. Not stored: read from the
/// values ([`Enhancements::preset`]), so any change shows as `Custom`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Modern,
    Classic,
    Custom,
}

impl Preset {
    /// The list's order.
    pub const ALL: [Preset; 3] = [Preset::Modern, Preset::Classic, Preset::Custom];

    /// Its name in the list.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Preset::Modern => "Modern",
            Preset::Classic => "Classic",
            Preset::Custom => "Custom",
        }
    }
}

impl Enhancements {
    /// The preset these values are: `Custom` when they match none.
    #[must_use]
    pub fn preset(&self) -> Preset {
        if Enhancements::default().keeping(self) == *self {
            Preset::Modern
        } else if Enhancements::classic().keeping(self) == *self {
            Preset::Classic
        } else {
            Preset::Custom
        }
    }

    /// These values with `preset` picked; `Custom` changes nothing.
    #[must_use]
    pub fn picked(&self, preset: Preset) -> Enhancements {
        match preset {
            Preset::Modern => Enhancements::default().keeping(self),
            Preset::Classic => Enhancements::classic().keeping(self),
            Preset::Custom => self.clone(),
        }
    }

    /// [`Enhancements::cast`], while experimental keys take effect.
    #[must_use]
    pub const fn cast_now(&self) -> Cast {
        if self.experimental {
            self.cast
        } else {
            Cast::Painted
        }
    }

    /// [`Enhancements::self_shadows`], while experimental keys take effect.
    #[must_use]
    pub const fn self_shadows_now(&self) -> bool {
        self.experimental && self.self_shadows
    }

    /// The wind's lean: 0 unless it and experimental keys are on.
    #[must_use]
    pub const fn wind_now(&self) -> f32 {
        if self.experimental && self.wind {
            self.wind_amplitude
        } else {
            0.0
        }
    }
}

/// [`Enhancements::wind_amplitude`]'s range: art pixels.
pub const WIND_REACHES: RangeInclusive<f32> = 0.0..=regent_draw::wind::MOST;

/// [`Enhancements::sun_direction`]'s range: degrees.
pub const SUN_DIRECTIONS: RangeInclusive<f32> = -180.0..=180.0;

/// [`Enhancements::sun_elevation`]'s range: degrees.
pub const SUN_ELEVATIONS: RangeInclusive<f32> = 10.0..=89.0;

/// [`Enhancements::sun_size`]'s range: degrees.
pub const SUN_SIZES: RangeInclusive<f32> = 0.0..=8.0;

/// Where the settings live; `None` with no config directory.
pub fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(crate::files::env::OWN).join(FILE))
}

/// The user's tree table for the wind, beside the settings.
const WIND_FILE: &str = "wind.toml";

/// The wind's trees: the shipped table, the user's [`WIND_FILE`] laid
/// over it when present. Never written; an unreadable file warns.
pub fn trees() -> regent_draw::wind::Trees {
    let path = dirs::config_dir().map(|d| d.join(crate::files::env::OWN).join(WIND_FILE));
    let text = path.filter(|p| p.exists()).and_then(|p| {
        std::fs::read_to_string(&p)
            .map_err(|e| warn!("{}: {e}", p.display()))
            .ok()
    });

    regent_draw::wind::Trees::read(text.as_deref())
}

/// The settings at [`path`]: the defaults, written there, when the file
/// is absent; defaults that will not write are warned of.
///
/// # Errors
///
/// A file that will not read or is not TOML.
pub fn load() -> Result<Settings, String> {
    load_at(path())
}

/// [`load`] of the file at `path`.
fn load_at(path: Option<PathBuf>) -> Result<Settings, String> {
    match path {
        Some(path) if path.exists() => read(&path),
        // Defaults that will not write still play: said, not fatal.
        Some(path) => {
            let settings = Settings::default();

            if let Err(e) = write(&path, &settings) {
                warn!("settings not written: {e}");
            }

            Ok(settings)
        }
        None => Ok(Settings::default()),
    }
}

/// The defaults and the commented `[paths]` example written to `path`,
/// when absent: something to fill in when no install is found.
///
/// # Errors
///
/// A file that will not write.
pub fn seed(path: &Path) -> Result<(), String> {
    if path.exists() {
        Ok(())
    } else {
        write(path, &Settings::default())
    }
}

/// `settings` written to [`path`]; nothing with no config directory.
///
/// # Errors
///
/// A file that will not write.
pub fn save(settings: &Settings) -> Result<(), String> {
    path().map_or(Ok(()), |path| write(&path, settings))
}

/// The section of the HD install's path, [`HD_KEY`]. Not a [`Settings`]
/// field, so the settings editor never offers it: a bad value there
/// would stop openregent starting. Never written either: the user adds it
/// by hand, and a save keeps it as an unknown section. A new file shows
/// commented examples.
pub const PATHS: &str = "paths";

/// The HD install's key in [`PATHS`], as `MAJESTY_HD`.
pub const HD_KEY: &str = "majesty_hd";

/// `[paths] majesty_hd` in the file at `path`: `~/` expanded, a relative
/// path from the file's folder; `None` when the file or key is absent.
///
/// # Errors
///
/// A file that will not read or is not TOML, or a value no string.
pub fn hd_at(path: &Path) -> Result<Option<PathBuf>, String> {
    let shown = |e: String| format!("{}: {e}", path.display());

    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(shown(e.to_string())),
        Ok(text) => {
            let file: Table = toml::from_str(&text).map_err(|e| shown(e.to_string()))?;
            let dir = path.parent().unwrap_or(Path::new(""));

            match file.get(PATHS) {
                None => Ok(None),
                Some(Value::Table(paths)) => match paths.get(HD_KEY) {
                    None => Ok(None),
                    Some(Value::String(hd)) => Ok(Some(expanded(dir, hd))),
                    Some(other) => {
                        Err(shown(format!("[{PATHS}] {HD_KEY} = {other} is not a path")))
                    }
                },
                Some(_) => Err(shown(format!("[{PATHS}] is not a table"))),
            }
        }
    }
}

/// The mod folders' key in [`PATHS`]: a list of folders, each searched
/// as `$XDG_DATA_HOME/openregent/mods` is (`env::mod_folders`).
pub const MODS_KEY: &str = "mods";

/// `[paths] mods` in the file at `path`, each as [`hd_at`] reads its
/// path; none when the file or key is absent. A value that is no list
/// of strings is warned of and read as none, or its strings alone: a
/// mod folder is no reason not to start.
pub fn mods_at(path: &Path) -> Vec<PathBuf> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let dir = path.parent().unwrap_or(Path::new(""));
    let file: Table = toml::from_str(&text).unwrap_or_default();
    let value = file
        .get(PATHS)
        .and_then(Value::as_table)
        .and_then(|paths| paths.get(MODS_KEY));
    let listed = match value {
        None => Vec::new(),
        Some(Value::Array(list)) => list.clone(),
        Some(other) => vec![other.clone()],
    };

    listed
        .iter()
        .filter_map(|v| match v {
            Value::String(folder) => Some(expanded(dir, folder)),
            other => {
                warn!("settings: [{PATHS}] {MODS_KEY}: {other} is not a folder; passed over");
                None
            }
        })
        .collect()
}

/// `value` with `~/` expanded; relative, from `dir`.
fn expanded(dir: &Path, value: &str) -> PathBuf {
    match (value.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => dir.join(value),
    }
}

fn read(path: &Path) -> Result<Settings, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;

    lenient(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// `text` read key by key over the defaults: a key whose value will not
/// read as its type, or is out of its range, keeps its default, warned
/// of; keys and sections the settings lack are ignored.
///
/// ```text
///  [enhancements]
///  shadows = "marks"   ──> Shadows::Soft, warned of
///  speed_keys = true   ──> read
///  old_key = 1         ──> ignored
/// ```
fn lenient(text: &str) -> Result<Settings, String> {
    let file: Table = toml::from_str(text).map_err(|e: toml::de::Error| e.to_string())?;

    // A known section that is no table: its keys all take their default.
    for part in Settings::parts() {
        if file.get(part.name).is_some_and(|v| !v.is_table()) {
            warn!(
                "settings: [{}] is not a table; its defaults used",
                part.name
            );
        }
    }

    let mut settings = Settings::default();

    settings.visit_mut(&mut Load(&file));
    Ok(settings)
}

/// Each key a file holds read over its setting: one bad value spoils no
/// other.
struct Load<'a>(&'a Table);

impl VisitMut for Load<'_> {
    fn field<T: Setting>(&mut self, part: &'static Part, meta: &'static Meta, value: &mut T) {
        let held = self
            .0
            .get(part.name)
            .and_then(Value::as_table)
            .and_then(|keys| keys.get(meta.key));

        if let Some(held) = held {
            match held.clone().try_into::<T>() {
                Ok(read) if in_range(meta, &read) => *value = read,
                _ => warn!(
                    "settings: [{}] {} = {held} is not valid; its default used",
                    part.name, meta.key
                ),
            }
        }
    }
}

fn write(path: &Path, settings: &Settings) -> Result<(), String> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let text = merged(&old, settings)?;
    let text = if old.is_empty() {
        format!("{OPTIONAL}{text}")
    } else {
        text
    };

    crate::files::store::make_parent(path)?;
    crate::files::store::replace(path, &text)
}

/// `settings` written over `old`, a file's text: a key's value replaced,
/// its comments kept, where `settings` differs from `old`'s reading
/// ([`lenient`]), or written where missing. So keys `settings` does not
/// write stay, and so does a value that did not read, unless its setting
/// changed from the default it took. Text that does not parse is
/// replaced whole.
fn merged(old: &str, settings: &Settings) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = old.parse().unwrap_or_default();
    let now = Written::of(settings)?;
    let was = Written::of(&lenient(old).unwrap_or_default())?;

    for ((part, meta, value), (_, _, before)) in now.iter().zip(&was) {
        let section = doc.entry(part.name).or_insert_with(toml_edit::table);

        if !section.is_table_like() {
            *section = toml_edit::table();
        }

        if let Some(section) = section.as_table_like_mut() {
            let there = section.get(meta.key).is_some_and(|i| !i.is_none());
            let same = value.to_string() == before.to_string();

            if !(there && same) {
                let item = section.entry(meta.key).or_insert(toml_edit::Item::None);

                kept(item, &toml_edit::Item::Value(value.clone()));
            }
        }
    }

    Ok(doc.to_string())
}

/// Each setting's value as the file writes it, in the file's order.
#[derive(Default)]
struct Written {
    values: Vec<(&'static Part, &'static Meta, toml_edit::Value)>,
    error: Option<String>,
}

impl Written {
    /// `settings`' values, or why one will not write.
    fn of(
        settings: &Settings,
    ) -> Result<Vec<(&'static Part, &'static Meta, toml_edit::Value)>, String> {
        let mut written = Written::default();

        settings.visit(&mut written);
        written.error.map_or(Ok(written.values), Err)
    }
}

impl Visit for Written {
    fn field<T: Setting>(&mut self, part: &'static Part, meta: &'static Meta, value: &T) {
        // toml's own text for the value, as a whole file writes it.
        let mut text = String::new();
        let value = value
            .serialize(toml::ser::ValueSerializer::new(&mut text))
            .map_err(|e| e.to_string())
            .and_then(|text| text.parse::<toml_edit::Value>().map_err(|e| e.to_string()));

        match value {
            Ok(value) => self.values.push((part, meta, value)),
            Err(e) => self.error = Some(format!("[{}] {}: {e}", part.name, meta.key)),
        }
    }
}

/// `old` given `new`'s value, keeping `old`'s comments and spacing.
fn kept(old: &mut toml_edit::Item, new: &toml_edit::Item) {
    match (old.as_value_mut(), new.as_value()) {
        (Some(was), Some(now)) => {
            let decor = was.decor().clone();

            *was = now.clone();
            *was.decor_mut() = decor;
        }
        _ => *old = new.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file a save over nothing writes, byte for byte.
    const DEFAULT_FILE: &str = include_str!("../../tests/fixtures/config/default.toml");

    /// A hand-edited file: comments, unknown keys and sections, `[paths]`,
    /// bad and out-of-range values, an integer for a fraction.
    const OLD_FILE: &str = include_str!("../../tests/fixtures/config/old.toml");

    /// [`OLD_FILE`] saved: its text kept, the missing keys added.
    const OLD_SAVED: &str = include_str!("../../tests/fixtures/config/old_saved.toml");

    #[test]
    fn a_save_over_nothing_writes_the_default_file() {
        assert_eq!(
            merged("", &Settings::default()).as_deref(),
            Ok(DEFAULT_FILE)
        );
    }

    /// Each value read alone: a bad or out-of-range one keeps its default.
    #[test]
    #[allow(clippy::float_cmp, reason = "read back exactly")]
    fn a_hand_edited_file_reads_key_by_key() {
        let settings = lenient(OLD_FILE).expect("parses");
        let mut expected = Settings::default();

        expected.ui.bars = HpBars::Always;
        expected.ui.window = WindowMode::FullScreen;
        expected.audio.effects = 0.3;
        expected.audio.music = 1.0;
        expected.game.speed = SpeedSlider::S7;
        expected.game.autosave = Autosave::Off;
        expected.enhancements.rings = Rings::Art;
        expected.enhancements.experimental = true;
        expected.enhancements.wind_amplitude = 3.5;

        assert_eq!(settings, expected);
    }

    /// A save of what was read keeps the file's text, byte for byte, and
    /// adds the keys it lacks; saved again, it stays.
    #[test]
    fn a_hand_edited_file_saves_with_its_text_kept() {
        let settings = lenient(OLD_FILE).expect("parses");

        assert_eq!(merged(OLD_FILE, &settings).as_deref(), Ok(OLD_SAVED));
        assert_eq!(merged(OLD_SAVED, &settings).as_deref(), Ok(OLD_SAVED));
    }

    /// One setting changed: one line of the file changed.
    #[test]
    fn a_change_rewrites_its_line_alone() {
        let mut settings = lenient(OLD_SAVED).expect("parses");

        settings.audio.effects = 0.7;
        settings.enhancements.rings = Rings::Native;

        let text = merged(OLD_SAVED, &settings).expect("merges");
        let changed: Vec<(&str, &str)> = OLD_SAVED
            .lines()
            .zip(text.lines())
            .filter(|(a, b)| a != b)
            .collect();

        assert_eq!(
            changed,
            [
                ("effects = 0.3", "effects = 0.7"),
                ("rings = \"art\"", "rings = \"native\""),
            ]
        );
        assert_eq!(OLD_SAVED.lines().count(), text.lines().count());
    }

    #[test]
    fn the_defaults_survive_a_round_trip() {
        let text = merged("", &Settings::default()).expect("writes");

        assert_eq!(lenient(&text).ok(), Some(Settings::default()));
    }

    #[test]
    fn a_missing_key_takes_its_default() {
        let settings = lenient("[ui]\nmain_menu = \"expansion\"\n").expect("parses");

        assert_eq!(
            settings,
            Settings {
                ui: Ui {
                    main_menu: MainMenu::Expansion,
                    ..Ui::default()
                },
                audio: Audio::default(),
                game: Game::default(),
                enhancements: Enhancements::default(),
            }
        );
    }

    /// Autosave is every 5 days by default; its days read, any other
    /// value keeps the default.
    #[test]
    fn the_autosave_setting_reads_its_days() {
        let every = |text: &str| lenient(text).map(|s| s.game.autosave.days());

        assert_eq!(Settings::default().game.autosave.days(), Some(5));
        assert_eq!(every("[game]\nautosave = \"1\"\n"), Ok(Some(1)));
        assert_eq!(every("[game]\nautosave = \"5\"\n"), Ok(Some(5)));
        assert_eq!(every("[game]\nautosave = \"10\"\n"), Ok(Some(10)));
        assert_eq!(every("[game]\nautosave = \"off\"\n"), Ok(None));
        assert_eq!(every("[game]\nautosave = \"7\"\n"), Ok(Some(5)));
        assert_eq!(every("[game]\nautosave = 1\n"), Ok(Some(5)));
    }

    /// Windowed by default; Alt+Enter flips it with full screen.
    #[test]
    fn the_window_is_windowed_by_default_and_flips() {
        let mode = |text: &str| lenient(text).map(|s| s.ui.window);

        assert_eq!(Settings::default().ui.window, WindowMode::Windowed);
        assert_eq!(
            mode("[ui]\nwindow = \"full_screen\"\n"),
            Ok(WindowMode::FullScreen)
        );
        assert_eq!(
            mode("[ui]\nwindow = \"exclusive\"\n"),
            Ok(WindowMode::Exclusive)
        );
        assert_eq!(mode("[ui]\nwindow = \"kiosk\"\n"), Ok(WindowMode::Windowed));
        assert_eq!(WindowMode::Windowed.flipped(), WindowMode::FullScreen);
        assert_eq!(WindowMode::FullScreen.flipped(), WindowMode::Windowed);
        assert_eq!(WindowMode::Exclusive.flipped(), WindowMode::Windowed);
    }

    /// The slider's position 5 by default, its pace the engine's; a
    /// position off the slider keeps the default.
    #[test]
    #[allow(clippy::float_cmp, reason = "the same division each side")]
    fn the_game_speed_is_the_sliders_position() {
        let speed = |text: &str| lenient(text).map(|s| s.game.speed);

        assert_eq!(Settings::default().game.speed, SpeedSlider::S5);
        assert_eq!(SpeedSlider::S5.factor(), 1.0);
        assert_eq!(SpeedSlider::S1.factor(), 50.0 / 250.0);
        assert_eq!(SpeedSlider::S9.factor(), 5.0);
        assert_eq!(speed("[game]\nspeed = \"9\"\n"), Ok(SpeedSlider::S9));
        assert_eq!(speed("[game]\nspeed = \"0\"\n"), Ok(SpeedSlider::S5));
        assert_eq!(speed("[game]\nspeed = 7\n"), Ok(SpeedSlider::S5));
        assert_eq!(SpeedSlider::of_factor(50.0 / 85.0), Some(SpeedSlider::S4));
        assert_eq!(SpeedSlider::of_factor(3.0), None);
    }

    #[test]
    fn the_scroll_speed_is_the_originals_and_held_to_its_slider() {
        let fast = lenient("[ui]\nscroll_speed = 64\n").expect("parses");

        assert_eq!(Settings::default().ui.scroll(), 32);
        assert_eq!(fast.ui.scroll(), 64);
        assert_eq!(
            lenient("[ui]\nscroll_speed = 100\n").map(|s| s.ui.scroll()),
            Ok(32)
        );
        assert!(Settings::default().enhancements.smooth_arrows);
    }

    /// WASD scrolling off by default; on read.
    #[test]
    fn wasd_scroll_is_off_by_default() {
        let settings = lenient("[ui]\nwasd_scroll = true\n").expect("parses");

        assert!(!Settings::default().ui.wasd_scroll);
        assert!(settings.ui.wasd_scroll);
    }

    /// Out of range in the file: the default, the keys in range read.
    #[test]
    fn a_value_out_of_range_takes_its_default() {
        let settings = lenient("[audio]\nmusic = -0.5\neffects = 0.25\n").expect("parses");

        assert_eq!(
            settings.audio,
            Audio {
                effects: 0.25,
                ..Audio::default()
            }
        );
    }

    #[test]
    fn the_speed_keys_are_on_by_default() {
        assert!(Settings::default().enhancements.speed_keys);
    }

    #[test]
    fn the_speed_keys_setting_is_read() {
        let settings = lenient("[enhancements]\nspeed_keys = true\n").expect("parses");

        assert!(settings.enhancements.speed_keys);
    }

    #[test]
    fn the_debug_keys_are_off_by_default() {
        assert!(!Settings::default().enhancements.debug_keys);
    }

    #[test]
    fn the_debug_keys_setting_is_read() {
        let settings = lenient("[enhancements]\ndebug_keys = true\n").expect("parses");

        assert!(settings.enhancements.debug_keys);
    }

    #[test]
    fn smooth_zoom_is_on_by_default() {
        assert!(Settings::default().enhancements.smooth_zoom);
    }

    #[test]
    fn the_smooth_zoom_setting_is_read() {
        let settings = lenient("[enhancements]\nsmooth_zoom = true\n").expect("parses");

        assert!(settings.enhancements.smooth_zoom);
    }

    /// Painted shadows by default; the models' sun read, one out of its
    /// range at its default.
    #[test]
    #[allow(clippy::float_cmp, reason = "read back exactly")]
    fn the_cast_and_sun_are_read() {
        let settings = lenient(
            "[enhancements]\ncast = \"models\"\nsun_direction = 45.0\nsun_elevation = 95.0\n",
        );

        assert_eq!(Settings::default().enhancements.cast, Cast::Painted);
        assert_eq!(
            settings.map(|s| (
                s.enhancements.cast,
                s.enhancements.sun_direction,
                s.enhancements.sun_elevation
            )),
            Ok((Cast::Models, 45.0, 50.0))
        );
    }

    /// The sun's size: read; out of its range, or not a number, at its
    /// default.
    #[test]
    #[allow(clippy::float_cmp, reason = "read back exactly")]
    fn the_sun_size_is_read() {
        let size = |text: &str| lenient(text).map(|s| s.enhancements.sun_size);
        let default = Settings::default().enhancements.sun_size;

        assert_eq!(default, regent_draw::sun::cast::SIZE);
        assert_eq!(size("[enhancements]\nsun_size = 3.5\n"), Ok(3.5));
        assert_eq!(size("[enhancements]\nsun_size = 0.0\n"), Ok(0.0));
        assert_eq!(size("[enhancements]\nsun_size = 20.0\n"), Ok(default));
        assert_eq!(size("[enhancements]\nsun_size = \"big\"\n"), Ok(default));
    }

    /// Off by default; read; a bad value takes the default.
    #[test]
    fn self_shadows_are_off_by_default_and_read() {
        let on = lenient("[enhancements]\nself_shadows = true\n");
        let bad = lenient("[enhancements]\nself_shadows = \"yes\"\n");

        assert!(!Settings::default().enhancements.self_shadows);
        assert_eq!(on.map(|s| s.enhancements.self_shadows), Ok(true));
        assert_eq!(bad.map(|s| s.enhancements.self_shadows), Ok(false));
    }

    #[test]
    fn smooth_position_is_on_by_default() {
        assert!(Settings::default().enhancements.smooth_position);
    }

    #[test]
    fn the_smooth_position_setting_is_read() {
        let settings = lenient("[enhancements]\nsmooth_position = true\n").expect("parses");

        assert!(settings.enhancements.smooth_position);
    }

    /// Off by default; a lean out of its range takes the default.
    #[test]
    #[allow(clippy::float_cmp, reason = "exact in binary")]
    fn the_wind_is_off_by_default_and_its_lean_held_to_its_range() {
        let settings = lenient("[enhancements]\nwind = true\nwind_amplitude = 3.0\n");
        let wild = lenient("[enhancements]\nwind_amplitude = 9.0\n");

        assert!(!Settings::default().enhancements.wind);
        assert_eq!(Settings::default().enhancements.wind_amplitude, 2.0);
        assert_eq!(
            settings.map(|s| (s.enhancements.wind, s.enhancements.wind_amplitude)),
            Ok((true, 3.0))
        );
        assert_eq!(wild.map(|s| s.enhancements.wind_amplitude), Ok(2.0));
    }

    #[test]
    fn the_spell_range_shows_by_default() {
        assert!(
            Settings::default()
                .enhancements
                .show_wizards_guild_spell_range
        );
    }

    #[test]
    fn the_spell_range_setting_is_read() {
        let settings =
            lenient("[enhancements]\nshow_wizards_guild_spell_range = false\n").expect("parses");

        assert!(!settings.enhancements.show_wizards_guild_spell_range);
    }

    #[test]
    fn the_ui_upscale_is_xbrz_by_default_and_read() {
        let settings = lenient("[enhancements]\nui_upscale = \"none\"\n").expect("parses");

        assert_eq!(Settings::default().enhancements.ui_upscale, Upscale::Xbrz);
        assert_eq!(settings.enhancements.ui_upscale, Upscale::None);
    }

    #[test]
    fn the_view_upscale_is_sharp_by_default_and_read() {
        let settings = lenient("[enhancements]\nview_upscale = \"fsr\"\n").expect("parses");

        assert_eq!(
            Settings::default().enhancements.view_upscale,
            ViewUpscale::Sharp
        );
        assert_eq!(settings.enhancements.view_upscale, ViewUpscale::Fsr);

        for (name, mode) in [
            ("sharp-sprite", ViewUpscale::SharpSprite),
            ("fsr-sprite", ViewUpscale::FsrSprite),
        ] {
            let settings =
                lenient(&format!("[enhancements]\nview_upscale = \"{name}\"\n")).expect("parses");

            assert_eq!(settings.enhancements.view_upscale, mode);
        }
    }

    #[test]
    fn the_video_upscale_is_fsr_by_default_and_read() {
        let settings = lenient("[enhancements]\nvideo_upscale = \"linear\"\n").expect("parses");

        assert_eq!(
            Settings::default().enhancements.video_upscale,
            VideoUpscale::Fsr
        );
        assert_eq!(settings.enhancements.video_upscale, VideoUpscale::Linear);
    }

    #[test]
    fn the_shadows_are_soft_by_default_and_read() {
        let settings = lenient("[enhancements]\nshadows = \"blend\"\n").expect("parses");

        assert_eq!(Settings::default().enhancements.shadows, Shadows::Soft);
        assert_eq!(settings.enhancements.shadows, Shadows::Blend);

        // Removed modes: the default.
        for gone in ["marks", "feather"] {
            let read = lenient(&format!("[enhancements]\nshadows = \"{gone}\"\n"));

            assert_eq!(
                read.map(|s| s.enhancements.shadows),
                Ok(Shadows::Soft),
                "{gone}"
            );
        }
    }

    #[test]
    fn the_message_links_are_on_by_default_and_read() {
        let settings = lenient("[enhancements]\nmessage_links = false\n").expect("parses");

        assert!(Settings::default().enhancements.message_links);
        assert!(!settings.enhancements.message_links);
    }

    /// The modern rings by default; a bad value takes the default.
    #[test]
    fn the_rings_are_modern_by_default_and_read() {
        let settings = lenient("[enhancements]\nrings = \"art\"\n").expect("parses");
        let bad = lenient("[enhancements]\nrings = \"square\"\n");

        assert_eq!(Settings::default().enhancements.rings, Rings::Modern);
        assert_eq!(settings.enhancements.rings, Rings::Art);
        assert_eq!(bad.map(|s| s.enhancements.rings), Ok(Rings::Modern));
    }

    /// The side ring on by default; off read.
    #[test]
    fn the_side_ring_is_on_by_default() {
        let settings = lenient("[enhancements]\nside_ring = false\n").expect("parses");

        assert!(Settings::default().enhancements.side_ring);
        assert!(!settings.enhancements.side_ring);
    }

    #[test]
    fn the_native_rings_read() {
        let settings = lenient("[enhancements]\nrings = \"native\"\n").expect("parses");

        assert_eq!(settings.enhancements.rings, Rings::Native);
    }

    #[test]
    fn the_bars_show_when_damaged_by_default() {
        assert_eq!(Settings::default().ui.bars, HpBars::Damaged);
    }

    #[test]
    fn the_bars_setting_is_read() {
        let settings = lenient("[ui]\nbars = \"always\"\n").expect("parses");

        assert_eq!(settings.ui.bars, HpBars::Always);
    }

    /// An old file's key is an unknown key, ignored: no file is migrated.
    #[test]
    fn an_older_files_keys_are_ignored() {
        let settings = lenient("[ui]\nstats = false\nbars = \"never\"\n[ring]\nscale = 1.15\n");

        assert_eq!(settings.map(|s| s.ui.bars), Ok(HpBars::Never));
    }

    /// On by default, as the original's `DisplayFlagNames`.
    #[test]
    fn the_names_show_by_default_and_are_read() {
        let settings = lenient("[ui]\nnames = \"heroes\"\n").expect("parses");

        assert_eq!(Settings::default().ui.names, Names::On);
        assert_eq!(settings.ui.names, Names::Heroes);
    }

    /// The check turns names off, and back to what showed before.
    #[test]
    fn the_names_check_toggles_off_and_back() {
        assert_eq!(Names::On.toggled(Names::On), Names::Off);
        assert_eq!(Names::Heroes.toggled(Names::Heroes), Names::Off);
        assert_eq!(Names::Off.toggled(Names::Heroes), Names::Heroes);
        assert_eq!(Names::Off.toggled(Names::On), Names::On);
        assert_eq!(Names::Off.toggled(Names::Off), Names::On);
    }

    /// Inknut by default; `egui` reads, anything else keeps the default.
    #[test]
    fn the_font_is_inknut_by_default() {
        let font = |text: &str| lenient(text).map(|s| s.ui.font);

        assert_eq!(Settings::default().ui.font, Typeface::Inknut);
        assert_eq!(font("[ui]\nfont = \"egui\"\n"), Ok(Typeface::Egui));
        assert_eq!(font("[ui]\nfont = \"comic\"\n"), Ok(Typeface::Inknut));
    }

    #[test]
    fn the_main_menu_is_the_base_games_by_default() {
        let read = |text| lenient(text).expect("parses").ui.main_menu;

        assert_eq!(Settings::default().ui.main_menu, MainMenu::Base);
        assert_eq!(
            read("[ui]\nmain_menu = \"expansion\"\n"),
            MainMenu::Expansion
        );
    }

    /// The removed weak perspective's keys: read past, kept by a save.
    #[test]
    fn the_removed_perspective_keys_are_ignored_and_kept() {
        let old =
            "[enhancements]\nperspective = true\nspeed_keys = true\n\n[perspective]\ntilt = 0.25\n";
        let settings = lenient(old).expect("parses");

        assert!(settings.enhancements.speed_keys);

        let text = merged(old, &settings).expect("merges");

        assert!(text.contains("perspective = true\n"), "{text}");
        assert!(text.contains("[perspective]\ntilt = 0.25\n"), "{text}");
    }

    #[test]
    fn an_unknown_key_or_section_is_ignored_and_kept_by_a_save() {
        let old =
            "[ui]\nbar_wdth = 260.0 # typo\nbars = \"never\"\n\n# theirs\n[mod]\nx = [1, 2]\n";
        let mut settings = lenient(old).expect("parses");

        assert_eq!(settings.ui.bars, HpBars::Never);

        settings.ui.bars = HpBars::Always;

        let text = merged(old, &settings).expect("merges");

        assert!(
            text.starts_with("[ui]\nbar_wdth = 260.0 # typo\nbars = \"always\"\n"),
            "{text}"
        );
        assert!(text.contains("\n# theirs\n[mod]\nx = [1, 2]\n"), "{text}");
        assert_eq!(lenient(&text), Ok(settings));
    }

    /// A value that will not read: its default, the other keys read.
    #[test]
    fn an_invalid_value_takes_its_default_alone() {
        let text = "[ui]\nbars = 3\nnames = \"heroes\"\n[audio]\nmusic = \"loud\"\neffects = 0.5\n[enhancements]\nspeed_keys = \"yes\"\nsmooth_zoom = true\n";
        let settings = lenient(text).expect("parses");

        assert_eq!(
            settings,
            Settings {
                ui: Ui {
                    names: Names::Heroes,
                    ..Ui::default()
                },
                audio: Audio {
                    effects: 0.5,
                    ..Audio::default()
                },
                game: Game::default(),
                enhancements: Enhancements {
                    smooth_zoom: true,
                    ..Enhancements::default()
                },
            }
        );
    }

    /// A known section that is no table: its defaults.
    #[test]
    fn a_section_that_is_no_table_takes_its_defaults() {
        let settings = lenient("ui = 3\n[audio]\nmuted = true\n").expect("parses");

        assert_eq!(settings.ui, Ui::default());
        assert!(settings.audio.muted);
    }

    /// The install's path, set by hand, survives a save; the defaults
    /// never write it.
    #[test]
    fn a_save_keeps_the_install_path() {
        let old = "[paths]\nmajesty_hd = \"~/Majesty HD\"\n";
        let mut settings = lenient(old).expect("parses");

        settings.audio.muted = true;

        let text = merged(old, &settings).expect("merges");

        assert!(
            text.contains("[paths]\nmajesty_hd = \"~/Majesty HD\"\n"),
            "{text}"
        );
        assert!(
            !merged("", &Settings::default())
                .expect("merges")
                .contains(HD_KEY)
        );
    }

    /// An invalid value stays in the file until its setting changes.
    #[test]
    fn a_save_keeps_an_invalid_value_until_changed() {
        let old = "[enhancements]\nshadows = \"marks\"\nspeed_keys = true\n";
        let mut settings = lenient(old).expect("parses");

        settings.enhancements.speed_keys = false;

        let text = merged(old, &settings).expect("merges");

        assert!(text.contains("shadows = \"marks\"\n"), "{text}");
        assert!(text.contains("speed_keys = false\n"), "{text}");

        settings.enhancements.shadows = Shadows::Blend;

        let text = merged(&text, &settings).expect("merges");

        assert!(text.contains("shadows = \"blend\"\n"), "{text}");
    }

    /// The default is Modern; each preset reads back as itself, a change
    /// as Custom, and a pick undoes it.
    #[test]
    fn a_preset_is_read_from_the_values() {
        let modern = Enhancements::default();
        let original = modern.picked(Preset::Classic);

        assert_eq!(modern.preset(), Preset::Modern);
        assert_eq!(original.preset(), Preset::Classic);

        let changed = Enhancements {
            side_ring: false,
            ..modern.clone()
        };

        assert_eq!(changed.preset(), Preset::Custom);
        assert_eq!(changed.picked(Preset::Custom), changed);
        assert_eq!(changed.picked(Preset::Modern), modern);
    }

    /// Classic is every switch off and every mode the original's.
    #[test]
    fn the_classic_preset_turns_every_departure_off() {
        let e = Enhancements::classic();

        assert!(
            ![
                e.speed_keys,
                e.smooth_zoom,
                e.smooth_position,
                e.show_wizards_guild_spell_range,
                e.message_links,
                e.rage_on_spell_bar,
                e.side_ring,
                e.smooth_arrows,
                e.experimental,
            ]
            .contains(&true)
        );
        assert_eq!(e.ui_upscale, Upscale::None);
        assert_eq!(e.view_upscale, ViewUpscale::Nearest);
        assert_eq!(e.video_upscale, VideoUpscale::Linear);
        assert_eq!(e.shadows, Shadows::Blend);
        assert_eq!(e.rings, Rings::Art);
    }

    /// Tuned experimental values and debug keys leave the preset alone and
    /// outlive a pick; the switch on is Custom, and a pick turns it off.
    #[test]
    fn presets_leave_experimental_values_alone() {
        let tuned = Enhancements {
            cast: Cast::Models,
            sun_size: 1.0,
            debug_keys: true,
            ..Enhancements::default()
        };
        let on = Enhancements {
            experimental: true,
            ..tuned.clone()
        };

        assert_eq!(tuned.preset(), Preset::Modern);
        assert_eq!(on.preset(), Preset::Custom);
        assert_eq!(on.picked(Preset::Modern), tuned);
    }

    /// With the switch off an experimental key plays as off.
    #[test]
    #[allow(clippy::float_cmp, reason = "exact in binary")]
    fn the_experimental_switch_gates_its_keys() {
        let off = Enhancements {
            cast: Cast::Models,
            self_shadows: true,
            wind: true,
            ..Enhancements::default()
        };
        let on = Enhancements {
            experimental: true,
            ..off.clone()
        };

        assert!(!Settings::default().enhancements.experimental);
        assert_eq!(off.cast_now(), Cast::Painted);
        assert!(!off.self_shadows_now());
        assert_eq!(off.wind_now(), 0.0);
        assert_eq!(on.cast_now(), Cast::Models);
        assert!(on.self_shadows_now());
        assert_eq!(on.wind_now(), on.wind_amplitude);
    }

    /// A save of what was read changes nothing: every key and section
    /// there, comments and order kept.
    #[test]
    fn a_full_round_trip_keeps_the_text() {
        let old = Settings::default();
        let full = merged("", &old).expect("merges");
        let text = format!(
            "# top\n[mod]\na = 1 # theirs\n\n{}\n[later]\nextra = \"x\"\n",
            full.replace("[ui]\n", "[ui]\n# mine\nold = 2\n")
                .replace("main_menu = \"base\"", "main_menu = 1 # bad")
                .replace("music = 1.0", "music = 1")
        );

        assert!(text.contains("main_menu = 1 # bad"), "{text}");
        assert!(text.contains("music = 1\n"), "{text}");
        let settings = lenient(&text).expect("parses");

        assert_eq!(merged(&text, &settings), Ok(text));
    }

    #[test]
    fn a_save_keeps_the_files_comments() {
        let old = "# mine\n[ui]\nbars = \"never\" # hidden\n\n[audio]\nmusic = 0.5\n";
        let mut settings = lenient(old).expect("parses");

        settings.ui.bars = HpBars::Always;

        let text = merged(old, &settings).expect("merges");

        assert!(text.starts_with("# mine\n[ui]\n"), "{text}");
        assert!(text.contains("bars = \"always\" # hidden"), "{text}");
        assert!(text.contains("music = 0.5"), "{text}");
        assert_eq!(lenient(&text).ok(), Some(settings));
    }

    #[test]
    fn a_save_over_nothing_writes_every_key() {
        let text = merged("", &Settings::default()).expect("merges");

        assert_eq!(lenient(&text).ok(), Some(Settings::default()));
        assert!(text.contains("[enhancements]"), "{text}");
    }

    #[test]
    fn a_new_file_shows_optional_settings() {
        let scratch = crate::testdata::scratch("config-optional");
        let path = scratch.path().join(FILE);
        let settings = Settings::default();

        write(&path, &settings).expect("written");
        let text = std::fs::read_to_string(&path).expect("read");

        assert!(text.starts_with(OPTIONAL), "{text}");
        assert_eq!(hd_at(&path), Ok(None));
        assert_eq!(mods_at(&path).len(), 0);
        assert_eq!(read(&path), Ok(settings.clone()));

        write(&path, &settings).expect("saved");
        assert_eq!(std::fs::read_to_string(&path).ok(), Some(text));
    }

    /// A config directory that cannot be written (here: under a file)
    /// starts the game on the defaults.
    #[test]
    fn defaults_that_will_not_write_still_load() {
        let scratch = crate::testdata::scratch("config-unwritable");
        let file = scratch.path().join("a file");

        std::fs::write(&file, "").expect("a file");

        assert_eq!(load_at(Some(file.join(FILE))), Ok(Settings::default()));
    }

    /// `[paths] mods`: each folder, `~/` expanded, relative from the
    /// file's folder; a value no string passed over.
    #[test]
    fn the_mod_folders_read_as_majesty_hd_reads() {
        let scratch = crate::testdata::scratch("config-mods");
        let dir = scratch.path();
        let file = dir.join(FILE);
        let home = dirs::home_dir().expect("a home");

        std::fs::write(
            &file,
            "[paths]\nmods = [\"~/workshop\", \"local mods\", 3, \"/abs\"]\n",
        )
        .expect("written");

        assert_eq!(
            mods_at(&file),
            [
                home.join("workshop"),
                dir.join("local mods"),
                PathBuf::from("/abs")
            ]
        );

        std::fs::write(&file, "[paths]\nmods = \"one\"\n").expect("written");
        assert_eq!(mods_at(&file), [dir.join("one")]);

        std::fs::write(&file, "[ui]\n").expect("written");
        assert_eq!(mods_at(&file).len(), 0);
        assert_eq!(mods_at(&dir.join("absent.toml")).len(), 0);
    }

    /// A list left here from before `mods.toml` is kept by a save, as any
    /// unknown section is, and not read.
    #[test]
    fn a_save_keeps_the_mod_list() {
        let old = "[theirs]\nenabled = [\"{B}\"]\n\n[[mod]]\nid = \"A\"\nenabled = true\n";
        let merged = merged(old, &Settings::default()).expect("merged");

        assert!(merged.contains("enabled = [\"{B}\"]"), "{merged}");
        assert!(merged.contains("[[mod]]\nid = \"A\""), "{merged}");
    }
}
