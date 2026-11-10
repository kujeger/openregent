//! What `MajestyHD.exe` does with each settings key: its parser case, and
//! where the value goes.
//!
//! **Binary** throughout; a reading of what a value controls is marked.
//!
//! ## A key's case is paired with it, not its position
//!
//! Each parser owns a table of `(key string, case number)` pairs, looks the
//! key up case-insensitively and switches on the number:
//!
//! ```text
//! GEINIFile        39 pairs ini key table        jump table engine_ini_cases
//! GSINIFile        43 pairs game ini key table        jump table game_ini_cases
//!                              no pair: falls back to GEINIFile
//! ```
//!
//! Position and case differ where keys were inserted beside related ones
//! but numbered on the end: `IsoCameraAngle` is position 18, case 34 in
//! HD. **Inferred**: the two pitch keys were added after
//! `ExpandCount` (case 33) and listed beside the lighting.
//!
//! ## Who reads `GOPT`
//!
//! HD loads one `GOPT`, the expansion's `mx_miscdata.cam`, parsed by a
//! `GSINIFile`. So a key in both tables stops in the game
//! table: `GOPT`'s `LocalPath` never reaches `GEINIFile`'s `LOCALPATH`.
//! Then [`PREFS`] from the registry and `MajXPrefs` override some values.
//!
//! ## Three places a setting lives
//!
//! | Table | Cases | Values go to |
//! |---|---|---|
//! | [`ENGINE`] | 39 | globals, singletons, setters |
//! | [`GAME`] | 43 | globals |
//! | [`PREFS`] | none | registry or `MajXPrefs`, each with a default |
//!
//! [`keys::DATASET`](super::keys::DATASET) has no case: its names are XML
//! elements compared in turn by dataset xml load (load) and dataset xml unload
//! (unload), and handed to the archive and bytecode loaders.

/// A keyword table a case reads its value through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Words {
    /// `TRUE`/`FALSE` for 1/0; a parser also takes a number.
    Truth,

    /// `OFF`, `ON`, `FALSE`, `TRUE`, `ENABLED`, `DISABLED`, as 0, 1, 0, 1, 0, 1.
    Switch,

    /// `8`, `16`, `24`, `32`: the bit depths `PixDepth` takes, and nothing else.
    Depths,

    /// `DirectDraw` 2, `DIB` 3: what `PixAlloc` takes.
    Allocators,
}

/// How a case reads its value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// A decimal integer (`atoi`).
    Int,

    /// A word from a keyword table, or a number.
    Keyword(Words),

    /// `%f`.
    Float,

    /// 16.16 fixed point.
    Fixed,

    /// A path or name.
    Text,

    /// Nothing: the case does not exist.
    Unread,
}

/// Where a case puts its value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    /// A 32-bit global and the file's value for it; `None` is `.bss`, zero
    /// at load unless code writes it first.
    Int { initial: Option<i32> },

    /// A one-byte global, `value != 0`.
    Flag { initial: Option<u8> },

    /// A float global.
    Float { initial: f32 },

    /// A field of a singleton.
    Field,

    /// Handed to a function.
    Call,

    /// Read and thrown away.
    Dropped,

    /// No case: the jump table sends the key to the exit. `GSINIFile`'s
    /// exit returns; it does not fall back to `GEINIFile`.
    Unhandled,
}

/// One key of a parser table, in table order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Case {
    pub key: &'static str,
    pub case: u8,
    pub value: Value,
    pub target: Target,
}

const fn case(key: &'static str, case: u8, value: Value, target: Target) -> Case {
    Case {
        key,
        case,
        value,
        target,
    }
}

const fn int(initial: i32) -> Target {
    Target::Int {
        initial: Some(initial),
    }
}

const fn bss() -> Target {
    Target::Int { initial: None }
}

const fn float(initial: f32) -> Target {
    Target::Float { initial }
}

use Target::{Call, Dropped, Field, Unhandled};
use Value::{Fixed, Float, Int, Keyword, Text, Unread};

/// `GEINIFile`'s table, in [`keys::ENGINE`](super::keys::ENGINE) order.
///
/// Readers, by global (**Binary** unless marked):
///
/// | Key | Read by |
/// |---|---|
/// | `UsePlayerColorPaletteRange` | `IsoGenUnit`. File value 1, `GOPT` sets `FALSE` |
/// | `PlayerColorPaletteRange*` | `GEPlayerColors` /, zeroed by its constructor. method sizes the range `End - Start + 1`; method ramps those palette indices from the player colour. **Inferred**: the indices a player colour recolours |
/// | `ScrollSpeedLow`, `High` | `Option4MenuController`. **Inferred**: the scroll slider's bounds |
/// | `Pix*` | `main` and the display code. `MajXPrefs` overrides them ([`PREFS`]) |
/// | `PixAlloc` | nothing: set, never read |
/// | `Light*`, `Contrast`, `Ambient` | `GE_Level`, as one block from `LightX` |
/// | `IsoCameraAngle`, `MaxSlope` | `GE_GlobalData`, into the camera. Both 55 in the file; every `GOPT` sets `MaxSlope` 45 |
/// | `ShowIntro` | intro play; `-nointro` clears it |
/// | `ExternalLogging` | `main` |
/// | `GplPseudorandom` | bit 0x10 of the flags word engine flags. Set, the frame loop game frame step reseeds the world RNG with frame + 1 each frame, and `MessageManager` seeds the GPL RNG |
/// | `PathHorizon`, `RepathLength` | the path singleton, constructor defaults 1024 and 10 |
/// | `CollisionDelay`, `ExpandCount` | `OrderMovePath` |
/// | `ParabolaGravity`, `ParabolaAngle` | `GE_OrderProcessor`, a projectile launch: the angle through the 256-entry trig table, gravity `-g / 32` in 16.16 |
/// | `DeferredSpriteLoading` | `UnitIMG`, `UnitIMGPartEntry`, `IsoGenUnit` |
pub const ENGINE: [Case; 39] = [
    case("CDPATH", 0, Text, Call),
    case("LOCALPATH", 1, Text, Call),
    case("LOADCAM", 2, Text, Call),
    case(
        "UsePlayerColorPaletteRange",
        3,
        Keyword(Words::Truth),
        Target::Flag { initial: Some(1) },
    ),
    case("PlayerColorPaletteRangeStart", 4, Int, Field),
    case("PlayerColorPaletteRangeEnd", 5, Int, Field),
    case("ScrollSpeedLow", 6, Int, int(32)),
    case("ScrollSpeedHigh", 7, Int, int(64)),
    case("PixDepth", 8, Keyword(Words::Depths), int(16)),
    case("PixWidth", 9, Int, int(640)),
    case("PixHeight", 10, Int, int(480)),
    case("PixExclusive", 11, Keyword(Words::Truth), int(1)),
    case("PixAlloc", 12, Keyword(Words::Allocators), int(2)),
    case("LightX", 13, Float, float(0.9)),
    case("LightY", 14, Float, float(0.9)),
    case("LightZ", 15, Float, float(1.0)),
    case("Contrast", 16, Float, float(1.5)),
    case("Ambient", 17, Float, float(0.4)),
    case("IsoCameraAngle", 34, Int, int(55)),
    case("MaxSlope", 35, Int, int(55)),
    case("ShowIntro", 18, Int, int(1)),
    case("MaxCYLibs", 19, Int, Dropped),
    case("ExternalLogging", 20, Keyword(Words::Truth), bss()),
    case("GPLPATH", 21, Text, Call),
    case("GplsPerFrame", 22, Int, Field),
    case("GplPseudorandom", 23, Int, Call),
    case("OdbcPath", 24, Unread, Unhandled),
    case("XmlPath", 25, Text, Call),
    case("ConstructBuffer", 26, Unread, Unhandled),
    case("HeightThreshold", 27, Unread, Unhandled),
    case("CDAudio", 28, Keyword(Words::Switch), Dropped),
    case("CDVolume", 29, Int, Call),
    case("PathHorizon", 30, Int, Field),
    case("RepathLength", 31, Int, Field),
    case("CollisionDelay", 32, Int, int(1000)),
    case("ExpandCount", 33, Int, int(4)),
    case("ParabolaGravity", 36, Int, int(8)),
    case("ParabolaAngle", 37, Int, int(60)),
    case(
        "DeferredSpriteLoading",
        38,
        Int,
        Target::Flag { initial: Some(1) },
    ),
];

/// `GSINIFile`'s table: the first 41 [`keys::EXPANSION_GAME`](super::keys::EXPANSION_GAME) keys
/// with [`keys::HD_INSERTED`](super::keys::HD_INSERTED) after `CYLibGarbageCollectionLRULimit`.
///
/// Readers, by global (**Binary** unless marked):
///
/// | Key | Read by |
/// |---|---|
/// | `QuestDirectory`, `ReportIfFrameLongerThan` | nothing: no case |
/// | `SaveGamePath`, `LocalPath`, `MajestyInstallPath` | nothing: read and dropped. So three of HD's `GOPT` paths do nothing |
/// | `GameValue1`, `GameValue2` | building price, a built Blacksmith's discount on prices, and building refund, a demolition's share; static initialisers set 0.05 and 0.125 (`world::GAME_VALUE_1`, `GAME_VALUE_2`) |
/// | `FramesPerAutosave` | the frame loop game frame step; the case also sets autosave option |
/// | `FramesPerChecksum` | the frame loop, with `-dochecksums` |
/// | `ExecutionWindow`, `TimePerFrame`, `BundleFactor` | `GSMessageManager` |
/// | `TossPlayerAfter` | `GSMessageManager`, game start |
/// | `GPLLinesPerFrame` | gpl root clear, gpl root ctor |
/// | `DefaultCDVolume`, `DefaultSFXVolume`, `DefaultVoiceVolume`, `AudibleVolumeLimit`, `MaxConcurrentSounds` | no reader found by address |
/// | `DefaultTaxRoute` | `BuildingRec`: a new building's `IsTaxed` (default tax route, 1) |
/// | `SoundViewRadius` | `GSIsoGameView`, sound radius set |
/// | `DifficultyLimit*` | pattern in difficulty band, freestyle setup: see [`DIFFICULTY_BANDS`](super::DIFFICULTY_BANDS) |
/// | `CYLibCache*`, `CYLibGarbageCollection*` | the cache code |
/// | `Language` | prefs registry load, start-up; the case stores only values below 13 |
/// | `DirectShowDecodingMethod` | read, use unknown |
/// | `BuildTreePath`, `FreeStyleIOPath` | logged, then stored as strings |
/// | `ExpansionDataPath` | logged only |
pub const GAME: [Case; 43] = [
    case("ConstructData1", 0, Int, Field),
    case("ConstructData2", 1, Fixed, Field),
    case("QuestDirectory", 2, Unread, Unhandled),
    case("SaveGamePath", 3, Text, Dropped),
    case("GameValue1", 4, Fixed, bss()),
    case("GameValue2", 5, Fixed, bss()),
    case("FramesPerAutosave", 6, Int, bss()),
    case("FramesPerChecksum", 7, Int, int(200)),
    case("ExecutionWindow", 8, Int, int(10)),
    case("TimePerFrame", 9, Int, int(50)),
    case("BundleFactor", 10, Int, int(2)),
    case("ReportIfFrameLongerThan", 11, Unread, Unhandled),
    case("TossPlayerAfter", 12, Int, int(60000)),
    case("GPLLinesPerFrame", 13, Int, int(1500)),
    case("DefaultCDVolume", 14, Int, int(60)),
    case("DefaultSFXVolume", 15, Int, int(80)),
    case("DefaultVoiceVolume", 16, Int, int(75)),
    case("AudibleVolumeLimit", 19, Int, int(0)),
    case("MaxConcurrentSounds", 20, Int, int(0)),
    case(
        "DefaultTaxRoute",
        17,
        Int,
        Target::Flag { initial: Some(1) },
    ),
    case("SoundViewRadius", 18, Float, float(1.5)),
    case("FramesPerExecutionAdjustment", 21, Int, int(500)),
    case("DifficultyLimitBeginnerLow", 22, Int, bss()),
    case("DifficultyLimitBeginnerHigh", 23, Int, int(39)),
    case("DifficultyLimitAdvancedLow", 24, Int, int(40)),
    case("DifficultyLimitAdvancedHigh", 25, Int, int(79)),
    case("DifficultyLimitExpertLow", 26, Int, int(80)),
    case("DifficultyLimitExpertHigh", 27, Int, int(100)),
    case("CYLibCacheTotal", 28, Int, bss()),
    case("CYLibCacheTILELow", 29, Int, bss()),
    case("CYLibCacheTILEHigh", 30, Int, bss()),
    case("CYLibCacheWAVELow", 31, Int, bss()),
    case("CYLibCacheWAVEHigh", 32, Int, bss()),
    case("CYLibCachePercentTotal", 33, Int, bss()),
    case("Language", 34, Int, bss()),
    case("CYLibGarbageCollectionInterval", 35, Int, int(300_000)),
    case("CYLibGarbageCollectionLRULimit", 36, Int, int(240_000)),
    case("DirectShowDecodingMethod", 37, Int, int(1)),
    case("LocalPath", 38, Text, Dropped),
    case("MajestyInstallPath", 39, Text, Dropped),
    case("BuildTreePath", 40, Text, Call),
    case("FreeStyleIOPath", 41, Text, Call),
    case("ExpansionDataPath", 42, Text, Dropped),
];

/// Where a persistent setting is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    /// `HKCU\Software\Cyberlore\Majesty Expansion`, through `CYPrefsManager`.
    Registry,

    /// `MajXPrefs`, read at start-up by prefs file load.
    File,
}

/// A persistent setting's kind and default, as its record holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Kind 4, a `u32`.
    Dword(u32),

    /// Kind 20, a string; default empty.
    Text,
}

/// One persistent setting, in record order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pref {
    pub key: &'static str,
    pub kind: Kind,
    pub store: Store,
}

const fn reg(key: &'static str, default: u32) -> Pref {
    Pref {
        key,
        kind: Kind::Dword(default),
        store: Store::Registry,
    }
}

const fn file(key: &'static str, default: u32) -> Pref {
    Pref {
        key,
        kind: Kind::Dword(default),
        store: Store::File,
    }
}

const fn text(key: &'static str, store: Store) -> Pref {
    Pref {
        key,
        kind: Kind::Text,
        store,
    }
}

const fn code(tag: [u8; 4]) -> u32 {
    u32::from_le_bytes(tag)
}

/// HD's persistent settings: no parser case, one `{name, kind, 0, default}`
/// record each at prefs records, loaded after
/// `GOPT`. The last 13 are [`keys::HD_EXTRA`](super::keys::HD_EXTRA), in `MajXPrefs`.
///
/// What the `MajXPrefs` loader prefs file load does with them (**Binary**):
///
/// | Key | Effect | Same as |
/// |---|---|---|
/// | `PixWidth`, `PixHeight` | override [`ENGINE`]'s, if at least 800 and 600. Their defaults rise to 1024×768 when the display allows | |
/// | `PixDepth` | overrides, if 16 or 32 | |
/// | `Windowed` | `PixExclusive = Windowed < 1` | `-w` |
/// | `DebugFlags` | debug flags | |
/// | `VSync` | byte vsync | |
/// | `BlitMode` | 0 GL blit, 1 DirectDraw blit, 2 DIB | `-useglblit`, `-useddblit`, `-dib` |
/// | `IntroVideo` | 0 no intro; 2 keep intro resolution; 3 blit movie to background | `-nointro`, `-keepintroresolution`, `-blitmovietobackground` |
/// | `UseD3D` | byte use | `-d3d9` |
/// | `UseGPLDebugger` | byte gpl debugger: `main` ORs 0x2180 into the flags word, and the loader mounts the SDK's `OriginalQuests/Data` and `DataMX` | `-gpldebug` |
/// | `UseDebugWindow` | byte debug window | `-debugoutwindow` |
/// | `SDKPath` | a path the loader passes on | |
/// | `MapDrag` | map drag | |
pub const PREFS: [Pref; 51] = [
    text("LastSaveGameUsed", Store::Registry),
    text("LastNameUsed", Store::Registry),
    reg("DisplayFlagNames", 1),
    reg("DisplayFlagStats", 1),
    reg("DisplayFlagTerrain", 1),
    reg("BuildingListVerbose", 1),
    reg("BuildingListShowUnavailable", 0),
    reg("SFXVolume", 80),
    reg("VoiceVolume", 75),
    reg("CDVolume", 60),
    reg("HeroListVerbose", 1),
    reg("HeroListSortType", 2),
    reg("SpellFastBook", 1),
    text("LastSessionNameUsed", Store::Registry),
    reg("HenchmenListSortType", 2),
    reg("BuildingsListSortType", 2),
    reg("ScrollSpeed", 32),
    reg("RGS1", code(*b"BEGI")),
    reg("RGS2", code(*b"BEGI")),
    reg("RGS3", code(*b"NONE")),
    reg("RGS4", code(*b"RAND")),
    reg("RGS5", 30000),
    reg("RGS6", 0),
    reg("RGS7", 0),
    reg("RGS8", code(*b"GRIT")),
    reg("RGS9", 128),
    reg("AutosaveConfig", 0),
    reg("ZoomMessageFlags", 1),
    reg("ProtocolLastUsed", 3),
    reg("WhichCinematic", 1),
    reg("AudibleVolumeLimit", 0),
    reg("MaxConcurrentSounds", 0),
    reg("SoundPlaybackFrequency", 0),
    reg("SoundPlaybackBitsPerSample", 0),
    reg("SoundPlaybackChannels", 0),
    reg("PerformanceValue1", 0),
    reg("RGS_SpecialEvent1", code(*b"NONE")),
    reg("RGS_SpecialEvent2", code(*b"NONE")),
    file("PixWidth", 800),
    file("PixHeight", 600),
    file("PixDepth", 16),
    file("Windowed", 0),
    file("DebugFlags", 0),
    file("VSync", 0),
    file("BlitMode", 0),
    file("IntroVideo", 1),
    file("UseD3D", 1),
    file("UseGPLDebugger", 0),
    file("UseDebugWindow", 0),
    text("SDKPath", Store::File),
    file("MapDrag", 1),
];

/// The case `key` has in [`ENGINE`] or [`GAME`], matched case-insensitively.
///
/// [`GAME`] first, as `GSINIFile` does, so `LocalPath` is the game
/// table's case 38.
///
/// ```
/// use regent_core::data::settings::cases::{self, Target};
///
/// assert_eq!(cases::find("IsoCameraAngle").map(|c| c.case), Some(34));
/// assert_eq!(cases::find("localpath").map(|c| c.target), Some(Target::Dropped));
/// assert_eq!(cases::find("HeightThreshold").map(|c| c.target), Some(Target::Unhandled));
/// assert!(cases::find("UseGPLDebugger").is_none());
/// ```
#[must_use]
pub fn find(key: &str) -> Option<&'static Case> {
    GAME.iter()
        .chain(ENGINE.iter())
        .find(|c| c.key.eq_ignore_ascii_case(key))
}

/// The key names of a case table, for comparison with [`keys`](super::keys).
pub fn names(table: &'static [Case]) -> impl Iterator<Item = &'static str> {
    table.iter().map(|c| c.key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::settings::keys;

    /// Every case number appears once, and the numbers are `0..n`.
    #[test]
    fn each_table_numbers_its_cases_once() {
        for table in [&ENGINE[..], &GAME[..]] {
            let mut numbers: Vec<u8> = table.iter().map(|c| c.case).collect();
            numbers.sort_unstable();

            assert!(
                numbers
                    .iter()
                    .copied()
                    .eq(0..u8::try_from(table.len()).unwrap())
            );
        }
    }

    #[test]
    fn the_engine_table_is_the_ports_run_in_order() {
        assert!(names(&ENGINE).eq(keys::ENGINE));
    }

    /// The game table is the expansion's parser keys with HD's two inserted.
    #[test]
    fn the_game_table_is_the_ports_parser_keys_and_two_more() {
        let port = &keys::EXPANSION_GAME[..keys::EXPANSION_CASES];
        let (before, after) = port.split_at(keys::GAME_CASES);
        let want = before.iter().chain(&keys::HD_INSERTED).chain(after);

        assert!(names(&GAME).eq(want.copied()));
    }

    /// Position and case part only where keys were inserted.
    #[test]
    fn position_and_case_differ_only_where_keys_were_inserted() {
        let moved = |table: &[Case]| -> Vec<&str> {
            table
                .iter()
                .enumerate()
                .filter(|(i, c)| usize::from(c.case) != *i)
                .map(|(_, c)| c.key)
                .collect()
        };

        // The pitch keys, then the sixteen after them, each two cases lower.
        let engine = moved(&ENGINE);

        assert_eq!(engine[..2], ["IsoCameraAngle", "MaxSlope"]);
        assert_eq!(engine[2..], keys::ENGINE[20..36]);
        assert_eq!(
            moved(&GAME),
            [
                "AudibleVolumeLimit",
                "MaxConcurrentSounds",
                "DefaultTaxRoute",
                "SoundViewRadius"
            ]
        );
    }

    #[test]
    fn hd_extra_is_the_file_backed_tail_of_the_persistent_table() {
        let tail = &PREFS[PREFS.len() - keys::HD_EXTRA.len()..];

        assert!(tail.iter().map(|p| p.key).eq(keys::HD_EXTRA));
        assert!(
            PREFS
                .iter()
                .all(|p| (p.store == Store::File) == tail.contains(p))
        );
    }
}
