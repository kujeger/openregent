//! Settings, from a TOML file in the XDG config directory.
//!
//! ```text
//!  $XDG_CONFIG_HOME/openregent/config.toml   (~/.config/... by default)
//!    absent  ──> the defaults and commented optional examples, written
//!    present ──> read key by key: a missing key, or one whose value
//!                will not read or is out of [`RANGES`], takes its
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

use bevy::log::warn;
use bevy::prelude::Resource;
use regent_core::gpl::runtime;
use regent_core::view::map::pan;
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
#[derive(Resource, Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub ui: Ui,
    pub audio: Audio,
    pub game: Game,
    pub enhancements: Enhancements,
}

/// The interface.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ui {
    /// Which units show a hit point bar on the map.
    pub bars: HpBars,
    /// Whose names show under them on the map: `on` every unit the
    /// original names, `heroes` heroes alone (this game's), `off` none.
    pub names: Names,
    /// The main menu's art.
    pub main_menu: MainMenu,
    /// The start-up videos, MV02 and MV04: `on` as the original's
    /// `ShowIntro`, `off` as its `-nointro`.
    pub intro: Intro,
    /// The view's scroll in pixels: an arrow key's step, and the window
    /// edge's (20 times that a second, at full ramp). The original's
    /// `ScrollSpeed`, 32 to 64.
    pub scroll_speed: i32,
    /// `windowed` (default), or `full_screen`: borderless, the screen's
    /// size; `exclusive`: the screen's video mode taken, `full_screen`
    /// where the system offers none (Wayland). Alt+Enter flips windowed
    /// and full screen. The original's `Windowed` switch.
    pub window: WindowMode,
    /// The text's typeface: `inknut`, Inknut Antiqua, a stand-in for the
    /// original's bitmap fonts; `egui`, egui's own (Ubuntu Light).
    pub font: Typeface,
    /// A LAN game's player list in the map's top right: `off` none,
    /// `players` each player's colour and name, `details` also its round
    /// trip and the lockstep delay in force. This game's.
    pub player_list: PlayerList,
}

impl Default for Ui {
    fn default() -> Self {
        Ui {
            bars: HpBars::default(),
            names: Names::default(),
            main_menu: MainMenu::default(),
            intro: Intro::default(),
            scroll_speed: pan::SCROLL_SPEED,
            window: WindowMode::default(),
            font: Typeface::default(),
            player_list: PlayerList::default(),
        }
    }
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

/// [`Ui::scroll_speed`]'s range: `ScrollSpeedLow` to `High`
/// ([`pan::SPEEDS`]).
#[allow(clippy::cast_precision_loss, reason = "32 and 64")]
pub const SCROLL_SPEEDS: RangeInclusive<f32> =
    *pan::SPEEDS.start() as f32..=*pan::SPEEDS.end() as f32;

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
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Audio {
    /// Sound effects and voices.
    pub effects: f32,
    pub music: f32,
    /// Silences both, keeping their levels.
    pub muted: bool,
}

impl Default for Audio {
    fn default() -> Self {
        Audio {
            effects: 1.0,
            music: 1.0,
            muted: false,
        }
    }
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Game {
    /// Save the game every 1, 5 (default) or 10 game days, keeping the
    /// last 10 autosaves; `off` never. The original offers 5, 10 and 20
    /// days and keeps one.
    pub autosave: Autosave,
    /// Game speed, the original's options slider: `1` slowest, `5` the
    /// engine's (default), `9` fastest (`runtime::PACES`). Each game
    /// starts at it; `<` and `>` step it. A LAN game runs at `5`, and
    /// neither changes it there.
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

/// Departures from the original (`docs/openregent/enhancements.md`). The default is
/// [`Preset::Modern`].
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches, each a TOML key"
)]
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Enhancements {
    /// Keys 1, 2 and 3 set the game speed to 1×, 2× and 4×.
    pub speed_keys: bool,
    /// Debug helpers: L lifts the fog, O shows the models of
    /// buildings casting from one in place of their sprites.
    pub debug_keys: bool,
    /// The wheel zooms smoothly, not between the original's two levels;
    /// the gold amounts show at every zoom; a middle click or 0 goes
    /// back to 1:1.
    pub smooth_zoom: bool,
    /// Pieces slide between game frames, not jump a frame at a time.
    pub smooth_position: bool,
    /// While aiming a spell limited to a Wizards Guild's range, the range
    /// shows on the map too, not only on the minimap.
    pub show_wizards_guild_spell_range: bool,
    /// The menus' and panels' pictures upscaled with xBRZ, not filtered:
    /// sharper edges in a large window. Text and the map are untouched.
    pub ui_upscale: Upscale,
    /// The game view drawn at the art grid and scaled into the map:
    /// `nearest` sampled nearest, `sharp` sharp-bilinear (default), `fsr`
    /// AMD FSR 1. Or at the screen's pixels, each sprite smoothed:
    /// `sharp-sprite` sharp-bilinear, `fsr-sprite` that and FSR 1's RCAS.
    pub view_upscale: ViewUpscale,
    /// Videos shown larger than they are: `linear`, egui's filter, or
    /// `fsr`, AMD FSR 1 (default).
    pub video_upscale: VideoUpscale,
    /// Shadows: `soft` (default), no stacking, one strength, hue kept;
    /// `blend`, a multiply as each draws, stacking as the original's.
    pub shadows: Shadows,
    /// Each game message about a unit shows its portrait; a click selects
    /// the unit while it exists and centres the view on it or its last place.
    pub message_links: bool,
    /// With a Temple to Krolm built, its Rage of Krolm is a button on the
    /// spell bar too, darkened as it runs out.
    pub rage_on_spell_bar: bool,
    /// Selection rings: `art`, the game's pictures; `native` (default),
    /// drawn as shapes of the same size, place and look, smooth at any
    /// zoom; `modern`, a thin line with a soft glow inside it.
    pub rings: Rings,
    /// The hover ring takes a thin line in its piece's side colour, its
    /// hit point bar's: sides tell apart on unhurt units too.
    pub side_ring: bool,
    /// A held arrow key scrolls smoothly, ramping up as the window's
    /// edge does, not a step each time the key repeats.
    pub smooth_arrows: bool,
    /// The experimental keys ([`EXPERIMENTAL_KEYS`]) take effect: off, each
    /// plays as if off, its value kept.
    pub experimental: bool,
    /// Experimental: trees sway in a west wind (`regent_draw::wind`).
    pub wind: bool,
    /// Art pixels the top of a tree `regent_draw::wind::Trees::reference`
    /// tall leans at most, in [`WIND_REACHES`].
    pub wind_amplitude: f32,
    /// Experimental: the buildings' shadows, `painted` (default) into
    /// their pictures, or cast by `models` of the buildings that have one
    /// from one sun, on hills and other things too. Under soft shadows.
    pub cast: Cast,
    /// The models' sun: degrees from screen right toward the viewer, in
    /// [`SUN_DIRECTIONS`]. Shadows fall the other way.
    pub sun_direction: f32,
    /// The models' sun: degrees above the ground, in [`SUN_ELEVATIONS`].
    pub sun_elevation: f32,
    /// The models' sun: its apparent size, degrees, in [`SUN_SIZES`].
    /// A shadow is sharp where its building stands and softer farther
    /// off; 0, as sharp all along.
    pub sun_size: f32,
    /// Experimental: under `cast = "models"`, a model building takes its
    /// own model's shadow too. For sprites with their painted self-shade
    /// removed; the stock ones darken twice.
    pub self_shadows: bool,
}

impl Default for Enhancements {
    fn default() -> Self {
        Enhancements::modern()
    }
}

/// The master switch of the experimental keys.
pub const EXPERIMENTAL_SWITCH: &str = "experimental";

/// The keys [`Enhancements::experimental`] gates. No preset sets them.
pub const EXPERIMENTAL_KEYS: [&str; 7] = [
    "cast",
    "sun_direction",
    "sun_elevation",
    "sun_size",
    "self_shadows",
    "wind",
    "wind_amplitude",
];

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
    /// [`Preset::Modern`].
    #[must_use]
    pub const fn modern() -> Enhancements {
        Enhancements {
            speed_keys: true,
            debug_keys: false,
            smooth_zoom: true,
            smooth_position: true,
            show_wizards_guild_spell_range: true,
            ui_upscale: Upscale::Xbrz,
            view_upscale: ViewUpscale::Sharp,
            video_upscale: VideoUpscale::Fsr,
            shadows: Shadows::Soft,
            message_links: true,
            rage_on_spell_bar: true,
            rings: Rings::Modern,
            side_ring: true,
            smooth_arrows: true,
            experimental: false,
            wind: false,
            wind_amplitude: 2.0,
            cast: Cast::Painted,
            sun_direction: 29.0,
            sun_elevation: 50.0,
            sun_size: regent_draw::sun::cast::SIZE,
            self_shadows: false,
        }
    }

    /// [`Preset::Classic`]: every departure off. `shadows` has no off:
    /// `blend` stacks in draw order, as the original's.
    #[must_use]
    pub const fn classic() -> Enhancements {
        Enhancements {
            speed_keys: false,
            smooth_zoom: false,
            smooth_position: false,
            show_wizards_guild_spell_range: false,
            message_links: false,
            rage_on_spell_bar: false,
            smooth_arrows: false,
            ui_upscale: Upscale::None,
            view_upscale: ViewUpscale::Nearest,
            video_upscale: VideoUpscale::Linear,
            shadows: Shadows::Blend,
            rings: Rings::Art,
            side_ring: false,
            ..Enhancements::modern()
        }
    }

    /// `self` with the keys no preset sets taken from `from`: the debug
    /// keys, and the experimental ones but their switch.
    const fn keeping(self, from: &Enhancements) -> Enhancements {
        Enhancements {
            debug_keys: from.debug_keys,
            wind: from.wind,
            wind_amplitude: from.wind_amplitude,
            cast: from.cast,
            sun_direction: from.sun_direction,
            sun_elevation: from.sun_elevation,
            sun_size: from.sun_size,
            self_shadows: from.self_shadows,
            ..self
        }
    }

    /// The preset these values are: `Custom` when they match none.
    #[must_use]
    pub fn preset(&self) -> Preset {
        if Enhancements::modern().keeping(self) == *self {
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
            Preset::Modern => Enhancements::modern().keeping(self),
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

/// The numbers held to a range: section, key, range. Out of it in the
/// file, a key takes its default.
pub const RANGES: [(&str, &str, RangeInclusive<f32>); 7] = [
    ("ui", "scroll_speed", SCROLL_SPEEDS),
    ("audio", "effects", LOUDNESS),
    ("audio", "music", LOUDNESS),
    ("enhancements", "wind_amplitude", WIND_REACHES),
    ("enhancements", "sun_direction", SUN_DIRECTIONS),
    ("enhancements", "sun_elevation", SUN_ELEVATIONS),
    ("enhancements", "sun_size", SUN_SIZES),
];

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
/// read, or is out of its [`RANGES`] range, keeps its default, warned
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
    let defaults = Table::try_from(Settings::default()).map_err(|e| e.to_string())?;

    // A known section that is no table: its keys all take their default.
    for section in defaults.keys() {
        if file.get(section).is_some_and(|v| !v.is_table()) {
            warn!("settings: [{section}] is not a table; its defaults used");
        }
    }

    let keys: Vec<(String, String)> = defaults
        .iter()
        .filter_map(|(section, keys)| keys.as_table().map(|keys| (section, keys)))
        .flat_map(|(section, keys)| keys.keys().map(move |key| (section.clone(), key.clone())))
        .collect();

    // Each key tried alone over those read so far: one bad value spoils
    // no other.
    let table = keys.iter().fold(defaults, |table, (section, key)| {
        match file.get(section).and_then(|keys| keys.get(key)) {
            Some(value) => tried(table, section, key, value),
            None => table,
        }
    });

    table.try_into().map_err(|e: toml::de::Error| e.to_string())
}

/// `table` with `section`'s `key` set to `value`, if the settings read it
/// and it is in its range; else `table` as it was, warned of.
fn tried(table: Table, section: &str, key: &str, value: &Value) -> Table {
    let mut tried = table.clone();

    if let Some(Value::Table(keys)) = tried.get_mut(section) {
        keys.insert(key.to_owned(), value.clone());
    }

    let read = in_range(section, key, value) && tried.clone().try_into::<Settings>().is_ok();

    if read {
        tried
    } else {
        warn!("settings: [{section}] {key} = {value} is not valid; its default used");
        table
    }
}

/// Whether `value` is in `section`'s `key`'s [`RANGES`] range; true for
/// a key with none, or a value no number (its type is checked apart).
fn in_range(section: &str, key: &str, value: &Value) -> bool {
    let number = match value {
        Value::Float(x) => Some(*x),
        #[allow(clippy::cast_precision_loss, reason = "a setting's size")]
        Value::Integer(x) => Some(*x as f64),
        _ => None,
    };

    RANGES
        .iter()
        .find(|(s, k, _)| *s == section && *k == key)
        .zip(number)
        .is_none_or(|((_, _, range), x)| {
            (f64::from(*range.start())..=f64::from(*range.end())).contains(&x)
        })
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
    let new: toml_edit::DocumentMut = toml::to_string(settings)
        .map_err(|e| e.to_string())?
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    let mut doc: toml_edit::DocumentMut = old.parse().unwrap_or_default();

    // The values as read, and as they are now, to compare key by key.
    let was = Table::try_from(lenient(old).unwrap_or_default()).map_err(|e| e.to_string())?;
    let now = Table::try_from(settings).map_err(|e| e.to_string())?;
    let same = |section: &str, key: &str| {
        let value = |t: &Table| t.get(section).and_then(|keys| keys.get(key)).cloned();

        value(&was) == value(&now)
    };

    for (name, item) in new.iter() {
        let section = doc.entry(name).or_insert_with(toml_edit::table);

        if !section.is_table_like() {
            *section = toml_edit::table();
        }

        let pairs = item.as_table_like().into_iter().flat_map(|t| t.iter());

        if let Some(section) = section.as_table_like_mut() {
            for (key, item) in pairs {
                let there = section.get(key).is_some_and(|i| !i.is_none());

                if !(there && same(name, key)) {
                    kept(section.entry(key).or_insert(toml_edit::Item::None), item);
                }
            }
        }
    }

    Ok(doc.to_string())
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

    #[test]
    fn the_defaults_survive_a_round_trip() {
        let text = toml::to_string(&Settings::default()).expect("writes");

        assert_eq!(
            toml::from_str::<Settings>(&text).ok(),
            Some(Settings::default())
        );
    }

    #[test]
    fn a_missing_key_takes_its_default() {
        let settings: Settings =
            toml::from_str("[ui]\nmain_menu = \"expansion\"\n").expect("parses");

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
        let fast: Settings = toml::from_str("[ui]\nscroll_speed = 64\n").expect("parses");

        assert_eq!(Settings::default().ui.scroll(), 32);
        assert_eq!(fast.ui.scroll(), 64);
        assert_eq!(
            lenient("[ui]\nscroll_speed = 100\n").map(|s| s.ui.scroll()),
            Ok(32)
        );
        assert!(Settings::default().enhancements.smooth_arrows);
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
        let settings: Settings =
            toml::from_str("[enhancements]\nspeed_keys = true\n").expect("parses");

        assert!(settings.enhancements.speed_keys);
    }

    #[test]
    fn the_debug_keys_are_off_by_default() {
        assert!(!Settings::default().enhancements.debug_keys);
    }

    #[test]
    fn the_debug_keys_setting_is_read() {
        let settings: Settings =
            toml::from_str("[enhancements]\ndebug_keys = true\n").expect("parses");

        assert!(settings.enhancements.debug_keys);
    }

    #[test]
    fn smooth_zoom_is_on_by_default() {
        assert!(Settings::default().enhancements.smooth_zoom);
    }

    #[test]
    fn the_smooth_zoom_setting_is_read() {
        let settings: Settings =
            toml::from_str("[enhancements]\nsmooth_zoom = true\n").expect("parses");

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
        let settings: Settings =
            toml::from_str("[enhancements]\nsmooth_position = true\n").expect("parses");

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
        let settings: Settings =
            toml::from_str("[enhancements]\nshow_wizards_guild_spell_range = false\n")
                .expect("parses");

        assert!(!settings.enhancements.show_wizards_guild_spell_range);
    }

    #[test]
    fn the_ui_upscale_is_xbrz_by_default_and_read() {
        let settings: Settings =
            toml::from_str("[enhancements]\nui_upscale = \"none\"\n").expect("parses");

        assert_eq!(Settings::default().enhancements.ui_upscale, Upscale::Xbrz);
        assert_eq!(settings.enhancements.ui_upscale, Upscale::None);
    }

    #[test]
    fn the_view_upscale_is_sharp_by_default_and_read() {
        let settings: Settings =
            toml::from_str("[enhancements]\nview_upscale = \"fsr\"\n").expect("parses");

        assert_eq!(
            Settings::default().enhancements.view_upscale,
            ViewUpscale::Sharp
        );
        assert_eq!(settings.enhancements.view_upscale, ViewUpscale::Fsr);

        for (name, mode) in [
            ("sharp-sprite", ViewUpscale::SharpSprite),
            ("fsr-sprite", ViewUpscale::FsrSprite),
        ] {
            let settings: Settings =
                toml::from_str(&format!("[enhancements]\nview_upscale = \"{name}\"\n"))
                    .expect("parses");

            assert_eq!(settings.enhancements.view_upscale, mode);
        }
    }

    #[test]
    fn the_video_upscale_is_fsr_by_default_and_read() {
        let settings: Settings =
            toml::from_str("[enhancements]\nvideo_upscale = \"linear\"\n").expect("parses");

        assert_eq!(
            Settings::default().enhancements.video_upscale,
            VideoUpscale::Fsr
        );
        assert_eq!(settings.enhancements.video_upscale, VideoUpscale::Linear);
    }

    #[test]
    fn the_shadows_are_soft_by_default_and_read() {
        let settings: Settings =
            toml::from_str("[enhancements]\nshadows = \"blend\"\n").expect("parses");

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
        let settings: Settings =
            toml::from_str("[enhancements]\nmessage_links = false\n").expect("parses");

        assert!(Settings::default().enhancements.message_links);
        assert!(!settings.enhancements.message_links);
    }

    /// The modern rings by default; a bad value takes the default.
    #[test]
    fn the_rings_are_modern_by_default_and_read() {
        let settings: Settings =
            toml::from_str("[enhancements]\nrings = \"art\"\n").expect("parses");
        let bad = lenient("[enhancements]\nrings = \"square\"\n");

        assert_eq!(Settings::default().enhancements.rings, Rings::Modern);
        assert_eq!(settings.enhancements.rings, Rings::Art);
        assert_eq!(bad.map(|s| s.enhancements.rings), Ok(Rings::Modern));
    }

    /// The side ring on by default; off read.
    #[test]
    fn the_side_ring_is_on_by_default() {
        let settings: Settings =
            toml::from_str("[enhancements]\nside_ring = false\n").expect("parses");

        assert!(Settings::default().enhancements.side_ring);
        assert!(!settings.enhancements.side_ring);
    }

    #[test]
    fn the_native_rings_read() {
        let settings: Settings =
            toml::from_str("[enhancements]\nrings = \"native\"\n").expect("parses");

        assert_eq!(settings.enhancements.rings, Rings::Native);
    }

    #[test]
    fn the_bars_show_when_damaged_by_default() {
        assert_eq!(Settings::default().ui.bars, HpBars::Damaged);
    }

    #[test]
    fn the_bars_setting_is_read() {
        let settings: Settings = toml::from_str("[ui]\nbars = \"always\"\n").expect("parses");

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
        let settings: Settings = toml::from_str("[ui]\nnames = \"heroes\"\n").expect("parses");

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
        let read = |text| {
            toml::from_str::<Settings>(text)
                .expect("parses")
                .ui
                .main_menu
        };

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
            ..Enhancements::modern()
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
            ..Enhancements::modern()
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
        let mut settings: Settings = toml::from_str(old).expect("parses");

        settings.ui.bars = HpBars::Always;

        let text = merged(old, &settings).expect("merges");

        assert!(text.starts_with("# mine\n[ui]\n"), "{text}");
        assert!(text.contains("bars = \"always\" # hidden"), "{text}");
        assert!(text.contains("music = 0.5"), "{text}");
        assert_eq!(toml::from_str::<Settings>(&text).ok(), Some(settings));
    }

    #[test]
    fn a_save_over_nothing_writes_every_key() {
        let text = merged("", &Settings::default()).expect("merges");

        assert_eq!(
            toml::from_str::<Settings>(&text).ok(),
            Some(Settings::default())
        );
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
